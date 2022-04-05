use miniz_oxide::inflate::decompress_to_vec_zlib;
use bytes::Buf;

use std::io::{Cursor, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::collections::HashMap;

use crate::{manifest::FGuid, http::HttpService}; // in an other file maybe?
use crate::Result;

#[derive(Debug)]
pub struct FileChunk {
    pub guid: FGuid,
    pub size: u64,
    pub hash: String,
    pub sha: String,
    pub data_group: u8,
    pub file_name: String,
    pub uri: String
}

impl FileChunk {
    pub fn new(guid: FGuid, size: u64, hash: &str, sha: &str, data_group: u8, base_url: &str) -> Self {
        let file_name = format!("{}_{}.chunk", hash, guid);
        Self {
            guid,
            size,
            hash: hash.to_owned(),
            sha: sha.to_owned(),
            data_group,
            file_name: file_name.clone(),
            uri: format!("{}{:02}/{}", base_url, data_group, file_name)
        }
    }
}

#[derive(Debug)]
pub struct FileChunkPart {
    pub guid: FGuid, 
    pub offset: i32,
    pub size: i32
}

impl FileChunkPart {
    pub fn new(reader: &mut Cursor<Vec<u8>>) -> Self {
        reader.seek(SeekFrom::Current(4)).unwrap();

        let guid = FGuid::new(reader);
        let offset = reader.get_i32_le();
        let size = reader.get_i32_le();

        Self {
            guid,
            offset,
            size
        }
    }
}

#[derive(Debug)]
pub struct ManifestContext {
    pub chunks: Arc<HashMap<FGuid, FileChunk>>,
    pub http: Arc<HttpService>,
    pub cache_dir: Option<String>
}

impl ManifestContext {
    pub fn new(chunks: Arc<HashMap<FGuid, FileChunk>>, http: Arc<HttpService>, cache_dir: Option<String>) -> Self {
        Self {
            chunks, 
            http,
             cache_dir
        }
    }
}

#[derive(Debug)]
pub struct FileManifest {
    pub name: String,
    pub hash: String,
    pub install_tags: Vec<String>,
    pub chunk_parts: Vec<FileChunkPart>,
    pub context: Arc<ManifestContext>,
    pub size: usize,
}

impl FileManifest {

    pub fn new(name: String, hash: String, install_tags: Vec<String>, chunk_parts: Vec<FileChunkPart>, context: Arc<ManifestContext>) -> Self {
        let mut size: usize = 0;
        for chunk_part in &chunk_parts {
            size += usize::try_from(chunk_part.size).unwrap_or_default();
        }

        Self {
            name,
            hash,
            install_tags, 
            chunk_parts, 
            context,
            size
        }
    }

    pub async fn save(&self) -> Result<Vec<u8>> {
        let mut result: Vec<u8> = Vec::new(); // todo

        for chunk_part in &self.chunk_parts {
            let chunk = self.context.chunks.get(&chunk_part.guid).unwrap();
            let chunk_data = self.read_chunk(chunk, self.context.http.clone()).await?;

            let offset = usize::try_from(chunk_part.offset)?;
            let size = usize::try_from(chunk_part.size)?;
            let data = &chunk_data.as_slice()[offset..offset+size];
            result.write(data)?;
        }

        Ok(result)
    }

    // todo: create dirs
    pub async fn read_chunk(&self, chunk: &FileChunk, http: Arc<HttpService>) -> Result<Vec<u8>> {
        if let Some(cache_dir) = &self.context.cache_dir {
            let mut path = PathBuf::new();
            path.push(cache_dir);
            path.push(&chunk.file_name);

            if path.as_path().exists() {
                return Ok(std::fs::read(path)?);
            }
        }

        let data = http.get(&chunk.uri).await?;
        let size = data.len();
        let mut cursor = Cursor::new(data);

        cursor.seek(SeekFrom::Start(8))?;
        let header_size = cursor.get_i32_le();

        cursor.seek(SeekFrom::Start(40))?;
        let is_compressed = cursor.get_u8() == 1;
        cursor.seek(SeekFrom::Start(u64::try_from(header_size)?))?;

        let pos_size = usize::try_from(cursor.position())?;
        let chunk_data_size = size - pos_size;
        let compressed_data = &cursor.get_ref()[pos_size..pos_size+chunk_data_size];

        let mut _result: Vec<u8> = Vec::new();
        if is_compressed {
            _result = decompress_to_vec_zlib(compressed_data).unwrap();
        } else {
            _result = compressed_data.to_vec();
        }

        if let Some(cache_dir) = &self.context.cache_dir {
            let mut path = PathBuf::new();
            path.push(cache_dir);
            path.push(&chunk.file_name);

            std::fs::write(path, &_result)?;
        }

        Ok(_result)
    }

}

pub struct FileManifestBuilder {
    pub name: String,
    pub hash: Option<String>,
    pub install_tags: Option<Vec<String>>,
    pub chunk_parts: Option<Vec<FileChunkPart>>,
    pub chunks: Option<Arc<HashMap<FGuid, FileChunk>>>,
}

impl FileManifestBuilder {

    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            hash: None,
            install_tags: None,
            chunk_parts: None,
            chunks: None
        }
    }

    pub fn set_hash(&mut self, hash: &str) -> &mut Self {
        self.hash = Some(hash.to_owned());
        self
    }

    pub fn set_install_tags(&mut self, install_tags: Vec<String>) -> &mut Self {
        self.install_tags = Some(install_tags);
        self
    }

    pub fn set_chunk_parts(&mut self, chunk_parts: Vec<FileChunkPart>) -> &mut Self {
        self.chunk_parts = Some(chunk_parts);
        self
    }

    pub fn set_chunks(&mut self, chunks: Arc<HashMap<FGuid, FileChunk>>) -> &mut Self {
        self.chunks = Some(chunks);
        self
    }

    pub fn build(self, context: Arc<ManifestContext>) -> FileManifest {
        FileManifest::new(
            self.name, 
            self.hash.unwrap_or_default(), 
            self.install_tags.unwrap_or_default(), 
            self.chunk_parts.unwrap_or_default(),
            context)
    }

}