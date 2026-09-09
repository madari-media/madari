//! LAN settings server, embedded assets and independently authorized browser sessions.
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use madari_model::{Error, ErrorCode};
use madari_native::profiles::{ProfileSession, Profiles};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use tokio::sync::{Mutex, oneshot};

const SESSION_LIFETIME: Duration = Duration::from_secs(30 * 60);
const MAX_BROWSERS: usize = 4;
struct Browser {
    profiles: Profiles,
    selected: Option<ProfileSession>,
    expires: Instant,
}
struct Pairing {
    failed: u32,
    locked_until: Option<Instant>,
    sessions: HashMap<String, Arc<Mutex<Browser>>>,
}
struct Server {
    path: PathBuf,
    code: String,
    port: u16,
    enabled: AtomicBool,
    pairing: Mutex<Pairing>,
    revision: Arc<AtomicU64>,
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
}
impl Drop for WebServer {
    fn drop(&mut self) {
        self.server.enabled.store(false, Ordering::Relaxed);
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}
pub async fn start(
    path: PathBuf,
    revision: Arc<AtomicU64>,
    address: SocketAddr,
) -> std::io::Result<WebServer> {
    let listener = tokio::net::TcpListener::bind(address).await?;
    let port = listener.local_addr()?.port();
    let code = uuid::Uuid::new_v4().simple().to_string()[..8].to_uppercase();
    let server = Arc::new(Server {
        path,
        code,
        port,
        enabled: AtomicBool::new(true),
        revision,
        pairing: Mutex::new(Pairing {
            failed: 0,
            locked_until: None,
            sessions: HashMap::new(),
        }),
    });
    let app = Router::new()
        .route(
            "/",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                    include_str!("../web/index.html"),
                )
            }),
        )
        .route(
            "/app.js",
            get(|| async {
                (
                    [(
                        header::CONTENT_TYPE,
                        "application/javascript; charset=utf-8",
                    )],
                    include_str!("../web/app.js"),
                )
            }),
        )
        .route(
            "/style.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
                    include_str!("../web/style.css"),
                )
            }),
        )
        .route("/api/pair", post(pair))
        .route("/api/overview", get(overview))
        .route("/api/command", post(command))
        .route("/api/logout", post(logout))
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
fn failure(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"error":message}))).into_response()
}
fn native_error(e: Error) -> Response {
    let status = match e.code {
        ErrorCode::Forbidden => StatusCode::FORBIDDEN,
        ErrorCode::Conflict => StatusCode::CONFLICT,
        ErrorCode::NotFound => StatusCode::NOT_FOUND,
        _ => StatusCode::BAD_REQUEST,
    };
    failure(status, &e.message)
}
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
    if let Some(origin) = request.headers().get(header::ORIGIN) {
        if origin.to_str().ok() != Some(format!("http://{host}").as_str()) {
            return failure(
                StatusCode::FORBIDDEN,
                "Cross-site requests are not allowed.",
            );
        }
    }
    if request.method() != axum::http::Method::GET
        && !request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .is_some_and(|h| h.split(';').next() == Some("application/json"))
    {
        return failure(StatusCode::UNSUPPORTED_MEDIA_TYPE, "Use JSON requests.");
    }
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert("referrer-policy", "no-referrer".parse().unwrap());
    headers.insert("content-security-policy","default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'".parse().unwrap());
    response
}
#[derive(Deserialize)]
struct Pair {
    code: String,
}
async fn pair(State(server): State<Arc<Server>>, Json(input): Json<Pair>) -> Response {
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
    let keys = auth.sessions.keys().cloned().collect::<Vec<_>>();
    for key in keys {
        if let Some(browser) = auth.sessions.get(&key) {
            let expired = browser.lock().await.expires <= Instant::now();
            if expired {
                auth.sessions.remove(&key);
            }
        }
    }
    if auth.sessions.len() >= MAX_BROWSERS {
        return failure(
            StatusCode::TOO_MANY_REQUESTS,
            "Four browsers are already paired. Disconnect one or restart web settings on the TV.",
        );
    }
    let profiles = match Profiles::open(server.path.join("profiles.sqlite")).await {
        Ok(p) => p,
        Err(e) => return native_error(e),
    };
    let token = uuid::Uuid::new_v4().simple().to_string();
    auth.sessions.insert(
        token.clone(),
        Arc::new(Mutex::new(Browser {
            profiles,
            selected: None,
            expires: Instant::now() + SESSION_LIFETIME,
        })),
    );
    Json(json!({"token":token,"expires_in":SESSION_LIFETIME.as_secs()})).into_response()
}
fn token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}
async fn browser(server: &Server, headers: &HeaderMap) -> Result<Arc<Mutex<Browser>>, Response> {
    let auth = server.pairing.lock().await;
    let session = token(headers)
        .and_then(|t| auth.sessions.get(t))
        .cloned()
        .ok_or_else(|| failure(StatusCode::UNAUTHORIZED, "Pair with the TV to continue."))?;
    if session.lock().await.expires <= Instant::now() {
        return Err(failure(
            StatusCode::UNAUTHORIZED,
            "Pairing expired. Enter the TV code again.",
        ));
    }
    Ok(session)
}
async fn overview(State(server): State<Arc<Server>>, headers: HeaderMap) -> Response {
    let browser = match browser(&server, &headers).await {
        Ok(b) => b,
        Err(r) => return r,
    };
    let browser = browser.lock().await;
    match summary(&browser).await {
        Ok(value) => Json(value).into_response(),
        Err(e) => native_error(e),
    }
}
async fn summary(browser: &Browser) -> madari_model::Result<Value> {
    let snapshot = match browser.selected.clone() {
        Some(s) => Some(browser.profiles.core(s).snapshot().await?),
        None => None,
    };
    Ok(
        json!({"profiles":browser.profiles.list().await?,"active_kids":browser.profiles.active_kids().await?,"selected":browser.selected.as_ref().map(|s|&s.profile),"snapshot":snapshot}),
    )
}
#[derive(Deserialize)]
struct Command {
    operation: String,
    #[serde(default)]
    args: Value,
}
async fn command(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Json(input): Json<Command>,
) -> Response {
    let browser = match browser(&server, &headers).await {
        Ok(b) => b,
        Err(r) => return r,
    };
    let mut browser = browser.lock().await;
    if browser.expires <= Instant::now() {
        return failure(StatusCode::UNAUTHORIZED, "Pairing expired.");
    }
    let result = execute(&mut browser, &input.operation, input.args).await;
    match result {
        Ok(()) => {
            server.revision.fetch_add(1, Ordering::Relaxed);
            match summary(&browser).await {
                Ok(v) => Json(v).into_response(),
                Err(e) => native_error(e),
            }
        }
        Err(e) => native_error(e),
    }
}
async fn execute(browser: &mut Browser, operation: &str, args: Value) -> madari_model::Result<()> {
    use super::{decode, invalid, string};
    match operation {
        "select_profile" => {
            let selected = browser
                .profiles
                .unlock_management(string(&args, "id"), string(&args, "pin"))
                .await?;
            browser.selected = Some(selected);
            return Ok(());
        }
        "lock_profile" => {
            if let Some(old) = browser.selected.take() {
                browser.profiles.lock_settings(old).await?;
            }
            return Ok(());
        }
        "create_profile" => {
            browser
                .profiles
                .create(
                    browser.selected.clone(),
                    string(&args, "name"),
                    args["kids"].as_bool().unwrap_or(false),
                    string(&args, "pin"),
                )
                .await?;
            return Ok(());
        }
        _ => {}
    }
    let session = browser
        .selected
        .clone()
        .ok_or_else(|| Error::new(ErrorCode::Forbidden, "Open a profile with its PIN first."))?;
    let core = browser.profiles.core(session.clone());
    match operation {
        "install" => {
            core.install(
                uuid::Uuid::new_v4().to_string(),
                &string(&args, "url"),
                args["allow_local"].as_bool().unwrap_or(false),
            )
            .await?;
        }
        "configure" => {
            core.configure_addon(
                &string(&args, "id"),
                &string(&args, "url"),
                args["allow_local"].as_bool().unwrap_or(false),
            )
            .await?;
        }
        "enable" => {
            core.set_enabled(
                &string(&args, "id"),
                args["enabled"].as_bool().unwrap_or(false),
            )
            .await?;
        }
        "remove_addon" => {
            core.remove_addon(&string(&args, "id")).await?;
        }
        "reorder" => {
            core.reorder(&decode::<Vec<String>>(args)?).await?;
        }
        "share" => {
            browser
                .profiles
                .share(
                    session,
                    string(&args, "id"),
                    string(&args, "target_id"),
                    string(&args, "pin"),
                )
                .await?;
        }
        "update_profile" => {
            let updated = browser
                .profiles
                .update(session, string(&args, "name"), string(&args, "pin"))
                .await?;
            // Keep the authorized token but display the new name and PIN status.
            if let Some(selected) = browser.selected.as_mut() {
                selected.profile = updated;
            }
        }
        "preferences" => {
            core.set_playback_preferences(decode(args)?).await?;
        }
        _ => return Err(invalid("Unknown settings action")),
    }
    Ok(())
}
async fn logout(State(server): State<Arc<Server>>, headers: HeaderMap) -> Response {
    let session = {
        let mut auth = server.pairing.lock().await;
        token(&headers).and_then(|t| auth.sessions.remove(t))
    };
    if let Some(session) = session {
        let mut session = session.lock().await;
        if let Some(selected) = session.selected.take() {
            let _ = session.profiles.lock_settings(selected).await;
        }
    }
    Response::new(Body::from("{}"))
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
        let server = start(
            dir.path().into(),
            revision.clone(),
            "127.0.0.1:0".parse().unwrap(),
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
        assert!(html.text().await.unwrap().contains("TV pairing code"));
        let token = paired(&client, &base, &server.server.code).await;
        let post = |op: &str, args: Value| {
            client
                .post(format!("{base}/api/command"))
                .bearer_auth(&token)
                .json(&json!({"operation":op,"args":args}))
        };
        assert_eq!(
            post("preferences", json!({}))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            post("select_profile", json!({"id":adult.id,"pin":"0000"}))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        post("select_profile", json!({"id":adult.id,"pin":"1234"}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        post("update_profile", json!({"name":"Living room"}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        post(
            "preferences",
            json!({"audio_languages":["hin","eng"],"subtitles_enabled":false}),
        )
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
        post("create_profile", json!({"name":"Kids","kids":true}))
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
        post("select_profile", json!({"id":kid.id,"pin":"1234"}))
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
            post("prepare", json!({})).send().await.unwrap().status(),
            StatusCode::BAD_REQUEST
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
    #[tokio::test]
    async fn cross_site_requests_pairing_guessing_and_expired_sessions_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let server = start(
            dir.path().into(),
            Arc::new(AtomicU64::new(0)),
            "127.0.0.1:0".parse().unwrap(),
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
                .post(format!("{base}/api/command"))
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
        {
            let auth = server.server.pairing.lock().await;
            auth.sessions[&token].lock().await.expires = Instant::now() - Duration::from_secs(1);
        }
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
