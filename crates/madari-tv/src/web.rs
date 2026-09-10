//! LAN settings server, embedded assets and independently authorized browser sessions.
use axum::{
    Json, Router,
    body::Body,
    extract::{
        DefaultBodyLimit, Path, Request, State, ws::Message, ws::WebSocket, ws::WebSocketUpgrade,
    },
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use madari_core::PublicSnapshot;
use madari_model::PlaybackPreferences;
use madari_native::profiles::{Profile, ProfileSession, Profiles};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;
use tokio::sync::{Mutex, broadcast, oneshot};

/// Paired browsers hold a JWT with this lifetime. The signing key is written to
/// disk, so restarting the TV app does not sign anyone out.
const TOKEN_LIFETIME: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// Upper bound on cached browser state. JWTs stay valid, so eviction only frees memory.
const MAX_BROWSERS: usize = 8;
/// The only remote keys the web UI may send; anything else is rejected.
const REMOTE_COMMANDS: [&str; 15] = [
    "up",
    "down",
    "left",
    "right",
    "select",
    "back",
    "play",
    "pause",
    "play_pause",
    "next",
    "previous",
    "seek_forward",
    "seek_back",
    "volume_up",
    "volume_down",
];
/// Player actions the web UI may ask the TV to perform. Payloads are applied by
/// the TV, which owns the player, so this list is the gate on what can be asked.
const PLAYER_ACTIONS: [&str; 13] = [
    "play",
    "pause",
    "play_pause",
    "seek",
    "seek_by",
    "speed",
    "track",
    "resize",
    "subtitle_size",
    "episode",
    "source",
    "retry",
    "stop",
];
/// How many state updates a slow socket may fall behind before it skips to the latest.
const UPDATE_BUFFER: usize = 32;
/// Browser identity plus expiry; everything else is rebuilt from the database.
#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
}
/// The only header this server issues or accepts, pinned so a caller cannot
/// downgrade the algorithm to `none`. Compare the decoded bytes, not the string.
const JWT_HEADER: &[u8] = br#"{"alg":"HS256","typ":"JWT"}"#;
type HmacSha256 = Hmac<Sha256>;
fn secret_bytes(path: &std::path::Path) -> Vec<u8> {
    let file = path.join("web.key");
    if let Ok(bytes) = std::fs::read(&file)
        && bytes.len() >= 32
    {
        return bytes;
    }
    // Two v4 UUIDs give 256 bits of entropy, the minimum key size for HS256.
    let secret = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
    .into_bytes();
    let _ = std::fs::write(&file, &secret);
    secret
}
fn now_secs() -> usize {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .min(usize::MAX as u64) as usize
}
fn expiry() -> usize {
    now_secs() + TOKEN_LIFETIME.as_secs() as usize
}
fn sign(secret: &[u8], signing_input: &str) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC takes keys of any length");
    mac.update(signing_input.as_bytes());
    mac.finalize().into_bytes().to_vec()
}
fn issue(server: &Server, sub: &str) -> Option<String> {
    let header = URL_SAFE_NO_PAD.encode(JWT_HEADER);
    let claims = URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&Claims {
            sub: sub.to_owned(),
            exp: expiry(),
        })
        .ok()?,
    );
    let signing_input = format!("{header}.{claims}");
    let signature = URL_SAFE_NO_PAD.encode(sign(&server.secret, &signing_input));
    Some(format!("{signing_input}.{signature}"))
}
/// Returns the claims only when the signature, the header and the expiry all hold.
fn verify(server: &Server, raw: &str) -> Option<Claims> {
    let mut parts = raw.split('.');
    let header = parts.next()?;
    let claims = parts.next()?;
    let provided = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if URL_SAFE_NO_PAD.decode(header).ok()? != JWT_HEADER {
        return None;
    }
    let expected = sign(&server.secret, &format!("{header}.{claims}"));
    let provided = URL_SAFE_NO_PAD.decode(provided).ok()?;
    if expected.len() != provided.len() || !bool::from(expected.ct_eq(&provided)) {
        return None;
    }
    let claims: Claims = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(claims).ok()?).ok()?;
    if claims.exp <= now_secs() {
        return None;
    }
    Some(claims)
}
struct Browser {
    profiles: Profiles,
    selected: Option<ProfileSession>,
    last_used: Instant,
}
struct Pairing {
    failed: u32,
    locked_until: Option<Instant>,
    browsers: HashMap<String, Arc<Mutex<Browser>>>,
}
/// Drops cached state for the least recently used browser once the cap is reached.
async fn prune(auth: &mut Pairing) {
    while auth.browsers.len() >= MAX_BROWSERS {
        let mut lru: Option<(String, Instant)> = None;
        for (key, browser) in auth.browsers.iter() {
            let used = browser.lock().await.last_used;
            match &lru {
                Some((_, best)) if *best <= used => {}
                _ => lru = Some((key.clone(), used)),
            }
        }
        match lru {
            Some((key, _)) => {
                auth.browsers.remove(&key);
            }
            None => break,
        }
    }
}
struct Server {
    path: PathBuf,
    code: String,
    port: u16,
    enabled: AtomicBool,
    pairing: Mutex<Pairing>,
    revision: Arc<AtomicU64>,
    /// JWT signing key, persisted beside the profile database.
    secret: Vec<u8>,
    /// Logged-out browsers, kept on disk so a restart cannot resurrect them.
    revoked: std::sync::Mutex<HashMap<String, usize>>,
    /// Keys waiting for the TV's poll loop to pick up. Each entry is an object
    /// carrying either a remote key or a player action.
    commands: Arc<std::sync::Mutex<Vec<Value>>>,
    /// Latest player state, shared with the JNI bridge that publishes it.
    player: Arc<std::sync::Mutex<Value>>,
    /// Fans player state out to every open socket.
    updates: broadcast::Sender<Value>,
}
impl Server {
    /// Revokes one browser id until its own expiry, then persists the list.
    fn revoke(&self, sub: &str, exp: usize) {
        let mut revoked = self.revoked.lock().expect("revoked lock");
        revoked.retain(|_, at| *at > now_secs());
        revoked.insert(sub.to_owned(), exp);
        let body = revoked
            .iter()
            .map(|(sub, exp)| format!("{sub} {exp}\n"))
            .collect::<String>();
        let _ = std::fs::write(self.path.join("web.revoked"), body);
    }
}
/// Reads the revoked-browser list, dropping entries that already expired.
fn revoked_browsers(path: &std::path::Path) -> HashMap<String, usize> {
    let mut map = HashMap::new();
    if let Ok(text) = std::fs::read_to_string(path.join("web.revoked")) {
        for line in text.lines() {
            if let Some((sub, exp)) = line.split_once(' ')
                && let Ok(exp) = exp.parse::<usize>()
                && exp > now_secs()
            {
                map.insert(sub.to_owned(), exp);
            }
        }
    }
    map
}
pub struct WebServer {
    server: Arc<Server>,
    stop: Option<oneshot::Sender<()>>,
}
impl WebServer {
    pub fn status(&self) -> Value {
        json!({"running":self.server.enabled.load(Ordering::Relaxed),"port":self.server.port,"code":self.server.code,"revision":self.server.revision.load(Ordering::Relaxed)})
    }
    pub fn stop(mut self) {
        self.server.enabled.store(false, Ordering::Relaxed);
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
    /// Pushes player state to every open socket. Sending with no subscribers is fine.
    pub fn publish(&self, state: Value) {
        let _ = self.server.updates.send(state);
    }
}
impl Drop for WebServer {
    fn drop(&mut self) {
        self.server.enabled.store(false, Ordering::Relaxed);
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

// ---------------------------------------------------------------------------
// Typed request/response DTOs. Schemas are derived only for OpenAPI generation.
// ---------------------------------------------------------------------------

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Serialize)]
struct WebError {
    error: String,
}
/// Public profile fields; `guardian_id` is intentionally not exposed to the browser.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Serialize)]
struct ProfileDto {
    id: String,
    name: String,
    kids: bool,
    pin_protected: bool,
    avatar: Option<String>,
}
impl From<Profile> for ProfileDto {
    fn from(profile: Profile) -> Self {
        Self {
            id: profile.id,
            name: profile.name,
            kids: profile.kids,
            pin_protected: profile.pin_protected,
            avatar: profile.avatar,
        }
    }
}
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Serialize)]
struct Overview {
    profiles: Vec<ProfileDto>,
    active_kids: Option<ProfileDto>,
    selected: Option<ProfileDto>,
    /// Absent until a profile is opened with its PIN.
    snapshot: Option<PublicSnapshot>,
}
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Deserialize)]
struct PairRequest {
    code: String,
}
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Serialize)]
struct PairResponse {
    token: String,
    expires_in: u64,
}
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Deserialize)]
struct SelectProfileRequest {
    id: String,
    #[serde(default)]
    pin: String,
}
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Deserialize)]
struct CreateProfileRequest {
    name: String,
    #[serde(default)]
    pin: String,
    #[serde(default)]
    kids: bool,
}
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Deserialize)]
struct UpdateProfileRequest {
    name: String,
    #[serde(default)]
    pin: String,
}
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Deserialize)]
struct InstallAddonRequest {
    url: String,
    #[serde(default)]
    allow_local: bool,
}
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Deserialize)]
struct ReorderAddonsRequest {
    ids: Vec<String>,
}
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Serialize)]
struct RemoteResponse {
    queued: String,
}
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Deserialize)]
struct RemoteRequest {
    command: String,
}
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Deserialize)]
struct SetEnabledRequest {
    enabled: bool,
}
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Deserialize)]
struct ShareAddonRequest {
    target_id: String,
    #[serde(default)]
    pin: String,
}

