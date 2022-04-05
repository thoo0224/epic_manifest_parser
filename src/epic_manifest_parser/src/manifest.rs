#![allow(dead_code)]

use string_builder::Builder as StringBuilder;
use serde_json::Value;
use miniz_oxide::inflate::decompress_to_vec_zlib;

use bytes::Buf;
use http::Uri;

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::Arc;
use std::io::{Cursor, SeekFrom, Seek, Read};

use crate::chunk::{FileManifest, FileManifestBuilder, FileChunk, FileChunkPart, ManifestContext};
use crate::{Result, http::HttpService};

const MANIFEST_HEADER_MAGIC: u32 = 0x44BEC00C;

const EMANIFEST_STORAGE_FLAG_COMPRESSED: u8 = 0x01;
const EMANIFEST_STORAGE_FLAG_ENCRYPTED: u8 = 0x02;

const EMANIFEST_META_VERSION_ORIGINAL: u8 = 0;
const EMANIFEST_META_VERSION_SERIALIZES_BUILD_ID: u8 = 1;
const EMANIFEST_META_VERSION_LATEST: u8 = EMANIFEST_META_VERSION_SERIALIZES_BUILD_ID + 1;

type ByteCursor = Cursor<Vec<u8>>;

trait CursorExt {
    fn read_fstring(&mut self) -> Result<String>;

    fn read_tarray<T, F>(&mut self, serialize: F) -> Result<Vec<T>>
    where F: Fn(&mut Self) -> T;

    fn read_sized_tarray<T, F>(&mut self, serialize: F, length: usize) -> Result<Vec<T>>
    where F: Fn(&mut Self) -> T;

}
 
impl CursorExt for Cursor<Vec<u8>> {
    fn read_fstring(&mut self) -> Result<String> {
        let length = self.get_i32_le();
        if length == 0 {
            return Ok(String::from(""));
        }

        if length < 0  {
            if length == i32::MIN {
                panic!("Archive is corrupted.")
            }

            let len = -length * 2;
            let mut buffer: Vec<u8> = vec![0; usize::try_from(len)?];
            self.read_exact(&mut buffer)?;

            //return Ok(String::from_utf8(buffer)?);
            panic!("Unicode FString's are not supported yet.");
        }

        let mut buffer = vec![0u8; usize::try_from(length)?];
        self.read_exact(&mut buffer)?;

        let buffer = buffer[0..buffer.len()-1].to_vec();

        let result = String::from_utf8(buffer)?;
        Ok(result)
    }

    fn read_tarray<T, F>(&mut self, serialize: F) -> Result<Vec<T>>
    where F: Fn(&mut Self) -> T {
        let length = self.get_i32_le();
        self.read_sized_tarray(serialize, usize::try_from(length)?)
    }

    fn read_sized_tarray<T, F>(&mut self, serialize: F, length: usize) -> Result<Vec<T>>
    where F: Fn(&mut Self) -> T {
        let mut result: Vec<T> =  Vec::with_capacity(length);
        for _ in 0..length {
            let item = serialize(self);
            result.push(item);
        }
    
        Ok(result)
    }

}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FGuid {
    pub a: u32,
    pub b: u32,
    pub c: u32,
    pub d: u32,
}

impl FGuid {

    pub fn new(reader: &mut impl Buf) -> Self {
        let a = reader.get_u32_le();
        let b = reader.get_u32_le();
        let c = reader.get_u32_le();
        let d = reader.get_u32_le();

        Self {
            a,
            b,
            c,
            d
        }
    }

}

impl Display for FGuid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:08X}{:08X}{:08X}{:08X}", self.a, self.b, self.c, self.d)
    }
}

#[derive(Debug)]
pub struct ChunkSha {
    pub data: [u8; 20]
}

impl ChunkSha {
    pub fn new<T>(reader: &mut T) -> Result<Self>
    where T: Read {
        let mut data = [0u8; 20];
        reader.read_exact(&mut data)?;

        Ok(Self {
            data
        })
    }
}

