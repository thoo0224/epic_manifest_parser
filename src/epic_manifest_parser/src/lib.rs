#![allow(clippy::must_use_candidate,
         clippy::module_name_repetitions,
         clippy::missing_panics_doc,
         clippy::missing_errors_doc,
         clippy::absurd_extreme_comparisons,
         clippy::unreadable_literal,
         clippy::too_many_lines)]

use hyper::client::{Client, HttpConnector};
use hyper::{Request, Method, Body, Response};
use hyper::body::{Buf, HttpBody};
use hyper_tls::HttpsConnector;
use manifest::ManifestInfo;
use serde::Deserialize;

use std::fmt::Display;
use std::path::{PathBuf, Path};

pub mod chunk;
pub mod manifest;
pub mod auth;
mod http;

use crate::auth::{ClientToken, Device, AuthResponse, ExchangeCode};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const ACCOUNT_PUBLIC_SERVICE: &str = "https://account-public-service-prod.ol.epicgames.com";

#[derive(Debug)]
pub struct ParserError  {
    message: String
}

impl ParserError {
    pub fn new(message: &str) -> Self {
        Self {
            message: message.to_owned()
        }
    }
}

impl std::error::Error for ParserError { }

impl Display for ParserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}


#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpicError {
    pub error_code: String,
    pub error_message: String,
}

impl Display for EpicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.error_code, self.error_message)
    }
}

impl std::error::Error for EpicError { }

// todo: httpservice
pub struct EpicGamesClient {
    client: Client<HttpsConnector<HttpConnector>>,
    auth: Option<AuthResponse>
}

impl EpicGamesClient {

    pub fn new() -> Self {
        let client = Client::builder()
            .build::<_, hyper::Body>(HttpsConnector::new());
        Self {
            client,
            auth: None
        }
    }

    /// # Errors
    /// 
    /// Will return `Err` if the request was not successful or if the client is not authenticated yet
    pub async fn get_manifest_info_authenticated(&self, url: &str) -> Result<ManifestInfo> {
        self.requires_authentication()?;

        let request = Request::builder()
            .uri(url)
            .header("Authorization", self.get_authentication_header())
            .body(Body::empty())?;

        let response = self.client.request(request).await?;
        let data = Self::process_response(response).await?;
        let json = &serde_json::from_reader(data.reader())?;

        Ok(ManifestInfo::new(json)?)
    }


    /// # Errors
    /// 
    /// Will return `Err` if the request was not successful
    pub async fn get_manifest_info(&self, _url: &str) -> Result<()> {
        todo!()
    }

    /// # Errors
    /// 
    /// Will return `Err` if the request was not successful
    pub async fn authenticate_with_device(&mut self, device: &Device, client_token: &ClientToken) -> Result<&AuthResponse> {
        self.set_authentication(self.authenticate(client_token, 
            &[("grant_type", "device_auth"),
             ("account_id", &device.account_id),
             ("device_id", &device.device_id),
             ("secret", &device.secret)]).await?);

        Ok(self.auth.as_ref().unwrap())
    }

    /// # Errors
    /// 
    /// Will return `Err` if the request was not successful
    pub async fn authenticate_with_exchange(&mut self, client_token: &ClientToken) -> Result<&AuthResponse> {
        let exchange = self.get_exchange_code().await?;
        self.set_authentication(self.authenticate(client_token, 
            &[("grant_type", "exchange_code"),
             ("exchange_code", &exchange.code)]).await?);

        Ok(self.auth.as_ref().unwrap())
    }

    /// # Errors
    /// 
    /// Will return `Err` if the request was not successful or if the user was not authenticated
    pub async fn get_exchange_code(&self) -> Result<ExchangeCode> {
        self.requires_authentication()?;

        let request = Request::builder()
            .uri(format!("{}{}", ACCOUNT_PUBLIC_SERVICE, "/account/api/oauth/exchange"))
            .header("Authorization", self.get_authentication_header())
            .body(Body::empty())?;

        let response = self.client.request(request).await?;
        let data = Self::process_response(response).await?;
        let exchange: ExchangeCode = serde_json::from_reader(data.reader())?;

        Ok(exchange)
    }

    pub async fn download_manifest_async(&self, manifest: &ManifestInfo, cache_dir: Option<&str>) -> Result<Vec<u8>> {
        if let Some(cache_dir) = cache_dir {
            let path: PathBuf = [cache_dir, &manifest.file_name].iter().collect();
            if path.as_path().exists() {
                let file = std::fs::read(path)?;
                return Ok(file);
            }
        }

        let mut response = self.client.get(manifest.uri.parse()?).await?;
        let content_length: usize = match response.headers().get(hyper::header::CONTENT_LENGTH) {
            Some(val) => val.to_str()?.parse()?,
            None => 0,
        };

        let mut result = Vec::with_capacity(std::cmp::max(content_length, 1024));
        while let Some(chunk) = response.body_mut().data().await {
            let chunk = chunk?;
            result.extend_from_slice(&chunk);
        }

        if let Some(cache_dir) = cache_dir {
            if !Path::new(cache_dir).exists() {
                std::fs::create_dir(cache_dir)?;
            }

            let path: PathBuf = [cache_dir, &manifest.file_name].iter().collect();
            std::fs::write(path, &result)?;
        }

        Ok(result)
    }

    /// # Errors
    /// 
    /// Will return `Err` if the request was not successful
    async fn authenticate(&self, client_token: &ClientToken, parameters: &[(&str, &str)]) -> Result<AuthResponse> {
        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("{}{}", ACCOUNT_PUBLIC_SERVICE, "/account/api/oauth/token"))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Authorization", format!("basic {}", client_token.encoded))
            .body(Body::from(serde_urlencoded::to_string(parameters)?))?;

        let response = self.client.request(request).await?;
        let data = Self::process_response(response).await?;

        let auth: AuthResponse = serde_json::from_reader(data.reader())?;

        Ok(auth)
    }

    async fn process_response(res: Response<Body>) -> Result<impl Buf> {
        let is_success = res.status().is_success();
        let data = hyper::body::aggregate(res).await?;
        if !is_success { 
            let error: EpicError = serde_json::from_reader(data.reader())?;
            return Err(Box::new(error))
        }

        Ok(data)
    }

    pub fn set_authentication(&mut self, auth: AuthResponse) {
        self.auth = Some(auth);
    }

    // todo: check for expiration
    fn requires_authentication(&self) ->Result<()> {
        if self.auth.is_none() {
            return Err(Box::new(ParserError::new("the client must be authenticated.")));
        }

        Ok(())
    }

    fn get_authentication_header(&self) -> String {
        format!("bearer {}", self.auth.as_ref().unwrap().access_token)
    }

}

impl Default for EpicGamesClient {
    fn default() -> Self {
        Self::new()
    }
}