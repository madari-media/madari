use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Path, Query, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, post, put},
};
use futures::StreamExt;
use madari_core::{Core, PublicSnapshot, prepare_playback};
use madari_media::{Ffmpeg, TorrentEngine, TorrentInput, byte_range, read_range};
use madari_model::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    convert::Infallible,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use tokio::sync::Semaphore;
use tower_http::cors::CorsLayer;
use uuid::Uuid;

pub mod openapi;

#[derive(Clone)]
struct Ticket {
    torrent_id: String,
    file: usize,
    expires: Instant,
}

pub struct AppState {
    pub core: Arc<Core>,
    pub torrents: Arc<dyn TorrentEngine>,
    ffmpeg: Ffmpeg,
    api_key_hash: [u8; 32],
    loopback_base: String,
    tickets: Mutex<HashMap<String, Ticket>>,
    media_slots: Arc<Semaphore>,
}

impl AppState {
    pub fn new(
        core: Arc<Core>,
        torrents: Arc<dyn TorrentEngine>,
        ffmpeg: Ffmpeg,
        api_key: &str,
        loopback_base: String,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            api_key.len() >= 32
                && api_key.len() <= 1024
                && api_key.bytes().all(|b| b.is_ascii_graphic()),
            "MADARI_API_KEY must contain 32–1024 printable non-space ASCII characters"
        );
        Ok(Self {
            core,
            torrents,
            ffmpeg,
            api_key_hash: Sha256::digest(api_key.as_bytes()).into(),
            loopback_base,
            tickets: Mutex::new(HashMap::new()),
            media_slots: Arc::new(Semaphore::new(32)),
        })
    }

    fn ticket(&self, token: &str) -> Result<Ticket> {
        let mut tickets = self.tickets.lock().expect("ticket lock poisoned");
        tickets.retain(|_, t| t.expires > Instant::now());
        tickets
            .get(token)
            .cloned()
            .ok_or_else(|| Error::new(ErrorCode::Forbidden, "media ticket expired or invalid"))
    }

    fn issue_ticket(&self, torrent_id: &str, file: usize, resume_ms: u64) -> Result<MediaTicket> {
        if !self
            .torrents
            .details(torrent_id)?
            .files
            .iter()
            .any(|f| f.index == file)
        {
            return Err(Error::new(ErrorCode::NotFound, "torrent file not found"));
        }
        if resume_ms > 604800000 {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "companion preparation supports resume positions up to seven days",
            ));
        }
        let token = Uuid::new_v4().simple().to_string();
        let lifetime = Duration::from_secs(6 * 60 * 60);
        let mut tickets = self.tickets.lock().expect("ticket lock poisoned");
        tickets.retain(|_, t| t.expires > Instant::now());
        if tickets.len() >= 256 {
            return Err(Error::new(ErrorCode::Busy, "media ticket limit reached"));
        }
        let mut transcode_path = format!("/media/{token}/transcode");
        if resume_ms > 0 {
            transcode_path.push_str(&format!(
                "?start={}.{:03}",
                resume_ms / 1000,
                resume_ms % 1000
            ));
        }
        tickets.insert(
            token.clone(),
            Ticket {
                torrent_id: torrent_id.to_owned(),
                file,
                expires: Instant::now() + lifetime,
            },
        );
        Ok(MediaTicket {
            direct_path: format!("/media/{token}/original"),
            transcode_path,
            token,
            expires_in_seconds: lifetime.as_secs(),
            transcode_start_ms: resume_ms,
        })
    }
}

#[async_trait::async_trait]
impl madari_core::PlaybackMedia for AppState {
    async fn resolve_torrent(&self, magnet: String) -> Result<Torrent> {
        self.torrents.add(TorrentInput::Magnet(magnet)).await
    }
    async fn create_torrent_ticket(
        &self,
        id: &str,
        file: usize,
        resume_ms: u64,
    ) -> Result<MediaTicket> {
        self.issue_ticket(id, file, resume_ms)
    }
}

