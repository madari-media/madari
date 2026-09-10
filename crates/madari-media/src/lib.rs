//! Native media adapters. The portable core does not link these dependencies.
use async_trait::async_trait;
use bytes::Bytes;
use futures::{Stream, StreamExt, stream};
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, Api, DhtSessionConfig, Session,
    SessionOptions, SessionPersistenceConfig, api::TorrentIdOrHash,
};
use madari_model::{Error, ErrorCode, Result, Torrent, TorrentFile};
use std::{
    io,
    path::{Path, PathBuf},
    pin::Pin,
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt},
    process::Command,
    sync::{OwnedSemaphorePermit, Semaphore},
};
use tokio_util::io::ReaderStream;
use url::Url;

pub trait MediaRead: AsyncRead + AsyncSeek + Unpin + Send {}
impl<T: AsyncRead + AsyncSeek + Unpin + Send> MediaRead for T {}
pub struct MediaFile {
    pub length: u64,
    pub mime: String,
    pub reader: Box<dyn MediaRead>,
}
pub type MediaBody = Pin<Box<dyn Stream<Item = io::Result<Bytes>> + Send>>;

pub enum TorrentInput {
    Magnet(String),
    Metainfo(Bytes),
}

#[derive(Clone, Debug)]
pub struct TorrentStats {
    pub state: String,
    pub downloaded: u64,
    pub total: u64,
    pub download_bytes_per_second: u64,
    pub upload_bytes_per_second: u64,
    pub peers: usize,
}

pub struct ManagedTorrent {
    pub id: String,
    pub name: String,
    pub download_directory: PathBuf,
    pub stats: TorrentStats,
}

#[async_trait]
pub trait TorrentEngine: Send + Sync {
    async fn prioritize_playback(&self, id: &str, file: usize) -> Result<()> {
        self.keep_file(id, file).await
    }
    async fn keep_file(&self, _id: &str, _file: usize) -> Result<()> {
        Ok(())
    }
    async fn set_paused(&self, _id: &str, _paused: bool) -> Result<()> {
        Err(Error::new(
            ErrorCode::Media,
            "Torrent management is unavailable.",
        ))
    }
    fn managed(&self) -> Vec<ManagedTorrent> {
        Vec::new()
    }
    fn stats(&self, _id: &str, _file: usize) -> Option<TorrentStats> {
        None
    }
    async fn add(&self, input: TorrentInput) -> Result<Torrent>;
    fn list(&self) -> Result<Vec<Torrent>>;
    fn details(&self, id: &str) -> Result<Torrent>;
    async fn open(&self, id: &str, file: usize) -> Result<MediaFile>;
    async fn remove(&self, id: &str) -> Result<()>;
    async fn shutdown(&self);
}

pub struct RqbitEngine {
    api: Api,
    additions: Semaphore,
}

fn media_error(_: impl std::fmt::Display) -> Error {
    Error::new(ErrorCode::Media, "torrent operation failed")
}
fn preparation_error(stage: &str, error: anyhow::Error) -> Error {
    // Classify the chain without exposing tracker credentials, magnets or local paths.
    let detail = format!("{error:#}");
    #[cfg(target_os = "android")]
    {
        #[link(name = "log")]
        unsafe extern "C" {
            fn __android_log_write(
                priority: i32,
                tag: *const std::ffi::c_char,
                text: *const std::ffi::c_char,
            ) -> i32;
        }
        // Local device diagnostics only; never return private tracker URLs in the UI.
        if let Ok(message) = std::ffi::CString::new(format!("{stage}: {detail}")) {
            unsafe {
                __android_log_write(6, c"MadariTorrent".as_ptr(), message.as_ptr());
            }
        }
    }
    let reason = if detail.contains("no known way to resolve peers") {
        "This source has no trackers and DHT is disabled. Choose a source with trackers or use an external media server."
    } else if detail.contains("input address stream exhausted") {
        "No reachable peers supplied the torrent metadata. Try another source."
    } else if let Some(error) = error.chain().find_map(|e| e.downcast_ref::<io::Error>()) {
        return Error::new(ErrorCode::Media, format!("{stage}: {}.", error.kind()));
    } else {
        "The torrent engine could not complete this step. Try another source."
    };
    Error::new(ErrorCode::Media, format!("{stage}: {reason}"))
}
fn torrent_id(id: &str) -> Result<TorrentIdOrHash> {
    if id.len() != 40 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "expected a torrent info hash",
        ));
    }
    TorrentIdOrHash::parse(id).map_err(media_error)
}

