use serde::{Deserialize, Serialize};

use std::fmt::{Display, Formatter, Error as FmtError};

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
    pub fn new(client_id: &str, secret: &str) -> Self {
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