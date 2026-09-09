use super::*;
use madari_model::Result;
use reqwest::{Client as HttpClient, Response};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::time::Duration;

// Deliberately no Debug implementation for credentials, tokens or device codes.
#[derive(Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}
impl Credentials {
    pub fn environment() -> Option<Self> {
        Some(Self {
            client_id: std::env::var("MADARI_TRAKT_CLIENT_ID").ok()?,
            client_secret: std::env::var("MADARI_TRAKT_CLIENT_SECRET").ok()?,
            redirect_uri: std::env::var("MADARI_TRAKT_REDIRECT_URI")
                .unwrap_or_else(|_| "urn:ietf:wg:oauth:2.0:oob".into()),
        })
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Tokens {
    pub(crate) access_token: String,
    pub(crate) refresh_token: String,
    pub(crate) created_at: u64,
    pub(crate) expires_in: u64,
}
impl Tokens {
    pub(crate) fn expires_soon(&self) -> bool {
        self.created_at.saturating_add(self.expires_in) <= now().saturating_add(120)
    }
    fn validate(self) -> Result<Self> {
        if self.access_token.is_empty() || self.refresh_token.is_empty() || self.expires_in == 0 {
            return Err(invalid("Trakt returned incomplete login credentials."));
        }
        Ok(self)
    }
}
#[derive(Deserialize)]
pub struct DeviceCode {
    device_code: String,
    pub user_code: String,
    pub verification_url: String,
    pub expires_in: u64,
    pub interval: u64,
    #[serde(skip, default = "std::time::Instant::now")]
    pub issued_at: std::time::Instant,
}
pub enum Poll {
    Pending,
    SlowDown(u64),
    Authorized(Tokens),
}
#[derive(Clone)]
pub struct Client {
    http: HttpClient,
    credentials: Credentials,
    api: String,
    auth: String,
}
impl Client {
    pub fn new(credentials: Credentials) -> Result<Self> {
        if credentials.client_id.trim().is_empty()
            || credentials.client_secret.trim().is_empty()
            || credentials.redirect_uri.trim().is_empty()
        {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "Configure the Trakt application client ID, client secret and redirect URI first.",
            ));
        }
        let http = HttpClient::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("Madari/0.1.0")
            .build()
            .map_err(|_| invalid("Could not initialize Trakt connection."))?;
        Ok(Self {
            http,
            credentials,
            api: "https://api.trakt.tv".into(),
            auth: "https://auth.trakt.tv".into(),
        })
    }
    async fn post(&self, path: &str, body: Value) -> Result<Response> {
        self.http
            .post(format!("{}{path}", self.auth))
            .header("trakt-api-version", "2")
            .header("trakt-api-key", &self.credentials.client_id)
            .json(&body)
            .send()
            .await
            .map_err(network_error)
    }
    pub fn credentials(&self) -> Credentials {
        self.credentials.clone()
    }
    pub async fn device_code(&self) -> Result<DeviceCode> {
        let response = self
            .post(
                "/oauth/device/code",
                json!({"client_id":self.credentials.client_id}),
            )
            .await?;
        check(&response)?;
        let code: DeviceCode = decode(response).await?;
        let url = url::Url::parse(&code.verification_url)
            .map_err(|_| invalid("Trakt returned an invalid activation URL."))?;
        if url.scheme() != "https"
            || !matches!(
                url.host_str(),
                Some("trakt.tv" | "auth.trakt.tv" | "app.trakt.tv")
            )
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
            || code.device_code.is_empty()
            || code.user_code.is_empty()
            || code.expires_in == 0
            || code.interval == 0
        {
            return Err(invalid(
                "Trakt returned an invalid device login. Try again.",
            ));
        }
        Ok(code)
    }
    pub async fn poll(&self, code: &DeviceCode) -> Result<Poll> {
        if code.issued_at.elapsed().as_secs() >= code.expires_in {
            return Err(invalid(
                "The Trakt code expired. Connect again for a new code.",
            ));
        }
        let response = self.post("/oauth/device/token", json!({"code":code.device_code,"client_id":self.credentials.client_id,"client_secret":self.credentials.client_secret})).await?;
        match response.status().as_u16() {
            200 => Ok(Poll::Authorized(
                decode::<Tokens>(response).await?.validate()?,
            )),
            400 => Ok(Poll::Pending),
            429 => Ok(Poll::SlowDown(retry_after(&response).unwrap_or(5))),
            404 | 409 | 410 => Err(invalid(
                "This Trakt code is no longer valid. Connect again for a new code.",
            )),
            418 => Err(Error::new(
                ErrorCode::Forbidden,
                "Trakt login was declined.",
            )),
            _ => {
                check(&response)?;
                Err(invalid("Unexpected Trakt login response."))
            }
        }
    }
    pub(crate) async fn refresh(&self, tokens: &Tokens) -> Result<Tokens> {
        let response = self.post("/oauth/token", json!({"refresh_token":tokens.refresh_token,"client_id":self.credentials.client_id,"client_secret":self.credentials.client_secret,"redirect_uri":self.credentials.redirect_uri,"grant_type":"refresh_token"})).await?;
        if matches!(response.status().as_u16(), 400 | 401) {
            return Err(Error::new(
                ErrorCode::Forbidden,
                "Trakt login expired. Disconnect and connect again.",
            ));
        }
        check(&response)?;
        decode::<Tokens>(response).await?.validate()
    }
    pub(crate) async fn revoke(&self, tokens: &Tokens) -> Result<()> {
        let response = self.post("/oauth/revoke", json!({"token":tokens.access_token,"client_id":self.credentials.client_id,"client_secret":self.credentials.client_secret})).await?;
        if response.status().as_u16() == 401 {
            return Ok(());
        }
        check(&response)
    }
    pub(super) async fn scrobble_request(
        &self,
        tokens: &Tokens,
        action: &str,
        body: Value,
    ) -> Result<Response> {
        self.http
            .post(format!("{}/scrobble/{action}", self.api))
            .header("trakt-api-version", "2")
            .header("trakt-api-key", &self.credentials.client_id)
            .bearer_auth(&tokens.access_token)
            .json(&body)
            .send()
            .await
            .map_err(network_error)
    }
    pub(crate) async fn get(&self, path: &str, tokens: &Tokens, page: u64) -> Result<Response> {
        let response = self
            .http
            .get(format!(
                "{}{path}{}page={page}&limit=100&extended=full",
                self.api,
                if path.contains('?') { "&" } else { "?" }
            ))
            .header("trakt-api-version", "2")
            .header("trakt-api-key", &self.credentials.client_id)
            .bearer_auth(&tokens.access_token)
            .send()
            .await
            .map_err(network_error)?;
        check(&response)?;
        Ok(response)
    }
    pub(crate) async fn account_name(&self, tokens: &Tokens) -> Result<String> {
        let value: Value = decode(self.get("/users/settings", tokens, 1).await?).await?;
        value["user"]["username"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| invalid("Trakt did not return the account name."))
    }
    pub(crate) async fn pages(&self, path: &str, tokens: &Tokens) -> Result<Vec<Value>> {
        let mut items = Vec::new();
        for page in 1..=10_000 {
            let response = self.get(path, tokens, page).await?;
            let total = response
                .headers()
                .get("x-pagination-page-count")
                .map(|h| {
                    h.to_str()
                        .ok()
                        .and_then(|s| s.parse::<u64>().ok())
                        .ok_or_else(|| invalid("Trakt returned invalid pagination."))
                })
                .transpose()?;
            let batch: Vec<Value> = decode(response).await?;
            let len = batch.len();
            items.extend(batch);
            if items.len() > 1_000_000 {
                return Err(invalid(
                    "Trakt library exceeds the import size limit; the previous import was kept.",
                ));
            }
            if total.is_some_and(|n| page >= n) || len == 0 || (total.is_none() && len < 100) {
                return Ok(items);
            }
        }
        Err(invalid(
            "Trakt pagination did not finish; the previous import was kept.",
        ))
    }
}
fn network_error(_: reqwest::Error) -> Error {
    Error::new(
        ErrorCode::Network,
        "Could not reach Trakt. Check your connection and try again.",
    )
}
fn retry_after(response: &Response) -> Option<u64> {
    response
        .headers()
        .get("retry-after")?
        .to_str()
        .ok()?
        .parse()
        .ok()
}
pub(super) fn check(response: &Response) -> Result<()> {
    if response.status().is_success() {
        return Ok(());
    }
    let status = response.status().as_u16();
    let message = match status {
        401 => "Trakt authorization is no longer valid. Reconnect your account.".into(),
        403 => "Trakt refused access. Check the application credentials and account permissions."
            .into(),
        429 => format!(
            "Trakt rate limit reached. Try syncing again in {} seconds.",
            retry_after(response).unwrap_or(60)
        ),
        _ => format!("Trakt returned HTTP {status}. Try again later."),
    };
    Err(Error::new(
        if status == 401 {
            ErrorCode::Forbidden
        } else {
            ErrorCode::Network
        },
        message,
    ))
}
pub(crate) async fn decode<T: DeserializeOwned>(mut response: Response) -> Result<T> {
    const MAX: usize = 16 * 1024 * 1024;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        if bytes.len().saturating_add(chunk.len()) > MAX {
            return Err(invalid("Trakt response is too large."));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| invalid("Trakt returned an unreadable response."))
}
#[cfg(test)]
mod tests;