fn normalize(d: librqbit::api::TorrentDetailsResponse) -> Torrent {
    Torrent {
        id: d.info_hash,
        name: d.name,
        files: d
            .files
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(index, f)| TorrentFile {
                index,
                name: f.name,
                length: f.length,
            })
            .collect(),
    }
}

impl RqbitEngine {
    pub async fn new(data: &Path) -> Result<Self> {
        Self::with_network_mode(data, false).await
    }

    /// Embedded playback uses outgoing peers, trackers and DHT discovery without a peer listener.
    pub async fn embedded(data: &Path) -> Result<Self> {
        Self::with_network_mode(data, true).await
    }

    async fn with_network_mode(data: &Path, embedded: bool) -> Result<Self> {
        let session = Session::new_with_opts(
            data.join("downloads"),
            SessionOptions {
                persistence: Some(SessionPersistenceConfig::Json {
                    folder: Some(data.join("torrents")),
                }),
                // Keep all application state under the configured directory, not rqbit's global defaults.
                dht: Some(DhtSessionConfig {
                    persistence: None,
                    ..Default::default()
                }),
                ipv4_only: embedded,
                disable_local_service_discovery: true,
                listen: None,
                concurrent_init_limit: Some(2),
                peer_limit: Some(50),
                ..Default::default()
            },
        )
        .await
        .map_err(|e| preparation_error("Starting the torrent engine", e))?;
        Ok(Self::from_session(session))
    }

    pub fn from_session(session: Arc<Session>) -> Self {
        Self {
            api: Api::new(session, None),
            additions: Semaphore::new(2),
        }
    }
}

#[async_trait]
impl TorrentEngine for RqbitEngine {
    async fn prioritize_playback(&self, id: &str, file: usize) -> Result<()> {
        let selected = torrent_id(id)?;
        // TV has one active player: old torrents must not compete for its bandwidth.
        for torrent in self.api.api_torrent_list().torrents {
            if torrent.info_hash != id {
                let other = torrent_id(&torrent.info_hash)?;
                let handle = self.api.mgr_handle(other).map_err(media_error)?;
                if handle.live().is_some() {
                    self.api
                        .api_torrent_action_pause(other)
                        .await
                        .map_err(media_error)?;
                }
            }
        }
        self.api
            .api_torrent_action_update_only_files(
                selected,
                &std::collections::HashSet::from([file]),
            )
            .await
            .map_err(media_error)?;
        self.keep_file(id, file).await
    }