pub struct ApiError(Error);
impl From<Error> for ApiError {
    fn from(value: Error) -> Self {
        Self(value)
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.0.code {
            ErrorCode::InvalidInput
            | ErrorCode::UnsupportedTransport
            | ErrorCode::UnsupportedResource
            | ErrorCode::ConfigurationRequired => StatusCode::BAD_REQUEST,
            ErrorCode::Forbidden => StatusCode::FORBIDDEN,
            ErrorCode::NotFound => StatusCode::NOT_FOUND,
            ErrorCode::Conflict => StatusCode::CONFLICT,
            ErrorCode::Busy => StatusCode::TOO_MANY_REQUESTS,
            ErrorCode::Timeout => StatusCode::GATEWAY_TIMEOUT,
            ErrorCode::Storage => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::BAD_GATEWAY,
        };
        (status, Json(self.0)).into_response()
    }
}
type ApiResult<T> = std::result::Result<T, ApiError>;

async fn authenticate(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    let key = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));
    let valid = key.filter(|k| k.len() <= 1024).is_some_and(|key| {
        let digest: [u8; 32] = Sha256::digest(key.as_bytes()).into();
        bool::from(digest.ct_eq(&state.api_key_hash))
    });
    if !valid {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            Json(Error::new(ErrorCode::Forbidden, "valid API key required")),
        )
            .into_response();
    }
    next.run(request).await
}

async fn response_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        "x-request-id",
        HeaderValue::from_str(&Uuid::new_v4().to_string()).expect("UUID header"),
    );
    response
}

pub fn router(state: Arc<AppState>, origins: Vec<HeaderValue>) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/snapshot", get(snapshot))
        .route("/events", get(events))
        .route("/addons", post(install))
        .route("/addons/order", put(reorder))
        .route("/addons/{id}", put(set_enabled).delete(remove_addon))
        .route("/query", post(query_all))
        .route("/addons/{id}/query", post(query_one))
        .route("/library", put(save_item).delete(remove_item))
        .route("/progress", put(progress))
        .route("/playback/plan", post(plan))
        .route("/playback/prepare", post(prepare))
        .route("/torrents", get(list_torrents).post(add_torrent))
        .route("/torrents/metainfo", post(add_metainfo))
        .route(
            "/torrents/{id}",
            get(torrent_details).delete(remove_torrent),
        )
        .route("/media", post(create_ticket))
        .route("/media/{token}", axum::routing::delete(revoke_ticket))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate));
    Router::new()
        .nest("/v1", api)
        .merge(
            utoipa_swagger_ui::SwaggerUi::new("/docs")
                .url("/openapi.json", openapi::document())
                .config(
                    utoipa_swagger_ui::Config::default()
                        .persist_authorization(false)
                        .validator_url("none"),
                ),
        )
        .route("/media/{token}/original", get(original))
        .route("/media/{token}/transcode", get(transcode))
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
        .layer(
            CorsLayer::new()
                .allow_origin(origins)
                .allow_methods([
                    Method::GET,
                    Method::HEAD,
                    Method::POST,
                    Method::PUT,
                    Method::DELETE,
                ])
                .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::RANGE])
                .expose_headers([
                    header::CONTENT_RANGE,
                    header::ACCEPT_RANGES,
                    header::CONTENT_LENGTH,
                ]),
        )
        .layer(middleware::from_fn(response_headers))
        .with_state(state)
}

#[derive(Serialize, utoipa::ToSchema)]
struct Health {
    api_version: u32,
    torrent: bool,
    ffmpeg: bool,
}
#[utoipa::path(
    get,
    path = "/v1/health",
    tag = "Core",
    summary = "Check server capabilities",
    responses((status = 200, description = "Success", body = Health), (status = 401, description = "Missing or invalid bearer API key", body = Error), (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error))
)]
async fn health(State(s): State<Arc<AppState>>) -> Json<Health> {
    Json(Health {
        api_version: API_VERSION,
        torrent: true,
        ffmpeg: s.ffmpeg.available().await,
    })
}
#[utoipa::path(
    get,
    path = "/v1/snapshot",
    tag = "Core",
    summary = "Read persisted state",
    responses((status = 200, description = "Success", body = PublicSnapshot), (status = 401, description = "Missing or invalid bearer API key", body = Error), (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error))
)]
async fn snapshot(State(s): State<Arc<AppState>>) -> ApiResult<Json<PublicSnapshot>> {
    Ok(Json(s.core.snapshot().await?))
}

