//! Authenticated companion transport. Credentials never travel to media URLs.
use async_trait::async_trait;
use madari_core::PlaybackMedia;
use madari_model::{Error, ErrorCode, MediaTicket, Result, Torrent};
use reqwest::{Client, Method};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::time::Duration;
use url::Url;

#[derive(Clone)]
pub struct Companion {
    internal: Option<std::sync::Arc<crate::internal_media::InternalMedia>>,
    origin: Url,
    key: String,
    http: Client,
}
fn failure(message: &str) -> Error {
    Error::new(ErrorCode::Network, message)
}
impl Companion {
    pub fn new(origin: &str, key: String) -> Result<Self> {
        let origin =
            Url::parse(origin).map_err(|_| failure("Enter a valid companion HTTP(S) address."))?;
        if !matches!(origin.scheme(), "http" | "https")
            || origin.host_str().is_none()
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.query().is_some()
            || origin.fragment().is_some()
            || origin.path() != "/"
        {
            return Err(failure(
                "Use the companion origin, such as http://localhost:11470, without a path or credentials.",
            ));
        }
        if !(32..=1024).contains(&key.len()) || !key.bytes().all(|b| (33..=126).contains(&b)) {
            return Err(failure(
                "The companion API key must contain 32–1024 non-space ASCII characters.",
            ));
        }
        let http = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|_| failure("Could not initialize companion transport."))?;
        Ok(Self {
            origin,
            key,
            http,
            internal: None,
        })
    }
    pub fn internal(data: std::path::PathBuf) -> Self {
        Self {
            internal: Some(std::sync::Arc::new(
                crate::internal_media::InternalMedia::new(data),
            )),
            origin: Url::parse("http://localhost/").unwrap(),
            key: String::new(),
            http: Client::new(),
        }
    }
    pub fn local(&self) -> Option<std::sync::Arc<crate::internal_media::InternalMedia>> {
        self.internal.clone()
    }
    pub fn is_internal(&self) -> bool {
        self.internal.is_some()
    }
    pub fn origin(&self) -> &str {
        if self.is_internal() {
            return "Built-in · no listening ports";
        }
        self.origin.as_str()
    }
    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Value,
    ) -> Result<T> {
        let url = self
            .origin
            .join(path)
            .map_err(|_| failure("Invalid companion route."))?;
        let mut response = self
            .http
            .request(method, url)
            .bearer_auth(&self.key)
            .json(&body)
            .send()
            .await
            .map_err(|_| {
                failure("Could not reach the companion. Check its address and connection.")
            })?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(failure("The companion rejected the API key."));
        }
        if !response.status().is_success() {
            return Err(failure("The companion could not complete this request."));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| failure("Could not read the companion response."))?
        {
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err(failure("Companion response is too large."));
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| failure("The companion returned an invalid response."))
    }
    pub async fn check(&self) -> Result<()> {
        if self.is_internal() {
            return Ok(());
        }
        let value: Value = self.request(Method::GET, "/v1/health", Value::Null).await?;
        if value.get("api_version").and_then(Value::as_u64)
            != Some(madari_model::API_VERSION.into())
        {
            return Err(failure("This companion API version is not supported."));
        }
        Ok(())
    }
    pub fn media_url(&self, path: &str) -> Result<String> {
        if self.is_internal() {
            let token = path
                .strip_prefix("/media/")
                .and_then(|p| p.strip_suffix("/original"))
                .filter(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_hexdigit()))
                .ok_or_else(|| failure("Invalid internal media path."))?;
            return Ok(format!("madari-internal://{token}"));
        }
        if !path.starts_with("/media/") || path.contains('\\') {
            return Err(failure("Invalid companion media path."));
        }
        let url = self
            .origin
            .join(path)
            .map_err(|_| failure("Invalid companion media path."))?;
        if url.origin() != self.origin.origin() || !url.path().starts_with("/media/") {
            return Err(failure("Companion media must use the configured server."));
        }
        Ok(url.into())
    }
    pub async fn revoke(&self, token: &str) {
        if let Some(local) = &self.internal {
            local.revoke(token);
            return;
        }
        if token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && let Ok(url) = self.origin.join(&format!("/v1/media/{token}"))
        {
            let _ = self
                .http
                .delete(url)
                .bearer_auth(&self.key)
                .timeout(Duration::from_secs(5))
                .send()
                .await;
        }
    }
}
#[async_trait]
impl PlaybackMedia for Companion {
    async fn resolve_torrent(&self, magnet: String) -> Result<Torrent> {
        if let Some(local) = &self.internal {
            return local.resolve(magnet).await;
        }
        self.request(Method::POST, "/v1/torrents", json!({"magnet":magnet}))
            .await
    }
    async fn create_torrent_ticket(
        &self,
        id: &str,
        file: usize,
        resume_ms: u64,
    ) -> Result<MediaTicket> {
        if let Some(local) = &self.internal {
            return local.ticket(id, file).await;
        }
        self.request(
            Method::POST,
            "/v1/media",
            json!({"torrent_id":id,"file":file,"resume_ms":resume_ms}),
        )
        .await
    }
}
/// Direct playback does not require a configured companion.
pub struct NoCompanion;
#[async_trait]
impl PlaybackMedia for NoCompanion {
    async fn resolve_torrent(&self, _: String) -> Result<Torrent> {
        Err(failure(
            "Configure a companion in profile settings to play torrents.",
        ))
    }
    async fn create_torrent_ticket(&self, _: &str, _: usize, _: u64) -> Result<MediaTicket> {
        Err(failure("Configure a companion to play torrents."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credentials_and_media_origins_are_constrained() {
        let key = "x".repeat(32);
        assert!(Companion::new("http://user:secret@localhost:11470", key.clone()).is_err());
        assert!(Companion::new("http://localhost:11470/prefix", key.clone()).is_err());
        let c = Companion::new("http://localhost:11470", key).unwrap();
        assert!(c.media_url("//evil.example/media/token").is_err());
        assert!(c.media_url("/media/../v1/health").is_err());
        assert_eq!(
            c.media_url("/media/token/transcode?start_ms=42000")
                .unwrap(),
            "http://localhost:11470/media/token/transcode?start_ms=42000"
        );
    }
}