    async fn keep_file(&self, id: &str, file: usize) -> Result<()> {
        let id = torrent_id(id)?;
        let handle = self.api.mgr_handle(id).map_err(media_error)?;
        if let Some(files) = handle.only_files() {
            let mut files: std::collections::HashSet<_> = files.into_iter().collect();
            files.insert(file);
            self.api
                .api_torrent_action_update_only_files(id, &files)
                .await
                .map_err(media_error)?;
        }
        if handle.is_paused() {
            self.api
                .api_torrent_action_start(id)
                .await
                .map_err(media_error)?;
        }
        Ok(())
    }
    async fn set_paused(&self, id: &str, paused: bool) -> Result<()> {
        let id = torrent_id(id)?;
        let handle = self.api.mgr_handle(id).map_err(media_error)?;
        if handle.is_paused() != paused {
            if paused {
                self.api.api_torrent_action_pause(id).await
            } else {
                self.api.api_torrent_action_start(id).await
            }
            .map_err(media_error)?;
        }
        Ok(())
    }
    fn managed(&self) -> Vec<ManagedTorrent> {
        self.api
            .api_torrent_list()
            .torrents
            .into_iter()
            .filter_map(|torrent| {
                let id = torrent_id(&torrent.info_hash).ok()?;
                let stats = self.api.api_stats_v1(id).ok()?;
                let handle = self.api.mgr_handle(id).ok()?;
                let details = self.details(&torrent.info_hash).ok()?;
                let selected = handle.only_files();
                let files = details
                    .files
                    .iter()
                    .filter(|f| selected.as_ref().is_none_or(|s| s.contains(&f.index)));
                let (downloaded, total) = files.fold((0u64, 0u64), |(done, total), f| {
                    (
                        done.saturating_add(stats.file_progress.get(f.index).copied().unwrap_or(0)),
                        total.saturating_add(f.length),
                    )
                });
                let state = if stats.state.to_string() == "live" {
                    if total > 0 && downloaded >= total {
                        "Seeding"
                    } else if total == 0 {
                        "Idle"
                    } else {
                        "Downloading"
                    }
                } else {
                    match stats.state.to_string().as_str() {
                        "paused" => "Paused",
                        "error" => "Error",
                        _ => "Initializing",
                    }
                };
                Some(ManagedTorrent {
                    download_directory: PathBuf::from(torrent.output_folder),
                    id: torrent.info_hash,
                    name: torrent.name.unwrap_or_else(|| "Unnamed torrent".into()),
                    stats: TorrentStats {
                        state: state.into(),
                        downloaded,
                        total,
                        download_bytes_per_second: stats
                            .live
                            .as_ref()
                            .map_or(0, |s| s.download_speed.as_bytes()),
                        upload_bytes_per_second: stats
                            .live
                            .as_ref()
                            .map_or(0, |s| s.upload_speed.as_bytes()),
                        peers: stats
                            .live
                            .as_ref()
                            .map_or(0, |s| s.snapshot.peer_stats.live as usize),
                    },
                })
            })
            .collect()
    }
    fn stats(&self, id: &str, file: usize) -> Option<TorrentStats> {
        let stats = self.api.api_stats_v1(torrent_id(id).ok()?).ok()?;
        let details = self.details(id).ok()?;
        let total = details.files.iter().find(|f| f.index == file)?.length;
        Some(TorrentStats {
            state: stats.state.to_string(),
            downloaded: *stats.file_progress.get(file)?,
            total,
            download_bytes_per_second: stats
                .live
                .as_ref()
                .map_or(0, |s| s.download_speed.as_bytes()),
            upload_bytes_per_second: stats.live.as_ref().map_or(0, |s| s.upload_speed.as_bytes()),
            peers: stats
                .live
                .as_ref()
                .map_or(0, |s| s.snapshot.peer_stats.live as usize),
        })
    }
    async fn add(&self, input: TorrentInput) -> Result<Torrent> {
        let _permit = self
            .additions
            .try_acquire()
            .map_err(|_| Error::new(ErrorCode::Busy, "torrent initialization limit reached"))?;
        let add = match input {
            TorrentInput::Magnet(magnet) => {
                let source = madari_addon::torrent::normalize_magnet(&magnet)?;
                let id = torrent_id(&source.info_hash)?;
                // Repeated preparation of an existing torrent still works at capacity.
                if let Ok(handle) = self.api.mgr_handle(id) {
                    tokio::time::timeout(Duration::from_secs(60), handle.wait_until_initialized())
                        .await
                        .map_err(|_| {
                            Error::new(ErrorCode::Timeout, "torrent initialization timed out")
                        })?
                        .map_err(|e| preparation_error("Initializing the torrent", e))?;
                    return self
                        .api
                        .api_torrent_details(id)
                        .map(normalize)
                        .map_err(media_error);
                }
                AddTorrent::from_url(source.magnet)
            }
            TorrentInput::Metainfo(bytes) => {
                if bytes.is_empty() || bytes.len() > 4 * 1024 * 1024 {
                    return Err(Error::new(
                        ErrorCode::InvalidInput,
                        "torrent metainfo must be between 1 byte and 4 MiB",
                    ));
                }
                AddTorrent::from_bytes(bytes)
            }
        };
        if self.api.api_torrent_list().torrents.len() >= 64 {
            return Err(Error::new(
                ErrorCode::Busy,
                "remove a torrent before adding more (limit 64)",
            ));
        }
        // No bulk download until a file is streamed. Active readers drive piece priority.
        let response = tokio::time::timeout(
            Duration::from_secs(60),
            self.api.session().add_torrent(
                add,
                Some(AddTorrentOptions {
                    only_files: Some(Vec::new()),
                    ..Default::default()
                }),
            ),
        )
        .await
        .map_err(|_| Error::new(ErrorCode::Timeout, "torrent metadata lookup timed out"))?
        .map_err(|e| preparation_error("Finding torrent metadata", e))?;
        let (id, handle) = match response {
            AddTorrentResponse::Added(id, handle)
            | AddTorrentResponse::AlreadyManaged(id, handle) => (id, handle),
            AddTorrentResponse::ListOnly(_) => {
                return Err(Error::new(ErrorCode::Media, "torrent was not added"));
            }
        };
        tokio::time::timeout(Duration::from_secs(60), handle.wait_until_initialized())
            .await
            .map_err(|_| Error::new(ErrorCode::Timeout, "torrent initialization timed out"))?
            .map_err(|e| preparation_error("Initializing the torrent", e))?;
        self.api
            .api_torrent_details(TorrentIdOrHash::Id(id))
            .map(normalize)
            .map_err(media_error)
    }