#[utoipa::path(
    get,
    path = "/v1/events",
    tag = "Core",
    summary = "Subscribe to a snapshot and revision changes",
    responses((status = 200, description = "SSE: initial snapshot event, then changed revision events. Reconnect after overflow.", body = String, content_type = "text/event-stream"), (status = 401, description = "Missing or invalid bearer API key", body = Error), (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error))
)]
async fn events(
    State(s): State<Arc<AppState>>,
) -> ApiResult<Sse<impl futures::Stream<Item = std::result::Result<Event, Infallible>>>> {
    let receiver = s.core.subscribe();
    let snapshot = s.core.snapshot().await?;
    let initial_revision = snapshot.revision;
    let initial = Event::default()
        .event("snapshot")
        .id(initial_revision.to_string())
        .json_data(snapshot)
        .expect("serializable snapshot");
    let stream = futures::stream::once(async { Ok(initial) }).chain(receiver.filter_map(
        move |event| async move {
            (event.revision > initial_revision).then(|| {
                Ok(Event::default()
                    .event("changed")
                    .id(event.revision.to_string())
                    .json_data(event)
                    .expect("serializable revision"))
            })
        },
    ));
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

#[derive(Serialize, utoipa::ToSchema)]
struct Revision {
    revision: u64,
}
#[derive(Deserialize, utoipa::ToSchema)]
struct Install {
    manifest_url: String,
    #[serde(default)]
    allow_local: bool,
}
#[derive(Serialize, utoipa::ToSchema)]
struct Installed {
    installation_id: String,
    revision: u64,
}
#[utoipa::path(
    post,
    path = "/v1/addons",
    tag = "Addons",
    summary = "Install a configured addon",
    request_body = Install,
    responses((status = 200, description = "Success", body = Installed), (status = 401, description = "Missing or invalid bearer API key", body = Error), (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error))
)]
async fn install(
    State(s): State<Arc<AppState>>,
    Json(input): Json<Install>,
) -> ApiResult<Json<Installed>> {
    let id = Uuid::new_v4().to_string();
    let revision = s
        .core
        .install(id.clone(), &input.manifest_url, input.allow_local)
        .await?;
    Ok(Json(Installed {
        installation_id: id,
        revision,
    }))
}
#[derive(Deserialize, utoipa::ToSchema)]
struct Enabled {
    enabled: bool,
}
#[utoipa::path(
    put,
    path = "/v1/addons/{id}",
    tag = "Addons",
    summary = "Enable or disable an addon",
    request_body = Enabled,
    params(("id" = String, Path, description = "Installation ID or torrent info hash, according to the route")),
    responses((status = 200, description = "Success", body = Revision), (status = 401, description = "Missing or invalid bearer API key", body = Error), (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error))
)]
async fn set_enabled(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<Enabled>,
) -> ApiResult<Json<Revision>> {
    Ok(Json(Revision {
        revision: s.core.set_enabled(&id, input.enabled).await?,
    }))
}
#[utoipa::path(
    delete,
    path = "/v1/addons/{id}",
    tag = "Addons",
    summary = "Remove an addon installation",
    params(("id" = String, Path, description = "Installation ID or torrent info hash, according to the route")),
    responses((status = 200, description = "Success", body = Revision), (status = 401, description = "Missing or invalid bearer API key", body = Error), (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error))
)]
async fn remove_addon(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<Revision>> {
    Ok(Json(Revision {
        revision: s.core.remove_addon(&id).await?,
    }))
}
#[utoipa::path(
    put,
    path = "/v1/addons/order",
    tag = "Addons",
    summary = "Set complete addon order",
    request_body = Vec<String>,
    responses((status = 200, description = "Success", body = Revision), (status = 401, description = "Missing or invalid bearer API key", body = Error), (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error))
)]
async fn reorder(
    State(s): State<Arc<AppState>>,
    Json(ids): Json<Vec<String>>,
) -> ApiResult<Json<Revision>> {
    Ok(Json(Revision {
        revision: s.core.reorder(&ids).await?,
    }))
}
#[utoipa::path(
    post,
    path = "/v1/query",
    tag = "Addons",
    summary = "Query all compatible installations",
    request_body = ResourceRequest,
    responses((status = 200, description = "Success", body = Vec<ProviderResult>), (status = 401, description = "Missing or invalid bearer API key", body = Error), (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error))
)]
async fn query_all(
    State(s): State<Arc<AppState>>,
    Json(request): Json<ResourceRequest>,
) -> ApiResult<Json<Vec<ProviderResult>>> {
    Ok(Json(s.core.query_all(request).await?))
}
#[utoipa::path(
    post,
    path = "/v1/addons/{id}/query",
    tag = "Addons",
    summary = "Query one addon resource",
    request_body = ResourceRequest,
    params(("id" = String, Path, description = "Installation ID or torrent info hash, according to the route")),
    responses((status = 200, description = "Success", body = ResourceData), (status = 401, description = "Missing or invalid bearer API key", body = Error), (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error))
)]
async fn query_one(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(request): Json<ResourceRequest>,
) -> ApiResult<Json<ResourceData>> {
    Ok(Json(s.core.query(&id, request).await?))
}
#[utoipa::path(
    put,
    path = "/v1/library",
    tag = "Core",
    summary = "Save a library item",
    request_body = LibraryEntry,
    responses((status = 200, description = "Success", body = Revision), (status = 401, description = "Missing or invalid bearer API key", body = Error), (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error))
)]
async fn save_item(
    State(s): State<Arc<AppState>>,
    Json(entry): Json<LibraryEntry>,
) -> ApiResult<Json<Revision>> {
    Ok(Json(Revision {
        revision: s.core.save_item(entry).await?,
    }))
}
#[utoipa::path(
    delete,
    path = "/v1/library",
    tag = "Core",
    summary = "Remove a library item",
    request_body = ItemKey,
    responses((status = 200, description = "Success", body = Revision), (status = 401, description = "Missing or invalid bearer API key", body = Error), (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error))
)]
async fn remove_item(
    State(s): State<Arc<AppState>>,
    Json(key): Json<ItemKey>,
) -> ApiResult<Json<Revision>> {
    Ok(Json(Revision {
        revision: s.core.remove_item(&key).await?,
    }))
}
#[utoipa::path(
    put,
    path = "/v1/progress",
    tag = "Core",
    summary = "Record actual player progress",
    request_body = Progress,
    responses((status = 200, description = "Success", body = Revision), (status = 401, description = "Missing or invalid bearer API key", body = Error), (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error))
)]
async fn progress(
    State(s): State<Arc<AppState>>,
    Json(progress): Json<Progress>,
) -> ApiResult<Json<Revision>> {
    Ok(Json(Revision {
        revision: s.core.record_progress(progress).await?,
    }))
}
#[derive(Deserialize, utoipa::ToSchema)]
struct PlanInput {
    source: madari_model::Stream,
    capabilities: PlayerCapabilities,
    key: Option<ItemKey>,
    video_id: Option<String>,
}
#[utoipa::path(
    post,
    path = "/v1/playback/plan",
    tag = "Core",
    summary = "Prepare a capability-aware playback plan",
    request_body = PlanInput,
    responses((status = 200, description = "Success", body = PlaybackPlan), (status = 401, description = "Missing or invalid bearer API key", body = Error), (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error))
)]
async fn plan(
    State(s): State<Arc<AppState>>,
    Json(input): Json<PlanInput>,
) -> ApiResult<Json<PlaybackPlan>> {
    match (input.key, input.video_id) {
        (Some(key), Some(video)) => Ok(Json(
            s.core
                .playback_plan(&key, &video, input.source, &input.capabilities)
                .await?,
        )),
        (None, None) => Ok(Json(prepare_playback(input.source, &input.capabilities, 0))),
        _ => Err(Error::new(
            ErrorCode::InvalidInput,
            "provide both key and video_id to recover resume state",
        )
        .into()),
    }
}

