//! Shared platform boundary for the mobile clients: the [`Bridge`] that owns the
//! runtime, profile session and torrent engine, plus the LAN web settings server.
//!
//! Each client has a thin FFI crate of its own — `madari-android` (JNI, the only
//! `cdylib`) and `madari-ios` (UniFFI) — so this crate stays a plain library.
mod web;
use madari_core::{Core, DEFAULT_ADDONS, PlaybackMedia};
use madari_model::*;
use madari_native::{
    internal_media::{InternalMedia, MediaFile},
    profiles::{ProfileSession, Profiles},
    trakt::{Client, Credentials, DeviceCode, Poll},
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicI64, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt},
    runtime::Runtime,
};
/// Re-exported for the `web_openapi` example used to generate `web/openapi.json`.
#[cfg(feature = "openapi")]
pub use web::openapi_document;

struct Media(Arc<InternalMedia>);
#[async_trait::async_trait]
impl PlaybackMedia for Media {
    async fn resolve_torrent(&self, magnet: String) -> Result<Torrent> {
        self.0.resolve(magnet).await
    }
    async fn create_torrent_ticket(&self, id: &str, file: usize, _: u64) -> Result<MediaTicket> {
        self.0.prioritize_playback(id, file).await?;
        self.0.ticket(id, file).await
    }
}
struct State {
    profiles: Profiles,
    session: Option<ProfileSession>,
}
pub struct Bridge {
    runtime: Runtime,
    path: PathBuf,
    web: Mutex<Option<web::WebServer>>,
    web_revision: Arc<AtomicU64>,
    /// Remote keys and player actions queued by the web UI, drained by the Kotlin poll loop.
    web_commands: Arc<Mutex<Vec<Value>>>,
    /// Latest player state, published by the TV and streamed to open sockets.
    web_player: Arc<Mutex<Value>>,
    state: Mutex<State>,
    media: Arc<InternalMedia>,
    readers: Mutex<HashMap<i64, Arc<Mutex<MediaFile>>>>,
    next_reader: AtomicI64,
    trakt_pending: Mutex<Option<(Client, DeviceCode)>>,
}
fn invalid(message: &str) -> Error {
    Error::new(ErrorCode::InvalidInput, message)
}
fn decode<T: serde::de::DeserializeOwned>(v: Value) -> Result<T> {
    serde_json::from_value(v).map_err(|_| invalid("Invalid request payload"))
}
fn encode<T: serde::Serialize>(v: T) -> Result<Value> {
    serde_json::to_value(v).map_err(|_| invalid("Could not encode response"))
}
fn string(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().to_owned()
}
impl Bridge {
    pub fn open(path: PathBuf) -> Result<Self> {
        madari_native::private_directory(&path)
            .map_err(|_| invalid("Could not create app storage"))?;
        let runtime = Runtime::new().map_err(|_| invalid("Could not start native runtime"))?;
        let profiles = runtime.block_on(Profiles::open(path.join("profiles.sqlite")))?;
        Ok(Self {
            runtime,
            path: path.clone(),
            web: Mutex::new(None),
            web_revision: Arc::new(AtomicU64::new(0)),
            web_commands: Arc::new(Mutex::new(Vec::new())),
            web_player: Arc::new(Mutex::new(Value::Null)),
            state: Mutex::new(State {
                profiles,
                session: None,
            }),
            media: Arc::new(InternalMedia::new(path.join("media"))),
            readers: Mutex::new(HashMap::new()),
            next_reader: AtomicI64::new(1),
            trakt_pending: Mutex::new(None),
        })
    }
    pub fn call(&self, operation: &str, args: Value) -> Result<Value> {
        if operation == "torrent_stats" {
            return Ok(self.media.stats(&string(&args, "token")).map(|s| json!({
                "state":s.state,"downloaded":s.downloaded,"total":s.total,
                "download_speed":s.download_bytes_per_second,"upload_speed":s.upload_bytes_per_second,"peers":s.peers
            })).unwrap_or(Value::Null));
        }
        if operation == "torrents" {
            // What the torrents page lists. The player reported per-token progress only, so
            // there was no way for a client to ask what is actually being downloaded.
            return Ok(Value::Array(
                self.media
                    .managed()
                    .into_iter()
                    .map(|torrent| {
                        json!({
                            "id": torrent.id,
                            "name": torrent.name,
                            "state": torrent.stats.state,
                            "downloaded": torrent.stats.downloaded,
                            "total": torrent.stats.total,
                            "download_speed": torrent.stats.download_bytes_per_second,
                            "upload_speed": torrent.stats.upload_bytes_per_second,
                            "peers": torrent.stats.peers,
                            "path": torrent.download_directory.to_string_lossy(),
                        })
                    })
                    .collect(),
            ));
        }
        if operation == "torrent_play" {
            // The one place playback tells the torrent engine what to fetch. Without the
            // prioritise call, reads of a piece that has not arrived yet block until the
            // download happens to reach them, which is why playback could stall at random
            // percentages. The ticket it returns is wrapped in the usual internal URI.
            let id = string(&args, "id");
            let file = args.get("file").and_then(Value::as_u64).unwrap_or(0) as usize;
            return self.runtime.block_on(async {
                self.media.prioritize_playback(&id, file).await?;
                let ticket = self.media.ticket(&id, file).await?;
                Ok(json!({ "token": ticket.token }))
            });
        }
        if operation == "torrent_pause" {
            let id = string(&args, "id");
            let paused = args.get("paused").and_then(Value::as_bool).unwrap_or(false);
            return self
                .runtime
                .block_on(self.media.set_paused(&id, paused))
                .map(|()| Value::Null);
        }
        if operation == "torrent_remove" {
            let id = string(&args, "id");
            return self
                .runtime
                .block_on(self.media.remove(&id))
                .map(|()| Value::Null);
        }
        if operation == "player_state" {
            // The player lives in Kotlin, so it reports state here and the server
            // streams it to every paired browser.
            if let Ok(mut player) = self.web_player.lock() {
                *player = args.clone();
            }
            if let Ok(web) = self.web.lock()
                && let Some(server) = web.as_ref()
            {
                server.publish(args);
            }
            return Ok(Value::Null);
        }
        if matches!(operation, "web_status" | "web_start" | "web_stop") {
            let mut web = self
                .web
                .lock()
                .map_err(|_| invalid("Web server unavailable"))?;
            if operation == "web_stop" {
                if let Some(server) = web.take() {
                    server.stop();
                }
            } else if operation == "web_start" && web.is_none() {
                *web = Some(
                    self.runtime
                        .block_on(web::start(
                            self.path.clone(),
                            self.web_revision.clone(),
                            "0.0.0.0:11471".parse().unwrap(),
                            self.web_commands.clone(),
                            self.web_player.clone(),
                        ))
                        .map_err(|_| {
                            invalid("Could not start web settings on port 11471. Try again.")
                        })?,
                );
            }
            // Commands are drained rather than read, so a key is delivered exactly once.
            let commands = self
                .web_commands
                .lock()
                .map(|mut queue| std::mem::take(&mut *queue))
                .unwrap_or_default();
            let mut status = web.as_ref().map(|s| s.status()).unwrap_or_else(
                || json!({"running":false,"revision":self.web_revision.load(Ordering::Relaxed)}),
            );
            if let Some(map) = status.as_object_mut() {
                map.insert("commands".into(), json!(commands));
            }
            return Ok(status);
        }
        if operation == "preferred_tracks" {
            let prefs: PlaybackPreferences = decode(args["preferences"].clone())?;
            let tracks: Vec<MediaTrack> = args["tracks"]
                .as_array()
                .ok_or_else(|| invalid("Missing tracks"))?
                .iter()
                .map(|t| MediaTrack {
                    id: t["id"].as_i64().unwrap_or(-1),
                    kind: string(t, "kind"),
                    language: string(t, "language"),
                    selected: t["selected"].as_bool().unwrap_or(false),
                    hearing_impaired: t["hearing_impaired"].as_bool().unwrap_or(false),
                    forced: t["forced"].as_bool().unwrap_or(false),
                    visual_impaired: t["visual_impaired"].as_bool().unwrap_or(false),
                    commentary: t["commentary"].as_bool().unwrap_or(false),
                    ..Default::default()
                })
                .collect();
            return Ok(
                json!({"audio":madari_core::preferred_track(&tracks,&prefs,"audio"),"sub":madari_core::preferred_track(&tracks,&prefs,"sub")}),
            );
        }
        // Network work must never hold the UI/session mutex. ProfileStorage checks
        // the cloned session token when each operation accesses persisted state.
        let (profiles, active_session) = {
            let state = self
                .state
                .lock()
                .map_err(|_| invalid("Native state unavailable"))?;
            (state.profiles.clone(), state.session.clone())
        };
        self.runtime.block_on(async {
            match operation {
                "profiles" => return Ok(json!({"profiles": profiles.list().await?, "active_kids": profiles.active_kids().await?, "avatars": madari_native::profiles::avatars::catalog()})),
                "create_profile" => return encode(profiles.create_with_avatar(active_session.clone(), string(&args,"name"), args["kids"].as_bool().unwrap_or(false), string(&args,"pin"), avatar_arg(&args)?).await?),
                "unlock" => { let session = profiles.unlock(string(&args,"id"), string(&args,"pin")).await?; let profile = session.profile.clone(); self.state.lock().map_err(|_| invalid("Native state unavailable"))?.session = Some(session); return encode(profile); }
                _ => {}
            }
            let session = active_session.clone().ok_or_else(|| invalid("Select a profile first"))?;
            let core: Arc<Core> = profiles.core(session.clone());
            match operation {
                "leave" => { profiles.leave(session, string(&args,"pin")).await?; self.state.lock().map_err(|_| invalid("Native state unavailable"))?.session = None; Ok(Value::Null) }
                "delete_profile" => {
                    // Deleting ends the profile's own session, so the bridge drops it too.
                    profiles.delete(session, string(&args,"pin")).await?;
                    self.state.lock().map_err(|_| invalid("Native state unavailable"))?.session = None;
                    Ok(Value::Null)
                }
                "authorize" => { profiles.authorize_settings(session, string(&args,"pin")).await?; Ok(Value::Null) }
                "lock_settings" => { profiles.lock_settings(session).await?; Ok(Value::Null) }
                "update_profile" => {
                    if args.get("avatar").is_some() {
                        encode(profiles.update_with_avatar(session, string(&args,"name"), string(&args,"pin"), avatar_arg(&args)?).await?)
                    } else {
                        encode(profiles.update(session, string(&args,"name"), string(&args,"pin")).await?)
                    }
                },
                "snapshot" => encode(core.snapshot().await?),
                "calendar" => {
                    let calendar = core.calendar().await?;
                    Ok(json!({"titles": calendar.titles.iter().map(|(key, resolved)| json!({"key":key,"meta":resolved.meta})).collect::<Vec<_>>(),"supported":calendar.supported,"warnings":calendar.warnings}))
                }
                "continue_metadata" => encode(core.continue_metadata(&decode::<Vec<ItemKey>>(args)?).await?),
                "episode" => {
                    let meta: Meta = decode(args["meta"].clone())?;
                    let key: ItemKey = decode(args["key"].clone())?;
                    let today = string(&args,"today");
                    let snapshot = core.snapshot().await?;
                    let video = if args["previous"].as_bool() == Some(true) {
                        madari_core::previous_episode(&meta.videos, &string(&args,"current"), &today)
                    } else if args["current"].is_string() {
                        madari_core::next_episode(&meta.videos, &string(&args,"current"), &today)
                    } else {
                        madari_core::continue_episode(&meta.videos, &snapshot.progress, &key, &today)
                    };
                    Ok(json!({"video":video}))
                }
                "install" => encode(core.install(uuid::Uuid::new_v4().to_string(), &string(&args,"url"), args["allow_local"].as_bool().unwrap_or(false)).await?),
                "configure_addon" => encode(core.configure_addon(&string(&args,"id"), &string(&args,"url"), args["allow_local"].as_bool().unwrap_or(false)).await?),
                "enable" => encode(core.set_enabled(&string(&args,"id"), args["enabled"].as_bool().unwrap_or(false)).await?),
                "remove_addon" => encode(core.remove_addon(&string(&args,"id")).await?),
                "reorder" => encode(core.reorder(&decode::<Vec<String>>(args)?).await?),
                "install_defaults" => encode(core.install_default_addons().await?),
                // The curated list with its installed state, so a client can offer
                // each one without hardcoding URLs.
                "addon_catalog" => {
                    let installed = core.installed_manifest_urls().await?;
                    Ok(json!({
                        "recommended": DEFAULT_ADDONS.iter().map(|entry| json!({
                            "url": entry.url,
                            "name": entry.name,
                            "description": entry.description,
                            "installed": installed.iter().any(|url| url == entry.url),
                        })).collect::<Vec<_>>()
                    }))
                }
                "share" => { profiles.share(session, string(&args,"id"), string(&args,"target_id"), string(&args,"pin")).await?; Ok(Value::Null) }
                "linked_profiles" => encode(profiles.linked_profiles(session, string(&args,"id")).await?),
                "trakt_status" => {
                    let connected = profiles.trakt_connected(session.clone()).await?;
                    let data = profiles.trakt_data(session).await?;
                    Ok(json!({
                        "connected": connected,
                        "username": data.as_ref().map(|d| d.username.clone()).unwrap_or_default(),
                        "lists": data.map(|d| d.lists.len()).unwrap_or(0)
                    }))
                }
                "trakt_connect_start" => {
                    let redirect = string(&args,"redirect_uri");
                    let client = Client::new(Credentials {
                        client_id: string(&args,"client_id"),
                        client_secret: string(&args,"client_secret"),
                        redirect_uri: if redirect.is_empty() { "urn:ietf:wg:oauth:2.0:oob".into() } else { redirect },
                    })?;
                    let code = client.device_code().await?;
                    let response = json!({"user_code":code.user_code,"verification_url":code.verification_url,"expires_in":code.expires_in,"interval":code.interval});
                    *self.trakt_pending.lock().map_err(|_| invalid("Trakt state unavailable"))? = Some((client, code));
                    Ok(response)
                }
                "trakt_connect_poll" => {
                    let pending = self.trakt_pending.lock().map_err(|_| invalid("Trakt state unavailable"))?.take();
                    let Some((client, code)) = pending else { return Err(invalid("Start Trakt setup first")) };
                    match client.poll(&code).await? {
                        Poll::Pending => {
                            *self.trakt_pending.lock().map_err(|_| invalid("Trakt state unavailable"))? = Some((client, code));
                            Ok(json!({"status":"pending"}))
                        }
                        Poll::SlowDown(seconds) => {
                            *self.trakt_pending.lock().map_err(|_| invalid("Trakt state unavailable"))? = Some((client, code));
                            Ok(json!({"status":"slow_down","interval":seconds}))
                        }
                        Poll::Authorized(tokens) => { profiles.connect_trakt(session, client, tokens).await?; Ok(json!({"status":"authorized"})) }
                    }
                }
                "trakt_sync" => {
                    let data = profiles.sync_trakt(session, args["stale"].as_bool().unwrap_or(false)).await?;
                    Ok(json!({"lists":data.as_ref().map(|d| d.lists.len()).unwrap_or(0)}))
                }
                "trakt_disconnect" => Ok(json!({"revoked":profiles.disconnect_trakt(session).await?})),
                "query" => encode(core.query(&string(&args,"installation_id"), decode(args["request"].clone())?).await?),
                "query_all" => encode(core.query_all(decode(args)?).await?),
                "metadata" => encode(core.metadata(&decode(args["key"].clone())?, decode(args["preview"].clone())?).await?),
                "save" => encode(core.save_item(decode(args)?).await?),
                "remove" => encode(core.remove_item(&decode(args)?).await?),
                "hide_continue" => encode(core.set_continue_hidden(&decode(args["key"].clone())?, args["hidden"].as_bool().unwrap_or(true)).await?),
                "progress" => encode(core.record_progress(decode(args)?).await?),
                "preferences" => encode(core.set_playback_preferences(decode(args)?).await?),
                "prepare" => encode(core.prepare_playback(decode(args)?, &Media(self.media.clone())).await?),
                "revoke" => { self.media.revoke(&string(&args,"token")); Ok(Value::Null) }
                _ => Err(invalid("Unknown native operation")),
            }
        })
    }
}
/// Result of one sequential read from an open internal media reader.
///
/// This mirrors the sentinel values the Android player's data source expects, but
/// is expressed as a real type so the iOS resource loader can match on it too.
pub enum ReaderRead {
    /// The torrent piece is not available yet. Retry later; never end of stream.
    Pending,
    /// End of the stream.
    Eof,
    /// The next bytes in the stream.
    Data(Vec<u8>),
}
impl Bridge {
    /// Opens an internal media URI as a seekable reader and returns its handle.
    pub fn open_reader(&self, uri: &str, position: u64) -> Result<i64> {
        let mut file = self.runtime.block_on(self.media.open(uri))?;
        if position > file.length {
            return Err(invalid("Position exceeds file length"));
        }
        self.runtime
            .block_on(file.reader.seek(std::io::SeekFrom::Start(position)))
            .map_err(|_| invalid("Seek failed"))?;
        let id = self.next_reader.fetch_add(1, Ordering::Relaxed);
        self.readers
            .lock()
            .map_err(|_| invalid("Reader registry unavailable"))?
            .insert(id, Arc::new(Mutex::new(file)));
        Ok(id)
    }
    /// Total length of an open internal media stream.
    pub fn reader_length(&self, id: i64) -> Result<u64> {
        let reader = self.reader(id)?;
        let file = reader.lock().map_err(|_| invalid("Reader unavailable"))?;
        Ok(file.length)
    }
    /// Moves an open reader to `position` so the next read starts there.
    pub fn seek_reader(&self, id: i64, position: u64) -> Result<()> {
        let reader = self.reader(id)?;
        let mut file = reader.lock().map_err(|_| invalid("Reader unavailable"))?;
        if position > file.length {
            return Err(invalid("Position exceeds file length"));
        }
        self.runtime
            .block_on(file.reader.seek(std::io::SeekFrom::Start(position)))
            .map_err(|_| invalid("Seek failed"))?;
        Ok(())
    }
    /// Reads up to `length` bytes (capped at 256 KiB) from the reader's position.
    ///
    /// Waits at most one second for the torrent piece. [`ReaderRead::Pending`] means
    /// the caller should retry instead of treating it as end of stream.
    pub fn read_reader(&self, id: i64, length: usize) -> Result<ReaderRead> {
        if length == 0 {
            return Ok(ReaderRead::Eof);
        }
        let reader = self.reader(id)?;
        let mut file = reader.lock().map_err(|_| invalid("Reader unavailable"))?;
        let mut buffer = vec![0u8; length.min(256 * 1024)];
        let read = match self.runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(1), file.reader.read(&mut buffer)).await
        }) {
            Err(_) => return Ok(ReaderRead::Pending),
            Ok(result) => result.map_err(|_| invalid("Torrent read failed"))?,
        };
        if read == 0 {
            return Ok(ReaderRead::Eof);
        }
        buffer.truncate(read);
        Ok(ReaderRead::Data(buffer))
    }
    pub fn close_reader(&self, id: i64) {
        if let Ok(mut readers) = self.readers.lock() {
            readers.remove(&id);
        }
    }
    fn reader(&self, id: i64) -> Result<Arc<Mutex<MediaFile>>> {
        self.readers
            .lock()
            .map_err(|_| invalid("Reader registry unavailable"))?
            .get(&id)
            .cloned()
            .ok_or_else(|| invalid("Reader closed"))
    }
}
fn avatar_arg(args: &Value) -> Result<Option<String>> {
    match args.get("avatar") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(id)) if id.is_empty() => Ok(None),
        Some(Value::String(id)) => Ok(Some(id.clone())),
        _ => Err(invalid("Invalid profile image")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn avatar_catalog_and_profile_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let b = Bridge::open(dir.path().into()).unwrap();
        let choices = b.call("profiles", json!({})).unwrap();
        assert!(
            choices["avatars"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["id"] == "Fox.webp")
        );
        let profile = b
            .call("create_profile", json!({"name":"TV","avatar":"Fox.webp"}))
            .unwrap();
        assert_eq!(profile["avatar"], "Fox.webp");
        b.call("unlock", json!({"id":profile["id"]})).unwrap();
        assert!(
            b.call("update_profile", json!({"name":"TV","avatar":"Duck.webp"}))
                .is_err()
        );
        b.call("authorize", json!({})).unwrap();
        let renamed = b.call("update_profile", json!({"name":"Renamed"})).unwrap();
        assert_eq!(renamed["avatar"], "Fox.webp");
        assert!(
            b.call("update_profile", json!({"name":"Wrong","avatar":123}))
                .is_err()
        );
        let changed = b
            .call(
                "update_profile",
                json!({"name":"Renamed","avatar":"Black Cat.webp"}),
            )
            .unwrap();
        assert_eq!(changed["avatar"], "Black Cat.webp");
        drop(b);
        let b = Bridge::open(dir.path().into()).unwrap();
        assert_eq!(
            b.call("profiles", json!({})).unwrap()["profiles"][0]["avatar"],
            "Black Cat.webp"
        );
        b.call("unlock", json!({"id":profile["id"]})).unwrap();
        b.call("authorize", json!({})).unwrap();
        assert!(
            b.call("update_profile", json!({"name":"TV","avatar":""}))
                .unwrap()["avatar"]
                .is_null()
        );
    }

    #[test]
    fn profiles_persist_and_settings_require_authorization() {
        let dir = tempfile::tempdir().unwrap();
        let b = Bridge::open(dir.path().into()).unwrap();
        let profile = b
            .call(
                "create_profile",
                json!({"name":"Living room", "pin":"1234"}),
            )
            .unwrap();
        assert!(
            b.call("unlock", json!({"id":profile["id"],"pin":"0000"}))
                .is_err()
        );
        b.call("unlock", json!({"id":profile["id"],"pin":"1234"}))
            .unwrap();
        assert!(
            b.call("create_profile", json!({"name":"Kids","kids":true}))
                .is_err()
        );
        b.call("authorize", json!({"pin":"1234"})).unwrap();
        let kids = b
            .call("create_profile", json!({"name":"Kids","kids":true}))
            .unwrap();
        b.call("leave", json!({})).unwrap();
        b.call("unlock", json!({"id":kids["id"]})).unwrap();
        assert!(b.call("leave", json!({"pin":"0000"})).is_err());
        drop(b);
        let b = Bridge::open(dir.path().into()).unwrap();
        assert_eq!(
            b.call("profiles", json!({})).unwrap()["active_kids"]["id"],
            kids["id"]
        );
    }
}