/// Verifies the bearer JWT and runs the handler body with the browser locked.
/// Keeps every authenticated route identical about 401 handling.
macro_rules! session {
    ($server:expr, $headers:expr, $browser:ident, $body:block) => {{
        let session = match browser(&$server, &$headers).await {
            Ok(session) => session,
            Err(response) => return response,
        };
        #[allow(unused_mut)]
        let mut $browser = session.lock().await;
        $browser.last_used = Instant::now();
        $body
    }};
}

pub async fn start(
    path: PathBuf,
    revision: Arc<AtomicU64>,
    address: SocketAddr,
    commands: Arc<std::sync::Mutex<Vec<Value>>>,
    player: Arc<std::sync::Mutex<Value>>,
) -> std::io::Result<WebServer> {
    let listener = tokio::net::TcpListener::bind(address).await?;
    let port = listener.local_addr()?.port();
    let (updates, _) = broadcast::channel(UPDATE_BUFFER);
    let code = uuid::Uuid::new_v4().simple().to_string()[..8].to_uppercase();
    let secret = secret_bytes(&path);
    let revoked = revoked_browsers(&path);
    let server = Arc::new(Server {
        path,
        code,
        port,
        enabled: AtomicBool::new(true),
        revision,
        pairing: Mutex::new(Pairing {
            failed: 0,
            locked_until: None,
            browsers: HashMap::new(),
        }),
        secret,
        revoked: std::sync::Mutex::new(revoked),
        commands,
        player,
        updates,
    });
    let routes = Router::new()
        .route("/", get(index_html))
        .route("/app.js", get(app_js))
        .route("/style.css", get(style_css))
        .route("/api/pair", post(pair))
        .route("/api/session/refresh", post(refresh))
        .route("/api/logout", post(logout))
        .route("/api/remote", post(remote))
        .route("/api/ws", get(socket))
        .route("/api/overview", get(overview))
        .route("/api/profiles/select", post(select_profile))
        .route("/api/profiles/lock", post(lock_profile))
        .route("/api/profiles", post(create_profile).patch(update_profile))
        .route("/api/preferences", put(set_preferences))
        .route("/api/addons", post(install_addon))
        .route("/api/addons/order", put(reorder_addons))
        .route("/api/addons/{id}/configuration", put(configure_addon))
        .route("/api/addons/{id}/enabled", put(set_addon_enabled))
        .route("/api/addons/{id}/share", post(share_addon))
        .route("/api/addons/{id}", delete(remove_addon));
    #[cfg(feature = "openapi")]
    let routes = routes.route("/openapi.json", get(openapi_json));
    // Deep links such as /manage/addons re-render the SPA; unknown /api paths stay JSON 404s.
    let routes = routes.fallback(spa_fallback);
    let app = routes
        .layer(DefaultBodyLimit::max(32 * 1024))
        .layer(middleware::from_fn_with_state(server.clone(), guard))
        .with_state(server.clone());
    let (tx, rx) = oneshot::channel();
    let state = server.clone();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await;
        state.enabled.store(false, Ordering::Relaxed);
    });
    Ok(WebServer {
        server,
        stop: Some(tx),
    })
}