#[derive(Debug)]
pub struct ManifestInfo {
    pub app_name: String,
    pub label_name: String,
    pub build_version: String,
    pub hash: String,
    pub file_name: String,
    pub uri: String
}

impl ManifestInfo {

    pub fn new(json: &Value) -> Result<Self> {
        let root_element = json.get("elements").unwrap().as_array().unwrap().first().unwrap();
        let app_name = root_element.get("appName").unwrap().as_str().unwrap();
        let label_name = root_element.get("labelName").unwrap().as_str().unwrap();
        let build_version = root_element.get("buildVersion").unwrap().as_str().unwrap();
        let hash = root_element.get("hash").unwrap().as_str().unwrap();

        let mut uris: Vec<String> = vec![];
        let manifests = root_element.get("manifests").unwrap().as_array().unwrap();
        for manifest in manifests {
            let uri = manifest.get("uri").unwrap().as_str().unwrap();
            let mut uri_builder = StringBuilder::default();
            uri_builder.append(uri);

            if let Some(query_params_value) = manifest.get("queryParams") {
                if let Some(query_params) = query_params_value.as_array() {
                    let mut first_query = true;
                    for param_value in query_params {
                        let name = param_value.get("name").unwrap().as_str().unwrap();
                        let value = param_value.get("value").unwrap().as_str().unwrap();

                        let param = format!("{}={}", name, value);
                        if first_query {
                            uri_builder.append(format!("?{}", param));
                            first_query = false;
                        } else {
                            uri_builder.append(format!("&{}", param));
                        }
                    }
                }
            }

            uris.push(uri_builder.string()?);
        }

        let uri_str = uris.first().unwrap();
        let uri: Uri = uri_str.parse()?;
        let path = uri.path();
        let from = path.chars().count() - path.chars().rev().position(|c| c == '/').unwrap();
        let file_name = &path[from..];

        Ok(Self {
            app_name: app_name.to_owned(),
            label_name: label_name.to_owned(),
            build_version: build_version.to_owned(),
            hash: hash.to_owned(),
            file_name: file_name.to_owned(),
            uri: uri_str.clone()
         })
    }

}

#[derive(Debug)]
pub struct ManifestOptions {
    pub cache_directory: Option<String>,
    pub chunk_base_uri: String
}

impl ManifestOptions {
    pub fn new(chunk_base_uri: &str, cache_directory: Option<String>) -> Self {
        Self {
            cache_directory,
            chunk_base_uri: chunk_base_uri.to_owned()
        }
    }
}

#[derive(Debug)]
pub struct Manifest {
    pub app_id: i32,
    pub app_name: String,
    pub build_version: String,
    pub launch_exe: String,
    pub launch_command: String,
    pub prereq_ids: Vec<String>,
    pub prereq_name: String,
    pub prereq_path: String,
    pub prereq_args: String,
    pub build_id: String,
    pub chunk_hashes: HashMap<FGuid, String>,
    pub chunk_shas: HashMap<FGuid, String>,
    pub data_groups: HashMap<FGuid, u8>,
    pub chunk_filesizes: HashMap<FGuid, u64>,
    pub file_manifests: Vec<FileManifest>,
    pub custom_fields: HashMap<String, String>,
    pub context: Arc<ManifestContext>
}