    fn list(&self) -> Result<Vec<Torrent>> {
        self.api
            .api_torrent_list()
            .torrents
            .into_iter()
            .map(|d| self.details(&d.info_hash))
            .collect()
    }
    fn details(&self, id: &str) -> Result<Torrent> {
        self.api
            .api_torrent_details(torrent_id(id)?)
            .map(normalize)
            .map_err(|_| Error::new(ErrorCode::NotFound, "torrent not found"))
    }
    async fn open(&self, id: &str, file: usize) -> Result<MediaFile> {
        let details = self.details(id)?;
        let selected = details
            .files
            .get(file)
            .ok_or_else(|| Error::new(ErrorCode::NotFound, "torrent file not found"))?;
        let handle = self.api.mgr_handle(torrent_id(id)?).map_err(media_error)?;
        tokio::time::timeout(Duration::from_secs(60), handle.wait_until_initialized())
            .await
            .map_err(|_| {
                Error::new(
                    ErrorCode::Timeout,
                    "Torrent storage initialization timed out",
                )
            })?
            .map_err(|e| preparation_error("Opening torrent storage", e))?;
        let reader = self
            .api
            .api_stream(torrent_id(id)?, file)
            .await
            .map_err(|e| preparation_error("Opening torrent stream", e.into()))?;
        let mime = self
            .api
            .torrent_file_mime_type(torrent_id(id)?, file)
            .unwrap_or("application/octet-stream")
            .to_owned();
        Ok(MediaFile {
            length: selected.length,
            mime,
            reader: Box::new(reader),
        })
    }
    async fn remove(&self, id: &str) -> Result<()> {
        // Forget only: downloaded data is not deleted implicitly.
        self.api
            .api_torrent_action_forget(torrent_id(id)?)
            .await
            .map_err(media_error)?;
        Ok(())
    }
    async fn shutdown(&self) {
        self.api.session().stop().await;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub start: u64,
    pub length: u64,
    pub partial: bool,
}

/// RFC-style single byte ranges, including open-ended and suffix ranges.
pub fn byte_range(header: Option<&str>, length: u64) -> Result<ByteRange> {
    let invalid = || Error::new(ErrorCode::InvalidInput, "unsatisfiable byte range");
    let Some(header) = header else {
        return Ok(ByteRange {
            start: 0,
            length,
            partial: false,
        });
    };
    let spec = header.strip_prefix("bytes=").ok_or_else(invalid)?;
    if spec.contains(',') || length == 0 {
        return Err(invalid());
    }
    let (start, end) = spec.split_once('-').ok_or_else(invalid)?;
    let (start, end) = if start.is_empty() {
        let suffix: u64 = end.parse().map_err(|_| invalid())?;
        if suffix == 0 {
            return Err(invalid());
        }
        (length.saturating_sub(suffix), length - 1)
    } else {
        let start: u64 = start.parse().map_err(|_| invalid())?;
        let end: u64 = if end.is_empty() {
            length - 1
        } else {
            end.parse().map_err(|_| invalid())?
        };
        if start >= length || end < start {
            return Err(invalid());
        }
        (start, end.min(length - 1))
    };
    Ok(ByteRange {
        start,
        length: end - start + 1,
        partial: true,
    })
}

pub async fn read_range(
    mut file: MediaFile,
    range: ByteRange,
    permit: OwnedSemaphorePermit,
) -> Result<MediaBody> {
    file.reader
        .seek(io::SeekFrom::Start(range.start))
        .await
        .map_err(media_error)?;
    let reader = ReaderStream::with_capacity(file.reader.take(range.length), 64 * 1024);
    // Permit lives with the body; each blocked read has a deadline.
    Ok(Box::pin(stream::unfold(
        Some((reader, permit)),
        |state| async move {
            let (mut reader, permit) = state?;
            let next = tokio::time::timeout(Duration::from_secs(60), reader.next()).await;
            match next {
                Ok(Some(chunk)) => Some((chunk, Some((reader, permit)))),
                Ok(None) => None,
                Err(_) => Some((
                    Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "media read timed out",
                    )),
                    None,
                )),
            }
        },
    )))
}

pub struct Ffmpeg {
    executable: PathBuf,
    slots: Arc<Semaphore>,
}
impl Ffmpeg {
    pub fn new(executable: PathBuf, concurrency: usize) -> Self {
        Self {
            executable,
            slots: Arc::new(Semaphore::new(concurrency)),
        }
    }