async fn index_html() -> Response {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        include_str!("../web/dist/index.html"),
    )
        .into_response()
}
async fn app_js() -> Response {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        include_str!("../web/dist/app.js"),
    )
        .into_response()
}
async fn style_css() -> Response {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../web/dist/style.css"),
    )
        .into_response()
}
/// Client-side routes (React Router) fall through to the single-page app.
async fn spa_fallback(request: Request) -> Response {
    let path = request.uri().path();
    let looks_like_file = path
        .rsplit('/')
        .next()
        .is_some_and(|name| name.contains('.'));
    if path.starts_with("/api/") || looks_like_file || request.method() != axum::http::Method::GET {
        return failure(StatusCode::NOT_FOUND, "Unknown endpoint");
    }
    index_html().await
}
#[cfg(feature = "openapi")]
async fn openapi_json() -> Response {
    Json(openapi_document()).into_response()
}

fn failure(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(WebError {
            error: message.into(),
        }),
    )
        .into_response()
}
fn native_error(e: madari_model::Error) -> Response {
    let status = match e.code {
        madari_model::ErrorCode::Forbidden => StatusCode::FORBIDDEN,
        madari_model::ErrorCode::Conflict => StatusCode::CONFLICT,
        madari_model::ErrorCode::NotFound => StatusCode::NOT_FOUND,
        _ => StatusCode::BAD_REQUEST,
    };
    failure(status, &e.message)
}