#[utoipa::path(
    post,
    path = "/v1/playback/prepare",
    tag = "Core",
    summary = "Resolve an addon stream into ready media delivery and saved resume state",
    description = "For torrents, builds the magnet, resolves metadata, selects the override/fileIdx/largest file, and issues media links. Direct URLs return validated player request headers. Relative media paths use the companion origin. Transcode paths already include the saved resume offset; report progress by adding transcode_start_ms to the player's relative position. Does not start a player or mutate saved progress.",
    request_body = PreparePlaybackRequest,
    responses(
        (status = 200, description = "Prepared delivery, or a preserved unsupported source", body = PreparedPlayback),
        (status = 400, description = "Invalid source, identity or file selection", body = Error),
        (status = 401, description = "Missing or invalid bearer API key", body = Error),
        (status = 429, description = "Torrent initialization or media-ticket capacity reached", body = Error),
        (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error)
    )
)]
async fn prepare(
    State(s): State<Arc<AppState>>,
    Json(input): Json<PreparePlaybackRequest>,
) -> ApiResult<Json<PreparedPlayback>> {
    Ok(Json(s.core.prepare_playback(input, s.as_ref()).await?))
}

#[derive(Deserialize, utoipa::ToSchema)]
struct AddMagnet {
    magnet: String,
}
#[utoipa::path(
    post,
    path = "/v1/torrents",
    tag = "Torrents",
    summary = "Add a torrent magnet",
    request_body = AddMagnet,
    responses((status = 200, description = "Success", body = Torrent), (status = 401, description = "Missing or invalid bearer API key", body = Error), (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error))
)]
async fn add_torrent(
    State(s): State<Arc<AppState>>,
    Json(input): Json<AddMagnet>,
) -> ApiResult<Json<Torrent>> {
    Ok(Json(
        s.torrents.add(TorrentInput::Magnet(input.magnet)).await?,
    ))
}
#[utoipa::path(
    post,
    path = "/v1/torrents/metainfo",
    tag = "Torrents",
    summary = "Upload torrent metainfo",
    request_body(content = Vec<u8>, content_type = "application/octet-stream", description = "Raw .torrent bytes, maximum 4 MiB"),
    responses((status = 200, description = "Success", body = Torrent), (status = 401, description = "Missing or invalid bearer API key", body = Error), (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error))
)]
async fn add_metainfo(State(s): State<Arc<AppState>>, body: Bytes) -> ApiResult<Json<Torrent>> {
    Ok(Json(s.torrents.add(TorrentInput::Metainfo(body)).await?))
}
#[utoipa::path(
    get,
    path = "/v1/torrents",
    tag = "Torrents",
    summary = "List registered torrents",
    responses((status = 200, description = "Success", body = Vec<Torrent>), (status = 401, description = "Missing or invalid bearer API key", body = Error), (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error))
)]
async fn list_torrents(State(s): State<Arc<AppState>>) -> ApiResult<Json<Vec<Torrent>>> {
    Ok(Json(s.torrents.list()?))
}
#[utoipa::path(
    get,
    path = "/v1/torrents/{id}",
    tag = "Torrents",
    summary = "List torrent files",
    params(("id" = String, Path, description = "Installation ID or torrent info hash, according to the route")),
    responses((status = 200, description = "Success", body = Torrent), (status = 401, description = "Missing or invalid bearer API key", body = Error), (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error))
)]
async fn torrent_details(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<Torrent>> {
    Ok(Json(s.torrents.details(&id)?))
}
#[utoipa::path(
    delete,
    path = "/v1/torrents/{id}",
    tag = "Torrents",
    summary = "Forget a torrent without deleting downloaded data",
    params(("id" = String, Path, description = "Installation ID or torrent info hash, according to the route")),
    responses((status = 204, description = "Completed"), (status = 401, description = "Missing or invalid bearer API key", body = Error), (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error))
)]
async fn remove_torrent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    s.torrents.remove(&id).await?;
    s.tickets
        .lock()
        .expect("ticket lock poisoned")
        .retain(|_, t| t.torrent_id != id);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, utoipa::ToSchema)]
