use serde::{Deserialize, Serialize};
use lazy_static::lazy_static;

use std::fmt::{Display, Formatter, Error as FmtError};

lazy_static! {
    pub static ref FORTNITE_ANDROID_GAME_CLIENT: ClientToken = ClientToken::new("3f69e56c7649492c8cc29f1af08a8a12", "b51ee9cb12234f50a69efa67ef53812e");
    pub static ref LAUNCHER_APP_CLIENT2: ClientToken = ClientToken::new("34a02cf8f4414e29b15921876da36f9a", "daafbccc737745039dffe53d94fc76cf");
}

#[derive(Debug, Clone)]
pub struct ClientToken {
    pub client_id: String,
    pub secret: String,
    pub encoded: String
}

impl Display for ClientToken {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::result::Result<(), FmtError> {
        write!(f, "{}", self.encoded)
    }
}

impl ClientToken {
    pub(crate) fn new(client_id: &str, secret: &str) -> Self {
        let encoded = base64::encode(format!("{}:{}", client_id, secret));
        Self {
            client_id: client_id.to_owned(),
            secret: secret.to_owned(),
            encoded
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub account_id: String,
    pub device_id: String,
    pub secret: String
}

impl Device {
    pub fn new(account_id: &str, device_id: &str, secret: &str) -> Self {
        Self{
            account_id: account_id.to_string(),
            device_id: device_id.to_string(),
            secret: secret.to_string()
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeCode {
    pub code: String
}

#[derive(Debug, Deserialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: String,
    pub refresh_expires_at: String
}