/// Host, Origin and content-type checks keep the LAN endpoint out of web pages.
async fn guard(State(server): State<Arc<Server>>, request: Request, next: Next) -> Response {
    if !server.enabled.load(Ordering::Relaxed) {
        return failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "Web settings are stopped. Enable them on the TV.",
        );
    }
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    // Only literal LAN IP hosts, not arbitrary DNS names (including rebinding names).
    let valid = host.parse::<SocketAddr>().ok().is_some_and(|a| {
        a.port() == server.port
            && match a.ip() {
                std::net::IpAddr::V4(ip) => {
                    ip.is_private() || ip.is_loopback() || ip.is_link_local()
                }
                std::net::IpAddr::V6(ip) => {
                    ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local()
                }
            }
    });
    if !valid {
        return failure(
            StatusCode::FORBIDDEN,
            "Open the IP address shown on your TV.",
        );
    }
    if let Some(origin) = request.headers().get(header::ORIGIN)
        && origin.to_str().ok() != Some(format!("http://{host}").as_str())
    {
        return failure(
            StatusCode::FORBIDDEN,
            "Cross-site requests are not allowed.",
        );
    }
    let json_request = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| h.split(';').next() == Some("application/json"));
    if request.method() != axum::http::Method::GET && !json_request {
        return failure(StatusCode::UNSUPPORTED_MEDIA_TYPE, "Use JSON requests.");
    }
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert("referrer-policy", "no-referrer".parse().unwrap());
    headers.insert("content-security-policy","default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self'; img-src 'self' data: https: http:; font-src 'self' data:; base-uri 'none'; frame-ancestors 'none'; form-action 'self'".parse().unwrap());
    response
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = "/api/pair", tag = "Session",
    summary = "Exchange the TV pairing code for a browser token",
    request_body = PairRequest,
    responses((status = 200, body = PairResponse), (status = 401, body = WebError), (status = 429, body = WebError), (status = "default", body = WebError))
))]
async fn pair(State(server): State<Arc<Server>>, Json(input): Json<PairRequest>) -> Response {
    let mut auth = server.pairing.lock().await;
    if auth
        .locked_until
        .is_some_and(|until| until > Instant::now())
    {
        return failure(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many attempts. Wait one minute.",
        );
    }
    if auth.locked_until.take().is_some() {
        auth.failed = 0;
    }
    if !bool::from(
        input
            .code
            .trim()
            .to_uppercase()
            .as_bytes()
            .ct_eq(server.code.as_bytes()),
    ) {
        auth.failed += 1;
        if auth.failed >= 5 {
            auth.locked_until = Some(Instant::now() + Duration::from_secs(60));
        }
        return failure(
            StatusCode::UNAUTHORIZED,
            "Incorrect pairing code. Check the TV Settings screen.",
        );
    }
    auth.failed = 0;
    prune(&mut auth).await;
    let profiles = match Profiles::open(server.path.join("profiles.sqlite")).await {
        Ok(p) => p,
        Err(e) => return native_error(e),
    };
    let sub = uuid::Uuid::new_v4().simple().to_string();
    let token = match issue(&server, &sub) {
        Some(token) => token,
        None => {
            return failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not start a session.",
            );
        }
    };
    auth.browsers.insert(
        sub,
        Arc::new(Mutex::new(Browser {
            profiles,
            selected: None,
            last_used: Instant::now(),
        })),
    );
    Json(PairResponse {
        token,
        expires_in: TOKEN_LIFETIME.as_secs(),
    })
    .into_response()
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = "/api/session/refresh", tag = "Session",
    summary = "Renew the browser JWT before it expires",
    responses((status = 200, body = PairResponse), (status = 401, body = WebError))
))]
async fn refresh(State(server): State<Arc<Server>>, headers: HeaderMap) -> Response {
    let claims = match claims(&server, &headers) {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    match issue(&server, &claims.sub) {
        Some(token) => Json(PairResponse {
            token,
            expires_in: TOKEN_LIFETIME.as_secs(),
        })
        .into_response(),
        None => failure(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not renew the session.",
        ),
    }
}
fn token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}
/// The error arm is the response the caller returns verbatim, so boxing it would
/// only add an allocation on every rejected request.
#[allow(clippy::result_large_err)]
fn claims(server: &Server, headers: &HeaderMap) -> Result<Claims, Response> {
    let raw = token(headers)
        .ok_or_else(|| failure(StatusCode::UNAUTHORIZED, "Pair with the TV to continue."))?;
    let claims = verify(server, raw).ok_or_else(|| {
        failure(
            StatusCode::UNAUTHORIZED,
            "Pairing expired. Enter the TV code again.",
        )
    })?;
    let revoked = server.revoked.lock().expect("revoked lock");
    if revoked
        .get(&claims.sub)
        .is_some_and(|exp| *exp > now_secs())
    {
        return Err(failure(
            StatusCode::UNAUTHORIZED,
            "This browser was disconnected on the TV.",
        ));
    }
    Ok(claims)
}
/// The JWT is the whole credential, so a browser is rebuilt from the database
/// the first time it is seen after the TV app restarts.
async fn browser(server: &Server, headers: &HeaderMap) -> Result<Arc<Mutex<Browser>>, Response> {
    let claims = claims(server, headers)?;
    let mut auth = server.pairing.lock().await;
    if let Some(existing) = auth.browsers.get(&claims.sub).cloned() {
        return Ok(existing);
    }
    let profiles = Profiles::open(server.path.join("profiles.sqlite"))
        .await
        .map_err(native_error)?;
    let browser = Arc::new(Mutex::new(Browser {
        profiles,
        selected: None,
        last_used: Instant::now(),
    }));
    auth.browsers.insert(claims.sub, browser.clone());
    Ok(browser)
}
async fn summary(browser: &Browser) -> madari_model::Result<Overview> {
    let snapshot = match browser.selected.clone() {
        Some(s) => Some(browser.profiles.core(s).snapshot().await?),
        None => None,
    };
    Ok(Overview {
        profiles: browser
            .profiles
            .list()
            .await?
            .into_iter()
            .map(ProfileDto::from)
            .collect(),
        active_kids: browser.profiles.active_kids().await?.map(ProfileDto::from),
        selected: browser
            .selected
            .as_ref()
            .map(|s| ProfileDto::from(s.profile.clone())),
        snapshot,
    })
}
/// Returns the refreshed overview so the browser can re-render. Only mutations bump
/// the revision; a plain overview poll must not wake the TV's web-status loop.
async fn respond(browser: &Browser, server: &Server, bump: bool) -> Response {
    if bump {
        server.revision.fetch_add(1, Ordering::Relaxed);
    }
    match summary(browser).await {
        Ok(overview) => Json(overview).into_response(),
        Err(error) => native_error(error),
    }
}
/// The selected profile owns addon and playback state; without one the route is forbidden.
/// `Response` is returned as-is by every caller; see `claims`.
#[allow(clippy::result_large_err)]
fn core_for(browser: &Browser) -> Result<Arc<madari_core::Core>, Response> {
    let session = browser
        .selected
        .clone()
        .ok_or_else(|| failure(StatusCode::FORBIDDEN, "Open a profile with its PIN first."))?;
    Ok(browser.profiles.core(session))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = "/api/logout", tag = "Session",
    summary = "Drop the browser session",
    responses((status = 200), (status = 401, body = WebError))
))]
async fn logout(State(server): State<Arc<Server>>, headers: HeaderMap) -> Response {
    let browser = match claims(&server, &headers) {
        Ok(claims) => {
            server.revoke(&claims.sub, claims.exp);
            let mut auth = server.pairing.lock().await;
            auth.browsers.remove(&claims.sub)
        }
        Err(_) => None,
    };
    if let Some(browser) = browser {
        let mut browser = browser.lock().await;
        if let Some(selected) = browser.selected.take() {
            let _ = browser.profiles.lock_settings(selected).await;
        }
    }
    Response::new(Body::from("{}"))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = "/api/remote", tag = "Remote",
    summary = "Send a remote-control key to the TV",
    request_body = RemoteRequest,
    responses((status = 200, body = RemoteResponse), (status = 400, body = WebError), (status = 401, body = WebError))
))]
async fn remote(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Json(input): Json<RemoteRequest>,
) -> Response {
    session!(server, headers, _browser, {
        if !REMOTE_COMMANDS.contains(&input.command.as_str()) {
            return failure(StatusCode::BAD_REQUEST, "Unknown remote command.");
        }
        match server.commands.lock() {
            Ok(mut queue) => queue.push(json!({"type":"key","command":input.command.clone()})),
            Err(_) => {
                return failure(StatusCode::INTERNAL_SERVER_ERROR, "Could not reach the TV.");
            }
        }
        Json(RemoteResponse {
            queued: input.command,
        })
        .into_response()
    })
}