#[allow(dead_code)]
impl Manifest {
    pub fn new(data: Vec<u8>, options: ManifestOptions) -> Result<Self> {
        let mut cursor = Cursor::new(data);
        let magic = cursor.get_u32_le();
        assert!(magic == MANIFEST_HEADER_MAGIC, "JSON manifests are not supported.");

        let header_size = cursor.get_i32_le();
        let _data_size_uncompressed = cursor.get_i32_le();
        let data_size_compressed = cursor.get_i32_le();
        cursor.seek(SeekFrom::Current(20))?; // Hashes

        let storage_flags = cursor.get_u8();
        let _version = cursor.get_i32_le();
        cursor.seek(SeekFrom::Start(u64::try_from(header_size)?))?;
    
        let pos = usize::try_from(cursor.position())?;
        let data = match storage_flags {
            EMANIFEST_STORAGE_FLAG_COMPRESSED => {
                let compressed = &cursor.get_mut()[pos..pos+usize::try_from(data_size_compressed)?];
                decompress_to_vec_zlib(compressed).unwrap()
            },
            EMANIFEST_STORAGE_FLAG_ENCRYPTED => {
                panic!("Encrypted manifests are not supported.");
            }
            _ => {
                let mut data = vec![0u8; 0];
                let block = &cursor.get_mut()[pos..usize::try_from(data_size_compressed)?];
                data.extend_from_slice(block);

                data
            }
        };

        let mut app_id = 0;
        let mut app_name = String::new();
        let mut build_version = String::new();
        let mut launch_exe = String::new();
        let mut launch_command = String::new();
        let mut prereq_ids: Vec<String> = vec![];
        let mut prereq_name = String::new();
        let mut prereq_path = String::new();
        let mut prereq_args = String::new();
        let mut build_id = String::new();

        let mut cursor = Cursor::new(data);
        let start_pos = cursor.position();
        let data_size = cursor.get_i32_le();
        let data_version = cursor.get_u8();
        if data_version >= EMANIFEST_META_VERSION_ORIGINAL {
            let _feature_level = cursor.get_i32();
            let _is_file_data = cursor.get_u8() != 0x00;
            app_id = cursor.get_i32_le();
            app_name = cursor.read_fstring()?;
            build_version = cursor.read_fstring()?;
            launch_exe = cursor.read_fstring()?;
            launch_command = cursor.read_fstring()?;
            prereq_ids = cursor.read_tarray(|r| r.read_fstring().unwrap())?;
            prereq_name = cursor.read_fstring()?;
            prereq_path = cursor.read_fstring()?;
            prereq_args = cursor.read_fstring()?;
        }

        if data_version >= EMANIFEST_META_VERSION_SERIALIZES_BUILD_ID {
            build_id = cursor.read_fstring()?;
        }

        let mut chunk_hashes: HashMap<FGuid, String> = HashMap::new();
        let mut chunk_shas: HashMap<FGuid, String>= HashMap::new();
        let mut data_groups: HashMap<FGuid, u8>= HashMap::new();
        let mut chunk_filesizes: HashMap<FGuid, u64> = HashMap::new();

        cursor.seek(SeekFrom::Start(start_pos + u64::try_from(data_size)?))?;
        let start_pos = cursor.position();      
        let data_size = cursor.get_i32_le();
        let data_version = cursor.get_u8();
        if data_version >= EMANIFEST_META_VERSION_ORIGINAL {
            let count = cursor.get_i32_le();
            let count_size = usize::try_from(count)?;

            let guids = cursor.read_sized_tarray(FGuid::new, count_size)?;
        
            chunk_hashes = HashMap::with_capacity(count_size);
            let hash_values = cursor.read_sized_tarray(ByteCursor::get_u64_le, count_size)?;
            for i in 0..count {
                let i = usize::try_from(i)?;
                let guid = guids[i];
                let val = hash_values[i];
                chunk_hashes.insert(guid, format!("{:016X?}",  val));
            }

            chunk_shas = HashMap::with_capacity(count_size);
            let sha_offset = usize::try_from(cursor.position())?;
            for i in 0..count {
                let i = usize::try_from(i)?;
                let guid = guids[i];
                let offset = sha_offset + (i*20);
                let data = &cursor.get_ref()[offset..offset + 20];
                chunk_shas.insert(guid, hex::encode_upper(data));
            }
            cursor.seek(SeekFrom::Current((count * 20).into()))?;

            data_groups = HashMap::with_capacity(count_size);
            let group_number_offset = cursor.position();
            let cursor_ref = cursor.get_ref();
            for i in 0..count {
                let i = usize::try_from(i)?;
                let guid = guids[i];
                data_groups.insert(guid, cursor_ref[usize::try_from(group_number_offset)? + i]);
            }
            cursor.seek(SeekFrom::Current(count.into()))?;
            cursor.seek(SeekFrom::Current((count * 4).into()))?;

            chunk_filesizes = HashMap::with_capacity(count_size);
            let file_sizes = cursor.read_sized_tarray(ByteCursor::get_u64_le, count_size)?;
            for i in 0..count {
                let i = usize::try_from(i)?;
                let guid = guids[i];
                let val = file_sizes[i];
                chunk_filesizes.insert(guid, val);
            }
        }

        let mut file_manifests_builders: Vec<FileManifestBuilder> = vec![];

        cursor.seek(SeekFrom::Start(start_pos + u64::try_from(data_size)?))?;
        let start_pos = cursor.position();
        let data_size = cursor.get_i32_le();
        let data_version = cursor.get_u8();
        if data_version >= EMANIFEST_META_VERSION_ORIGINAL {
            let count = cursor.get_i32_le();
            let count_size = usize::try_from(count)?;
            file_manifests_builders = Vec::with_capacity(count_size);

            for _ in 0..count {
                let file_name = cursor.read_fstring()?;
                file_manifests_builders.push(FileManifestBuilder::new(&file_name));
            }

            for _ in 0..count { // SymlinkTarget
                let len = cursor.get_i32_le();
                cursor.seek(SeekFrom::Current(i64::try_from(len)?))?;
            }

            let sha_offset = usize::try_from(cursor.position())?;
            for i in 0..count {
                let i = usize::try_from(i)?;
                let file = &mut file_manifests_builders[i];
                let offset = sha_offset + (i*20);
                let data = &cursor.get_ref()[offset..offset + 20];
                file.set_hash(&format!("{:X?}", data));
            }
            
            cursor.seek(SeekFrom::Current((count * 20).into()))?;
            cursor.seek(SeekFrom::Current(count.into()))?; // FileList

            for file in &mut file_manifests_builders {
                let install_tags = cursor.read_tarray(|r| r.read_fstring().unwrap())?;
                file.set_install_tags(install_tags);
            }

            for file in &mut file_manifests_builders {
                let chunk_parts = cursor.read_tarray(FileChunkPart::new)?;
                file.set_chunk_parts(chunk_parts);
            }
        }

        let mut custom_fields: HashMap<String, String> = HashMap::new();

        cursor.seek(SeekFrom::Start(start_pos + u64::try_from(data_size)?))?;
        let _start_pos = cursor.position();
        let _data_size = cursor.get_i32_le();
        let data_version = cursor.get_u8();
        if data_version > EMANIFEST_META_VERSION_ORIGINAL {
            let count = cursor.get_i32_le();
            custom_fields = HashMap::with_capacity(usize::try_from(count)?);

            let keys = cursor.read_tarray(|r| r.read_fstring().unwrap())?;
            let values = cursor.read_tarray(|r| r.read_fstring().unwrap())?;

            for i in 0..count {
                let i = usize::try_from(i)?;
                custom_fields.insert(keys[i].clone(), values[i].clone());
            }
        }

        let mut chunks: HashMap<FGuid, FileChunk> = HashMap::with_capacity(chunk_filesizes.len());
        for (guid, size) in &chunk_filesizes {
            let hash = chunk_hashes.get(guid).unwrap().clone();
            let sha = chunk_shas.get(guid).unwrap().clone();
            let data_group = data_groups.get(guid).unwrap();
            let chunk = FileChunk::new(*guid, *size, &hash, &sha, *data_group, &options.chunk_base_uri);
            chunks.insert(*guid, chunk);
        }

        let chunks = Arc::new(chunks);
        let http = Arc::new(HttpService::new());
        let context = Arc::new(ManifestContext::new(chunks, http, options.cache_directory));

        let mut file_manifests: Vec<FileManifest> = Vec::with_capacity(file_manifests_builders.len());
        for builder in file_manifests_builders {
            let manifest = builder.build(context.clone());
            file_manifests.push(manifest);
        }

        Ok(Self {
            app_id,
            app_name,
            build_version,
            launch_exe,
            launch_command,
            prereq_ids,
            prereq_name,
            prereq_path,
            prereq_args,
            build_id,
            chunk_hashes,
            chunk_shas,
            data_groups,
            chunk_filesizes,
            file_manifests,
            custom_fields,
            context
        })
    }

}