    pub async fn available(&self) -> bool {
        tokio::time::timeout(
            Duration::from_secs(5),
            Command::new(&self.executable)
                .arg("-version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .status(),
        )
        .await
        .is_ok_and(|r| r.is_ok_and(|s| s.success()))
    }

    /// Input must be Madari's authenticated loopback media URL, never arbitrary input.
    /// A new request with start_seconds seeks by restarting the transcode.
    pub async fn transcode(&self, input: &str, start_seconds: f64) -> Result<MediaBody> {
        let url = Url::parse(input)
            .map_err(|_| Error::new(ErrorCode::InvalidInput, "invalid transcoder input"))?;
        if url.scheme() != "http"
            || !matches!(url.host_str(), Some("127.0.0.1" | "[::1]"))
            || !url.path().starts_with("/media/")
            || !start_seconds.is_finite()
            || !(0.0..=604800.0).contains(&start_seconds)
        {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "transcoder requires a loopback media input and valid start time",
            ));
        }
        let permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::new(ErrorCode::Busy, "transcoding capacity reached"))?;
        let mut command = Command::new(&self.executable);
        command
            .args([
                "-nostdin",
                "-hide_banner",
                "-loglevel",
                "error",
                "-threads",
                "2",
                "-filter_threads",
                "1",
                "-protocol_whitelist",
                "http,tcp",
                "-format_whitelist",
                "matroska,webm,mov,mp4,m4a,3gp,3g2,mj2,avi,mpegts,ogg,mpeg,flv",
                "-rw_timeout",
                "60000000",
                "-ss",
            ])
            .arg(start_seconds.to_string())
            .arg("-i")
            .arg(input)
            .args([
                "-map",
                "0:v:0",
                "-map",
                "0:a:0?",
                "-sn",
                "-dn",
                "-c:v",
                "libx264",
                "-threads",
                "2",
                "-preset",
                "veryfast",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-ac",
                "2",
                "-movflags",
                "frag_keyframe+empty_moov+default_base_moof",
                "-f",
                "mp4",
                "pipe:1",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .map_err(|_| Error::new(ErrorCode::Media, "could not start FFmpeg"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::new(ErrorCode::Media, "FFmpeg output unavailable"))?;
        let mut reader = ReaderStream::with_capacity(stdout, 64 * 1024);
        // Read first output before committing HTTP 200. Failure is still a structured error.
        let first = tokio::time::timeout(Duration::from_secs(60), reader.next())
            .await
            .map_err(|_| Error::new(ErrorCode::Timeout, "FFmpeg produced no output"))?
            .ok_or_else(|| Error::new(ErrorCode::Media, "FFmpeg could not decode this source"))?
            .map_err(|_| Error::new(ErrorCode::Media, "FFmpeg output failed"))?;
        let tail = stream::unfold(Some((reader, child, permit)), |state| async move {
            let (mut reader, mut child, permit) = state?;
            match tokio::time::timeout(Duration::from_secs(60), reader.next()).await {
                Ok(Some(chunk)) => Some((chunk, Some((reader, child, permit)))),
                Ok(None) => match child.wait().await {
                    Ok(status) if status.success() => None,
                    _ => Some((Err(io::Error::other("FFmpeg exited unsuccessfully")), None)),
                },
                Err(_) => Some((
                    Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "FFmpeg output timed out",
                    )),
                    None,
                )),
            }
        });
        Ok(Box::pin(stream::once(async { Ok(first) }).chain(tail)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "requires torrent-engine network socket initialization"]
    async fn selected_files_keep_seeding_until_paused_or_removed() {
        use librqbit::{CreateTorrentOptions, create_torrent, spawn_utils::BlockingSpawner};
        let dir = tempfile::tempdir().unwrap();
        let downloads = dir.path().join("downloads");
        tokio::fs::create_dir_all(&downloads).await.unwrap();
        let path = downloads.join("sample.bin");
        tokio::fs::write(&path, vec![42u8; 65536]).await.unwrap();
        let torrent = create_torrent(
            &path,
            CreateTorrentOptions {
                name: None,
                trackers: vec![],
                piece_length: Some(16384),
            },
            &BlockingSpawner::new(2),
        )
        .await
        .unwrap();
        let engine = RqbitEngine::embedded(dir.path()).await.unwrap();
        // Preseed only this fixture; production never overwrites unrelated files.
        let handle = engine
            .api
            .session()
            .add_torrent(
                AddTorrent::from_bytes(torrent.as_bytes().unwrap()),
                Some(AddTorrentOptions {
                    overwrite: true,
                    only_files: Some(Vec::new()),
                    ..Default::default()
                }),
            )
            .await
            .unwrap()
            .into_handle()
            .unwrap();
        handle.wait_until_initialized().await.unwrap();
        let torrent = engine.list().unwrap().remove(0);
        engine.keep_file(&torrent.id, 0).await.unwrap();
        let other_path = downloads.join("other.bin");
        tokio::fs::write(&other_path, vec![7u8; 32768])
            .await
            .unwrap();
        let other = create_torrent(
            &other_path,
            CreateTorrentOptions {
                name: None,
                trackers: vec![],
                piece_length: Some(16384),
            },
            &BlockingSpawner::new(2),
        )
        .await
        .unwrap();
        let other_handle = engine
            .api
            .session()
            .add_torrent(
                AddTorrent::from_bytes(other.as_bytes().unwrap()),
                Some(AddTorrentOptions {
                    overwrite: true,
                    ..Default::default()
                }),
            )
            .await
            .unwrap()
            .into_handle()
            .unwrap();
        other_handle.wait_until_initialized().await.unwrap();
        engine.prioritize_playback(&torrent.id, 0).await.unwrap();
        assert!(
            other_handle.is_paused(),
            "TV playback pauses competing torrents"
        );
        assert_eq!(handle.only_files(), Some(vec![0]));
        engine
            .api
            .api_torrent_action_forget(torrent_id(&other_handle.info_hash().as_string()).unwrap())
            .await
            .unwrap();
        let reader = engine.open(&torrent.id, 0).await.unwrap();
        drop(reader);
        let managed = engine.managed();
        assert_eq!(managed[0].stats.total, 65536);
        assert_eq!(managed[0].stats.state, "Seeding");
        engine.set_paused(&torrent.id, true).await.unwrap();
        assert_eq!(engine.managed()[0].stats.state, "Paused");
        engine.set_paused(&torrent.id, false).await.unwrap();
        assert_eq!(engine.managed()[0].stats.state, "Seeding");
        engine.remove(&torrent.id).await.unwrap();
        assert!(engine.managed().is_empty());
        assert!(path.exists(), "Remove must retain downloaded files");
        engine.shutdown().await;
    }

    #[tokio::test]
    #[ignore = "requires torrent-engine network socket initialization"]
    async fn embedded_trackerless_magnet_has_peer_discovery() {
        let dir = tempfile::tempdir().unwrap();
        let engine = RqbitEngine::embedded(dir.path()).await.unwrap();
        assert!(engine.api.session().get_dht().is_some());
        // An unknown hash should wait for peers, not fail immediately for lack of discovery.
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(250),
            engine.add(TorrentInput::Magnet(format!(
                "magnet:?xt=urn:btih:{}",
                "a".repeat(40)
            ))),
        )
        .await;
        engine.shutdown().await;
        assert!(
            result.is_err(),
            "trackerless magnets must enter peer discovery"
        );
    }

    #[test]
    fn preparation_errors_keep_stage_without_leaking_private_input() {
        let error =
            anyhow::anyhow!("no known way to resolve peers (https://tracker.example/secret)");
        let error = preparation_error("Finding torrent metadata", error);
        assert!(error.message.contains("no trackers"));
        assert!(!error.message.contains("secret"));
        let error = anyhow::Error::new(io::Error::from(io::ErrorKind::PermissionDenied))
            .context("private path");
        let error = preparation_error("Starting the torrent engine", error);
        assert!(error.message.contains("permission denied"));
        assert!(!error.message.contains("private path"));
    }

    #[test]
    fn player_range_requests() {
        assert_eq!(
            byte_range(Some("bytes=10-19"), 100).unwrap(),
            ByteRange {
                start: 10,
                length: 10,
                partial: true
            }
        );
        assert_eq!(byte_range(Some("bytes=-20"), 100).unwrap().start, 80);
        assert_eq!(byte_range(Some("bytes=90-"), 100).unwrap().length, 10);
        assert_eq!(byte_range(Some("bytes=90-500"), 100).unwrap().length, 10);
        for value in ["bytes=100-", "bytes=-0", "bytes=20-10", "bytes=0-1,4-5"] {
            assert!(byte_range(Some(value), 100).is_err());
        }
        assert!(byte_range(Some("bytes=0-"), 0).is_err());
    }
}