async fn socket(State(server): State<Arc<Server>>, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move |socket| websocket(socket, server))
}
/// Verifies the first frame's token. A WebSocket handshake cannot carry an
/// `Authorization` header, so the browser authenticates in-band instead of
/// putting the token in a URL that ends up in logs and history.
fn authorize(server: &Server, text: &str) -> Option<Claims> {
    let message: Value = serde_json::from_str(text).ok()?;
    if message.get("type")?.as_str()? != "auth" {
        return None;
    }
    let claims = verify(server, message.get("token")?.as_str()?)?;
    let revoked = server.revoked.lock().ok()?;
    if revoked
        .get(&claims.sub)
        .is_some_and(|exp| *exp > now_secs())
    {
        return None;
    }
    Some(claims)
}
async fn send_state(socket: &mut WebSocket, state: &Value) -> Result<(), axum::Error> {
    socket
        .send(Message::Text(
            json!({"type":"player","state":state}).to_string().into(),
        ))
        .await
}
fn queue(server: &Server, item: Value) {
    if let Ok(mut queue) = server.commands.lock() {
        // A TV that stopped polling must not grow the queue without bound.
        if queue.len() < 64 {
            queue.push(item);
        }
    }
}
/// Validates one client message and queues the work. Returns an optional reply.
fn accept(server: &Server, text: &str) -> Value {
    let Ok(message) = serde_json::from_str::<Value>(text) else {
        return json!({"type":"error","error":"Malformed message."});
    };
    let kind = message
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match kind {
        "ping" => json!({"type":"pong"}),
        "key" => {
            let command = message
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !REMOTE_COMMANDS.contains(&command) {
                return json!({"type":"error","error":"Unknown remote command."});
            }
            queue(server, json!({"type":"key","command":command}));
            json!({"type":"queued"})
        }
        "player" => {
            let action = message
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !PLAYER_ACTIONS.contains(&action) {
                return json!({"type":"error","error":"Unknown player action."});
            }
            let mut queued = message;
            queued["type"] = json!("player");
            queue(server, queued);
            json!({"type":"queued"})
        }
        _ => json!({"type":"error","error":"Unknown message."}),
    }
}
/// One browser socket: authenticate once, then stream player state both ways.
async fn websocket(mut socket: WebSocket, server: Arc<Server>) {
    let authenticated = match socket.recv().await {
        Some(Ok(Message::Text(text))) => authorize(&server, text.as_str()),
        _ => None,
    };
    if authenticated.is_none() {
        let _ = socket
            .send(Message::Text(
                json!({"type":"error","error":"Pair with the TV to continue."})
                    .to_string()
                    .into(),
            ))
            .await;
        let _ = socket.send(Message::Close(None)).await;
        return;
    }
    let mut updates = server.updates.subscribe();
    // A new socket starts from the current state rather than waiting for a change.
    let current = server
        .player
        .lock()
        .map(|state| state.clone())
        .unwrap_or(Value::Null);
    if send_state(&mut socket, &current).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            update = updates.recv() => match update {
                Ok(state) => {
                    if send_state(&mut socket, &state).await.is_err() {
                        break;
                    }
                }
                // A lagging socket skips ahead: the next message is full state anyway.
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    let reply = accept(&server, text.as_str());
                    if reply.get("type").and_then(Value::as_str) == Some("error")
                        && socket.send(Message::Text(reply.to_string().into())).await.is_err()
                    {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(_)) => {}
                Some(Err(_)) => break,
            },
        }
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = "/api/overview", tag = "Session",
    summary = "Read profiles and the selected profile snapshot",
    responses((status = 200, body = Overview), (status = 401, body = WebError))
))]
async fn overview(State(server): State<Arc<Server>>, headers: HeaderMap) -> Response {
    session!(server, headers, browser, {
        respond(&browser, &server, false).await
    })
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = "/api/profiles/select", tag = "Profiles",
    summary = "Open a profile for management (its PIN unlocks settings)",
    request_body = SelectProfileRequest,
    responses((status = 200, body = Overview), (status = 403, body = WebError), (status = 401, body = WebError), (status = "default", body = WebError))
))]
async fn select_profile(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Json(input): Json<SelectProfileRequest>,
) -> Response {
    session!(server, headers, browser, {
        match browser
            .profiles
            .unlock_management(input.id, input.pin)
            .await
        {
            Ok(selected) => {
                browser.selected = Some(selected);
                respond(&browser, &server, true).await
            }
            Err(error) => native_error(error),
        }
    })
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = "/api/profiles/lock", tag = "Profiles",
    summary = "Close the selected profile and relock settings",
    responses((status = 200, body = Overview), (status = 401, body = WebError))
))]
async fn lock_profile(State(server): State<Arc<Server>>, headers: HeaderMap) -> Response {
    session!(server, headers, browser, {
        if let Some(selected) = browser.selected.take() {
            let _ = browser.profiles.lock_settings(selected).await;
        }
        respond(&browser, &server, true).await
    })
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = "/api/profiles", tag = "Profiles",
    summary = "Create a profile (a PIN-protected adult is required once one exists)",
    request_body = CreateProfileRequest,
    responses((status = 200, body = Overview), (status = 403, body = WebError), (status = 401, body = WebError), (status = "default", body = WebError))
))]
async fn create_profile(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Json(input): Json<CreateProfileRequest>,
) -> Response {
    session!(server, headers, browser, {
        let session = browser.selected.clone();
        match browser
            .profiles
            .create_with_avatar(session, input.name, input.kids, input.pin, None)
            .await
        {
            Ok(_) => respond(&browser, &server, true).await,
            Err(error) => native_error(error),
        }
    })
}

