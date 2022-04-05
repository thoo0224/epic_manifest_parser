use std::sync::Arc;

use hyper::{Client, client::HttpConnector, Body, body::HttpBody};
use hyper::{Request};
use hyper_tls::HttpsConnector;

use crate::Result;

#[derive(Debug)]
pub struct HttpService {
    client: Arc<Client<HttpsConnector<HttpConnector>>>
}

impl HttpService {

    pub fn new() -> Self  {
        let connector = HttpsConnector::new();
        let client = Client::builder()
            .build(connector);

        Self {
            client: Arc::new(client)
        }
    }

    // todo: unsuccessful result
    pub async fn get(&self, uri: &str) -> Result<Vec<u8>> {
        let request = Request::builder()
            .uri(uri)
            .body(Body::empty())
            .unwrap();

        let mut response = self.client.request(request).await?;        
        let content_length: usize = match response.headers().get(hyper::header::CONTENT_LENGTH) {
            Some(val) => val.to_str()?.parse()?,
            None => 0,
        };
        
        let mut result = Vec::with_capacity(std::cmp::max(content_length, 1024));
        while let Some(chunk) = response.body_mut().data().await {
            let chunk = chunk?;
            result.extend_from_slice(&chunk);
        }

        Ok(result)
    }

}

impl Default for HttpService {
    fn default() -> Self {
        Self::new()
    }
}