#[cfg(test)]
mod flow_tests {
    use super::*;
    #[test]
    fn library_progress_resume_and_episode_policy_use_shared_core() {
        let dir = tempfile::tempdir().unwrap();
        let bridge = Bridge::open(dir.path().into()).unwrap();
        let profile = bridge.call("create_profile", json!({"name":"TV"})).unwrap();
        bridge.call("unlock", json!({"id":profile["id"]})).unwrap();
        let key = json!({"installation_id":"fixture","content_type":"series","item_id":"show"});
        let meta = json!({"id":"show","type":"series","name":"A shared story","poster":"https://example.com/poster.jpg","videos":[
            {"id":"first","title":"One","season":1,"episode":1,"released":"2020-01-01"},
            {"id":"special","title":"Special","season":0,"episode":1},
            {"id":"next","title":"Two","season":2,"episode":1,"released":"2020-02-01"},
            {"id":"future","title":"Future","season":2,"episode":2,"released":"2099-01-01"}
        ]});
        bridge
            .call(
                "save",
                json!({"key":key,"metadata":meta,"title":"A shared story"}),
            )
            .unwrap();
        let progress = json!({"key":key,"metadata":meta,"video_id":"first","position_ms":42000,"duration_ms":300000,"completed":false});
        bridge.call("progress", progress.clone()).unwrap();
        let source = json!({"url":"https://example.com/video.mp4","behaviorHints":{"proxyHeaders":{"request":{"Referer":"https://example.com/"}}}});
        let request = json!({"key":key,"video_id":"first","source":source,"capabilities":{"url_schemes":["http","https"],"request_headers":true}});
        let prepared = bridge.call("prepare", request.clone()).unwrap();
        assert_eq!(prepared["plan"]["resume_ms"], 42000);
        assert_eq!(
            prepared["delivery"]["request_headers"]["referer"],
            "https://example.com/"
        );
        let next = bridge
            .call(
                "episode",
                json!({"key":key,"meta":meta,"current":"first","today":"2026-09-09"}),
            )
            .unwrap();
        assert_eq!(next["video"]["id"], "next");
        assert!(
            bridge
                .call(
                    "episode",
                    json!({"key":key,"meta":meta,"current":"next","today":"2026-09-09"})
                )
                .unwrap()["video"]
                .is_null()
        );
        bridge
            .call("hide_continue", json!({"key":key,"hidden":true}))
            .unwrap();
        assert_eq!(
            bridge.call("snapshot", json!({})).unwrap()["progress"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let mut finished = progress.clone();
        finished["completed"] = json!(true);
        bridge.call("progress", finished).unwrap();
        assert_eq!(
            bridge.call("prepare", request).unwrap()["plan"]["resume_ms"],
            0
        );
        drop(bridge);
        let bridge = Bridge::open(dir.path().into()).unwrap();
        bridge.call("unlock", json!({"id":profile["id"]})).unwrap();
        let snapshot = bridge.call("snapshot", json!({})).unwrap();
        assert_eq!(snapshot["library"][0]["metadata"]["name"], "A shared story");
        assert_eq!(snapshot["progress"][0]["completed"], true);
        bridge.call("remove", key).unwrap();
        assert!(
            bridge.call("snapshot", json!({})).unwrap()["library"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
}

#[cfg(test)]
mod addon_flow_tests {
    use super::*;
    use std::io::{Read, Write};
    #[test]
    fn real_addon_http_flows_through_the_native_boundary() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let active = running.clone();
        let server = std::thread::spawn(move || {
            while active.load(Ordering::Relaxed) {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = [0; 4096];
                let Ok(n) = stream.read(&mut bytes) else {
                    continue;
                };
                let request = String::from_utf8_lossy(&bytes[..n]);
                let body = if request.contains("manifest.json") {
                    json!({"id":"dev.madari.fixture","version":"1.0.0","name":"TV fixture","resources":["catalog","meta","stream"],"types":["movie"],"catalogs":[{"type":"movie","id":"all","name":"Films","extra":[{"name":"search"},{"name":"skip"}]}]})
                } else if request.contains("/catalog/") {
                    json!({"metas":[{"id":"film","type":"movie","name":"Fixture film"}]})
                } else if request.contains("/meta/") {
                    json!({"meta":{"id":"film","type":"movie","name":"Fixture film","description":"Real HTTP metadata"}})
                } else {
                    json!({"streams":[{"name":"Direct","url":"https://example.com/film.mp4"}]})
                };
                let body = body.to_string();
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
            }
        });
        let result = std::panic::catch_unwind(|| {
            let dir = tempfile::tempdir().unwrap();
            let bridge = Bridge::open(dir.path().into()).unwrap();
            let profile = bridge.call("create_profile", json!({"name":"TV"})).unwrap();
            bridge.call("unlock", json!({"id":profile["id"]})).unwrap();
            bridge.call("authorize", json!({})).unwrap();
            bridge
                .call(
                    "install",
                    json!({"url":format!("{origin}/manifest.json"),"allow_local":true}),
                )
                .unwrap();
            let snapshot = bridge.call("snapshot", json!({})).unwrap();
            let provider = snapshot["addons"][0]["installation_id"].clone();
            let result=bridge.call("query",json!({"installation_id":provider,"request":{"resource":"catalog","type":"movie","id":"all","extra":{"search":"film","skip":"100"}}})).unwrap();
            assert_eq!(result["data"][0]["name"], "Fixture film");
            let key = json!({"installation_id":provider,"content_type":"movie","item_id":"film"});
            let meta = bridge.call("metadata", json!({"key":key})).unwrap();
            assert_eq!(meta["description"], "Real HTTP metadata");
            let sources = bridge
                .call(
                    "query_all",
                    json!({"resource":"stream","type":"movie","id":"film"}),
                )
                .unwrap();
            assert_eq!(sources[0]["result"]["Ok"]["data"][0]["name"], "Direct");
            bridge
                .call("enable", json!({"id":provider,"enabled":false}))
                .unwrap();
            assert!(
                bridge
                    .call(
                        "query_all",
                        json!({"resource":"stream","type":"movie","id":"film"})
                    )
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
        });
        running.store(false, Ordering::Relaxed);
        server.join().unwrap();
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }
}