#[cfg_attr(feature = "openapi", utoipa::path(
    patch, path = "/api/profiles", tag = "Profiles",
    summary = "Rename the selected profile or replace its PIN",
    request_body = UpdateProfileRequest,
    responses((status = 200, body = Overview), (status = 401, body = WebError), (status = "default", body = WebError))
))]
async fn update_profile(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Json(input): Json<UpdateProfileRequest>,
) -> Response {
    session!(server, headers, browser, {
        let Some(session) = browser.selected.clone() else {
            return failure(StatusCode::FORBIDDEN, "Open a profile with its PIN first.");
        };
        match browser
            .profiles
            .update(session, input.name, input.pin)
            .await
        {
            Ok(updated) => {
                if let Some(selected) = browser.selected.as_mut() {
                    selected.profile = updated;
                }
                respond(&browser, &server, true).await
            }
            Err(error) => native_error(error),
        }
    })
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put, path = "/api/preferences", tag = "Playback",
    summary = "Replace the selected profile's playback preferences",
    request_body = PlaybackPreferences,
    responses((status = 200, body = Overview), (status = 403, body = WebError), (status = 401, body = WebError), (status = "default", body = WebError))
))]
async fn set_preferences(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Json(preferences): Json<PlaybackPreferences>,
) -> Response {
    session!(server, headers, browser, {
        let core = match core_for(&browser) {
            Ok(core) => core,
            Err(response) => return response,
        };
        match core.set_playback_preferences(preferences).await {
            Ok(_) => respond(&browser, &server, true).await,
            Err(error) => native_error(error),
        }
    })
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = "/api/addons", tag = "Addons",
    summary = "Install an addon from a configured manifest URL",
    request_body = InstallAddonRequest,
    responses((status = 200, body = Overview), (status = 403, body = WebError), (status = 401, body = WebError), (status = "default", body = WebError))
))]
async fn install_addon(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Json(input): Json<InstallAddonRequest>,
) -> Response {
    session!(server, headers, browser, {
        let core = match core_for(&browser) {
            Ok(core) => core,
            Err(response) => return response,
        };
        match core
            .install(
                uuid::Uuid::new_v4().to_string(),
                &input.url,
                input.allow_local,
            )
            .await
        {
            Ok(_) => respond(&browser, &server, true).await,
            Err(error) => native_error(error),
        }
    })
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put, path = "/api/addons/order", tag = "Addons",
    summary = "Reorder installed addons for the selected profile",
    request_body = ReorderAddonsRequest,
    responses((status = 200, body = Overview), (status = 403, body = WebError), (status = 401, body = WebError), (status = "default", body = WebError))
))]
async fn reorder_addons(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Json(input): Json<ReorderAddonsRequest>,
) -> Response {
    session!(server, headers, browser, {
        let core = match core_for(&browser) {
            Ok(core) => core,
            Err(response) => return response,
        };
        match core.reorder(&input.ids).await {
            Ok(_) => respond(&browser, &server, true).await,
            Err(error) => native_error(error),
        }
    })
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put, path = "/api/addons/{id}/configuration", tag = "Addons",
    summary = "Reconfigure a shared addon installation",
    params(("id" = String, Path, description = "Addon installation id")),
    request_body = InstallAddonRequest,
    responses((status = 200, body = Overview), (status = 403, body = WebError), (status = 401, body = WebError), (status = "default", body = WebError))
))]
async fn configure_addon(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<InstallAddonRequest>,
) -> Response {
    session!(server, headers, browser, {
        let core = match core_for(&browser) {
            Ok(core) => core,
            Err(response) => return response,
        };
        match core
            .configure_addon(&id, &input.url, input.allow_local)
            .await
        {
            Ok(_) => respond(&browser, &server, true).await,
            Err(error) => native_error(error),
        }
    })
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put, path = "/api/addons/{id}/enabled", tag = "Addons",
    summary = "Enable or disable an addon for the selected profile",
    params(("id" = String, Path, description = "Addon installation id")),
    request_body = SetEnabledRequest,
    responses((status = 200, body = Overview), (status = 403, body = WebError), (status = 401, body = WebError), (status = "default", body = WebError))
))]
async fn set_addon_enabled(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<SetEnabledRequest>,
) -> Response {
    session!(server, headers, browser, {
        let core = match core_for(&browser) {
            Ok(core) => core,
            Err(response) => return response,
        };
        match core.set_enabled(&id, input.enabled).await {
            Ok(_) => respond(&browser, &server, true).await,
            Err(error) => native_error(error),
        }
    })
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = "/api/addons/{id}/share", tag = "Addons",
    summary = "Link a shared addon installation to another profile",
    params(("id" = String, Path, description = "Addon installation id")),
    request_body = ShareAddonRequest,
    responses((status = 200, body = Overview), (status = 403, body = WebError), (status = 401, body = WebError), (status = "default", body = WebError))
))]
async fn share_addon(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<ShareAddonRequest>,
) -> Response {
    session!(server, headers, browser, {
        let Some(session) = browser.selected.clone() else {
            return failure(StatusCode::FORBIDDEN, "Open a profile with its PIN first.");
        };
        match browser
            .profiles
            .share(session, id, input.target_id, input.pin)
            .await
        {
            Ok(_) => respond(&browser, &server, true).await,
            Err(error) => native_error(error),
        }
    })
}

#[cfg_attr(feature = "openapi", utoipa::path(
    delete, path = "/api/addons/{id}", tag = "Addons",
    summary = "Remove an addon from the selected profile",
    params(("id" = String, Path, description = "Addon installation id")),
    responses((status = 200, body = Overview), (status = 403, body = WebError), (status = 401, body = WebError), (status = "default", body = WebError))
))]
async fn remove_addon(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    session!(server, headers, browser, {
        let core = match core_for(&browser) {
            Ok(core) => core,
            Err(response) => return response,
        };
        match core.remove_addon(&id).await {
            Ok(_) => respond(&browser, &server, true).await,
            Err(error) => native_error(error),
        }
    })
}

