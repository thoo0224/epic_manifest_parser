use std::path::{Path, PathBuf};

use log::Level;
use simple_logger::init_with_level;

use epic_manifest_parser::manifest::{Manifest, ManifestOptions};
use epic_manifest_parser::{EpicGamesClient, Result};
use epic_manifest_parser::auth::{Device, ClientToken};

use lazy_static::lazy_static;

const MANIFESTINFO_URL: &str =
     "https://launcher-public-service-prod06.ol.epicgames.com/launcher/api/public/assets/v2/platform/Windows/namespace/9773aa1aa54f4f7b80e44bef04986cea/catalogItem/530145df28a24424923f5828cc9031a1/app/Sugar/label/Live";

const CHUNK_BASE_URI: &str = 
    "http://epicgames-download1.akamaized.net/Builds/Org/o-98larctxyhn55kqjq5xjb9wzjl9hf9/e6bcca5b37d0457ca881aec508205542/default/ChunksV4/";

lazy_static! {
    pub static ref FORTNITE_ANDROID_GAME_CLIENT: ClientToken = ClientToken::new("3f69e56c7649492c8cc29f1af08a8a12", "b51ee9cb12234f50a69efa67ef53812e");
    pub static ref LAUNCHER_APP_CLIENT2: ClientToken = ClientToken::new("34a02cf8f4414e29b15921876da36f9a", "daafbccc737745039dffe53d94fc76cf");
}

#[tokio::main]
async fn main() -> Result<()> {
    init_with_level(Level::Info)?;

    dotenv::dotenv()?;

    let account_id = dotenv::var("ACCOUNT_ID")?;
    let device_id = dotenv::var("DEVICE_ID")?;
    let secret = dotenv::var("SECRET")?;
    let device = Device::new(&account_id, &device_id, &secret);

    let mut client = EpicGamesClient::new();
    client.authenticate_with_device(&device, &FORTNITE_ANDROID_GAME_CLIENT.clone()).await?;
    let _exchange_auth = client.authenticate_with_exchange(&LAUNCHER_APP_CLIENT2.clone()).await?;
    let manifest_info = client.get_manifest_info_authenticated(MANIFESTINFO_URL).await?;
    let manifest_data = client.download_manifest_async(&manifest_info, Some("cached_chunks")).await?;
    
    log::info!("Parsing manifest");
    let manifest = Manifest::new(manifest_data, ManifestOptions::new(CHUNK_BASE_URI, Some(String::from("cached_chunks"))))?;
    log::info!("Done.");

    for file in manifest.file_manifests.into_iter().filter(|f| f.name.ends_with("T_SF.upk")) {
        let file_name = Path::new(&file.name).file_name().unwrap().to_str().unwrap();
        let data = file.save().await.unwrap();
        log::info!("downloaded {:?}", file_name);
    
        let mut output = PathBuf::new();
        output.push("output");
        output.push(file_name);
        std::fs::write(output, data).unwrap();
    }
    
    loop { }
}