struct TicketInput {
    torrent_id: String,
    file: usize,
    #[serde(default)]
    resume_ms: u64,
}
#[utoipa::path(
    post,
    path = "/v1/media",
    tag = "Media",
    summary = "Create a six-hour file-scoped media ticket",
    request_body = TicketInput,
    responses((status = 200, description = "Success", body = MediaTicket), (status = 401, description = "Missing or invalid bearer API key", body = Error), (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error))
)]
async fn create_ticket(
    State(s): State<Arc<AppState>>,
    Json(input): Json<TicketInput>,
) -> ApiResult<Json<MediaTicket>> {
    Ok(Json(s.issue_ticket(
        &input.torrent_id,
        input.file,
        input.resume_ms,
    )?))
}
#[utoipa::path(
    delete,
    path = "/v1/media/{token}",
    tag = "Media",
    summary = "Revoke a media ticket for future requests",
    params(("token" = String, Path, description = "Expiring file-scoped media capability token")),
    responses((status = 204, description = "Completed"), (status = 401, description = "Missing or invalid bearer API key", body = Error), (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error))
)]
async fn revoke_ticket(State(s): State<Arc<AppState>>, Path(token): Path<String>) -> StatusCode {
    s.tickets
        .lock()
        .expect("ticket lock poisoned")
        .remove(&token);
    StatusCode::NO_CONTENT
}