// ---------------------------------------------------------------------------
// OpenAPI document (generated only for client codegen; see `web/openapi.json`).
// ---------------------------------------------------------------------------

#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    info(
        title = "Madari TV web settings",
        version = "1.0.0",
        description = "LAN-only settings API served by the Madari Android TV app. Pair with the code shown on the TV, then open a profile with its PIN to manage addons, playback preferences and the profile itself. Every mutation returns the refreshed overview."
    ),
    paths(
        pair, refresh, logout, overview, remote,
        select_profile, lock_profile, create_profile, update_profile,
        set_preferences,
        install_addon, reorder_addons, configure_addon, set_addon_enabled, share_addon, remove_addon
    ),
    components(schemas(
        WebError, ProfileDto, Overview, PairRequest, PairResponse, RemoteRequest, RemoteResponse,
        SelectProfileRequest, CreateProfileRequest, UpdateProfileRequest,
        InstallAddonRequest, ReorderAddonsRequest, SetEnabledRequest, ShareAddonRequest,
        PlaybackPreferences, PublicSnapshot
    )),
    tags(
        (name = "Session", description = "Pairing and the current browser session"),
        (name = "Profiles", description = "Profile selection and management"),
        (name = "Playback", description = "Per-profile playback preferences"),
        (name = "Addons", description = "Installed addons and sharing")
    )
)]
struct ApiDoc;