#[utoipa::path(
    method(get, head),
    path = "/media/{token}/original",
    tag = "Media",
    summary = "Stream a torrent file with byte-range seeking",
    params(("token" = String, Path, description = "Expiring file-scoped media capability token"), ("Range" = Option<String>, Header, description = "Single byte range, for example bytes=0-1023 or bytes=-4096")),
    security(()),
    responses((status = 200, description = "Streaming media body; HEAD returns headers only", body = Vec<u8>, content_type = "application/octet-stream"), (status = 206, description = "Partial content with Content-Range, Content-Length and Accept-Ranges headers", body = Vec<u8>, content_type = "application/octet-stream"), (status = 416, description = "Unsatisfiable range; Content-Range specifies total length"), (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error))
)]
async fn original(
    State(s): State<Arc<AppState>>,
    Path(token): Path<String>,
    method: Method,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let ticket = s.ticket(&token)?;
    let permit = s
        .media_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| Error::new(ErrorCode::Busy, "media stream limit reached"))?;
    let file = s.torrents.open(&ticket.torrent_id, ticket.file).await?;
    let length = file.length;
    let header = headers
        .get(header::RANGE)
        .map(|h| h.to_str())
        .transpose()
        .map_err(|_| Error::new(ErrorCode::InvalidInput, "invalid range header"))?;
    let range = match byte_range(header, length) {
        Ok(range) => range,
        Err(_) => {
            return Ok((
                StatusCode::RANGE_NOT_SATISFIABLE,
                [(header::CONTENT_RANGE, format!("bytes */{length}"))],
            )
                .into_response());
        }
    };
    let mut builder = Response::builder()
        .status(if range.partial {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header(header::CONTENT_TYPE, &file.mime)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, range.length);
    if range.partial {
        builder = builder.header(
            header::CONTENT_RANGE,
            format!(
                "bytes {}-{}/{length}",
                range.start,
                range.start + range.length - 1
            ),
        );
    }
    let body = if method == Method::HEAD {
        Body::empty()
    } else {
        Body::from_stream(read_range(file, range, permit).await?)
    };
    builder
        .body(body)
        .map_err(|_| Error::new(ErrorCode::Media, "invalid media response").into())
}

#[derive(Deserialize, Default, utoipa::ToSchema)]
struct TranscodeQuery {
    #[serde(default)]
    start: f64,
}
#[utoipa::path(
    method(get, head),
    path = "/media/{token}/transcode",
    tag = "Media",
    summary = "Transcode a torrent file to H.264 and AAC MP4",
    params(("token" = String, Path, description = "Expiring file-scoped media capability token"), ("start" = Option<f64>, Query, description = "Start offset in seconds, 0 to 604800; a new request restarts transcoding")),
    security(()),
    responses((status = 200, description = "Streaming media body; HEAD returns headers only", body = Vec<u8>, content_type = "video/mp4"), (status = "default", description = "Structured application error; malformed HTTP/JSON may return framework text errors", body = Error))
)]
async fn transcode(
    State(s): State<Arc<AppState>>,
    Path(token): Path<String>,
    method: Method,
    Query(query): Query<TranscodeQuery>,
) -> ApiResult<Response> {
    s.ticket(&token)?;
    if method == Method::HEAD {
        return Ok(([(header::CONTENT_TYPE, "video/mp4")], Body::empty()).into_response());
    }
    let input = format!("{}/media/{token}/original", s.loopback_base);
    let body = s.ffmpeg.transcode(&input, query.start).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "video/mp4"),
            (header::ACCEPT_RANGES, "none"),
        ],
        Body::from_stream(body),
    )
        .into_response())
}