#[cfg(feature = "openapi")]
pub fn openapi_document() -> utoipa::openapi::OpenApi {
    <ApiDoc as utoipa::OpenApi>::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn paired(client: &reqwest::Client, base: &str, code: &str) -> String {
        client
            .post(format!("{base}/api/pair"))
            .json(&json!({"code":code}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json::<Value>()
            .await
            .unwrap()["token"]
            .as_str()
            .unwrap()
            .into()
    }
    #[tokio::test]
    async fn settings_are_paired_pin_protected_and_independent_of_watching() {
        let dir = tempfile::tempdir().unwrap();
        let tv = Profiles::open(dir.path().join("profiles.sqlite"))
            .await
            .unwrap();
        let adult = tv
            .create(None, "Original".into(), false, "1234".into())
            .await
            .unwrap();
        let watching = tv.unlock(adult.id.clone(), "1234".into()).await.unwrap();
        let revision = Arc::new(AtomicU64::new(0));
        let commands = Arc::new(std::sync::Mutex::new(Vec::new()));
        let server = start(
            dir.path().into(),
            revision.clone(),
            "127.0.0.1:0".parse().unwrap(),
            commands.clone(),
            Arc::new(std::sync::Mutex::new(Value::Null)),
        )
        .await
        .unwrap();
        let base = format!("http://127.0.0.1:{}", server.server.port);
        let client = reqwest::Client::new();
        assert_eq!(
            client
                .get(format!("{base}/api/overview"))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let html = client.get(&base).send().await.unwrap();
        assert!(html.headers().contains_key("content-security-policy"));
        let token = paired(&client, &base, &server.server.code).await;
        let put = |path: &str| {
            client
                .put(format!("{base}{path}"))
                .bearer_auth(&token)
                .json(&json!({}))
        };
        assert_eq!(
            put("/api/preferences").send().await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            client
                .post(format!("{base}/api/profiles/select"))
                .bearer_auth(&token)
                .json(&json!({"id":adult.id,"pin":"0000"}))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        client
            .post(format!("{base}/api/profiles/select"))
            .bearer_auth(&token)
            .json(&json!({"id":adult.id,"pin":"1234"}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        client
            .patch(format!("{base}/api/profiles"))
            .bearer_auth(&token)
            .json(&json!({"name":"Living room"}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        client
            .put(format!("{base}/api/preferences"))
            .bearer_auth(&token)
            .json(&json!({"audio_languages":["hin","eng"],"subtitles_enabled":false}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        let tv_snapshot = tv.core(watching.clone()).snapshot().await.unwrap();
        assert_eq!(
            tv_snapshot.playback_preferences.audio_languages,
            vec!["hin", "eng"]
        );
        assert!(!tv_snapshot.playback_preferences.subtitles_enabled);
        assert_eq!(tv.list().await.unwrap()[0].name, "Living room");
        // Browser authorization did not unlock the TV's own Settings screen.
        assert!(
            tv.core(watching.clone())
                .set_playback_preferences(Default::default())
                .await
                .is_err()
        );
        client
            .post(format!("{base}/api/profiles"))
            .bearer_auth(&token)
            .json(&json!({"name":"Kids","kids":true}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        let kid = tv
            .list()
            .await
            .unwrap()
            .into_iter()
            .find(|p| p.kids)
            .unwrap();
        client
            .post(format!("{base}/api/profiles/select"))
            .bearer_auth(&token)
            .json(&json!({"id":kid.id,"pin":"1234"}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        assert!(
            tv.active_kids().await.unwrap().is_none(),
            "managing kids must not activate kids mode"
        );
        tv.core(watching).snapshot().await.unwrap();
        assert_eq!(
            client
                .post(format!("{base}/api/unknown"))
                .bearer_auth(&token)
                .json(&json!({}))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
        // The remote queue takes allowlisted keys and rejects everything else.
        assert_eq!(
            client
                .post(format!("{base}/api/remote"))
                .bearer_auth(&token)
                .json(&json!({"command":"up"}))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            client
                .post(format!("{base}/api/remote"))
                .bearer_auth(&token)
                .json(&json!({"command":"select; reboot"}))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            commands.lock().unwrap().as_slice(),
            [json!({"type":"key","command":"up"})]
        );
        client
            .post(format!("{base}/api/logout"))
            .bearer_auth(&token)
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        assert_eq!(
            client
                .get(format!("{base}/api/overview"))
                .bearer_auth(&token)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert!(revision.load(Ordering::Relaxed) > 0);
        server.stop();
    }
    /// The socket only forwards messages it can name: unknown keys, unknown
    /// player actions and malformed frames never reach the TV's queue.
    #[tokio::test]
    async fn socket_messages_are_validated_before_they_reach_the_tv() {
        let dir = tempfile::tempdir().unwrap();
        let commands = Arc::new(std::sync::Mutex::new(Vec::new()));
        let server = start(
            dir.path().into(),
            Arc::new(AtomicU64::new(0)),
            "127.0.0.1:0".parse().unwrap(),
            commands.clone(),
            Arc::new(std::sync::Mutex::new(Value::Null)),
        )
        .await
        .unwrap();
        let state = &server.server;
        assert_eq!(accept(state, "not json")["type"], json!("error"));
        assert_eq!(
            accept(state, "{\"type\":\"key\",\"command\":\"nope\"}")["type"],
            json!("error")
        );
        assert_eq!(
            accept(state, "{\"type\":\"player\",\"action\":\"rm -rf\"}")["type"],
            json!("error")
        );
        assert!(commands.lock().unwrap().is_empty());
        assert_eq!(accept(state, "{\"type\":\"ping\"}")["type"], json!("pong"));
        accept(state, "{\"type\":\"key\",\"command\":\"select\"}");
        accept(
            state,
            "{\"type\":\"player\",\"action\":\"seek\",\"position_ms\":4200}",
        );
        assert_eq!(
            commands.lock().unwrap().as_slice(),
            [
                json!({"type":"key","command":"select"}),
                json!({"type":"player","action":"seek","position_ms":4200}),
            ]
        );
        // A socket cannot authenticate with a token that does not verify.
        assert!(authorize(state, "{\"type\":\"auth\",\"token\":\"nope\"}").is_none());
        let issued = issue(state, "browser").unwrap();
        assert!(authorize(state, &json!({"type":"auth","token":issued}).to_string()).is_some());
        server.stop();
    }
    #[tokio::test]
    async fn cross_site_requests_pairing_guessing_and_expired_sessions_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let server = start(
            dir.path().into(),
            Arc::new(AtomicU64::new(0)),
            "127.0.0.1:0".parse().unwrap(),
            Arc::new(std::sync::Mutex::new(Vec::new())),
            Arc::new(std::sync::Mutex::new(Value::Null)),
        )
        .await
        .unwrap();
        let base = format!("http://127.0.0.1:{}", server.server.port);
        let client = reqwest::Client::new();
        let token = paired(&client, &base, &server.server.code).await;
        let result = client
            .post(format!("{base}/api/pair"))
            .header("Origin", "http://evil.example")
            .json(&json!({"code":server.server.code}))
            .send()
            .await
            .unwrap();
        assert_eq!(result.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            client
                .get(&base)
                .header("Host", format!("evil.example:{}", server.server.port))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            client
                .post(format!("{base}/api/profiles/lock"))
                .bearer_auth(&token)
                .header("Content-Type", "text/plain")
                .body("{}")
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
        for _ in 0..5 {
            assert_eq!(
                client
                    .post(format!("{base}/api/pair"))
                    .json(&json!({"code":"wrong"}))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::UNAUTHORIZED
            );
        }
        assert_eq!(
            client
                .post(format!("{base}/api/pair"))
                .json(&json!({"code":server.server.code}))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        // A forged bearer token is rejected, and so is an expired JWT we signed ourselves.
        assert_eq!(
            client
                .get(format!("{base}/api/overview"))
                .bearer_auth("not.a.jwt")
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let expired = {
            let header = URL_SAFE_NO_PAD.encode(JWT_HEADER);
            let claims = URL_SAFE_NO_PAD.encode(
                serde_json::to_vec(&Claims {
                    sub: "browser".into(),
                    exp: 1,
                })
                .unwrap(),
            );
            let signing_input = format!("{header}.{claims}");
            let signature = URL_SAFE_NO_PAD.encode(sign(&server.server.secret, &signing_input));
            format!("{signing_input}.{signature}")
        };
        assert_eq!(
            client
                .get(format!("{base}/api/overview"))
                .bearer_auth(&expired)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        // Clearing in-memory state stands in for a TV app restart: the JWT alone
        // re-admits the browser, which is what keeps pairing across restarts.
        server.server.pairing.lock().await.browsers.clear();
        assert_eq!(
            client
                .get(format!("{base}/api/overview"))
                .bearer_auth(&token)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        server.stop();
    }
    #[tokio::test]
    async fn management_respects_active_kids_guardian_and_does_not_clear_lock() {
        let dir = tempfile::tempdir().unwrap();
        let tv = Profiles::open(dir.path().join("profiles.sqlite"))
            .await
            .unwrap();
        let adult = tv
            .create(None, "Adult".into(), false, "1234".into())
            .await
            .unwrap();
        let session = tv.unlock(adult.id.clone(), "1234".into()).await.unwrap();
        tv.authorize_settings(session.clone(), "1234".into())
            .await
            .unwrap();
        let kid = tv
            .create(Some(session), "Kids".into(), true, "".into())
            .await
            .unwrap();
        tv.unlock(kid.id.clone(), "".into()).await.unwrap();
        let remote = Profiles::open(dir.path().join("profiles.sqlite"))
            .await
            .unwrap();
        assert!(
            remote
                .unlock_management(adult.id, "1234".into())
                .await
                .is_err()
        );
        assert!(
            remote
                .unlock_management(kid.id.clone(), "0000".into())
                .await
                .is_err()
        );
        let session = remote
            .unlock_management(kid.id.clone(), "1234".into())
            .await
            .unwrap();
        remote.lock_settings(session).await.unwrap();
        assert_eq!(tv.active_kids().await.unwrap().unwrap().id, kid.id);
    }
}
