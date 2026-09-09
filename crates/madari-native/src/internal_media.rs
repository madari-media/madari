//! In-process media delivery: no HTTP listener or local network endpoint.
pub use madari_media::ManagedTorrent;
pub use madari_media::MediaFile;
pub use madari_media::TorrentStats;
use madari_media::{RqbitEngine, TorrentEngine, TorrentInput};
use madari_model::{Error, ErrorCode, MediaTicket, Result, Torrent};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::OnceCell;

struct Ticket {
    torrent: String,
    file: usize,
    expires: Instant,
}
pub struct InternalMedia {
    data: PathBuf,
    engine: OnceCell<Arc<dyn TorrentEngine>>,
    tickets: Mutex<HashMap<String, Ticket>>,
}
impl InternalMedia {
    pub async fn start(&self) -> Result<()> {
        self.engine().await.map(|_| ())
    }
    pub fn managed(&self) -> Vec<ManagedTorrent> {
        self.engine
            .get()
            .map_or_else(Vec::new, |engine| engine.managed())
    }
    pub async fn set_paused(&self, id: &str, paused: bool) -> Result<()> {
        self.engine().await?.set_paused(id, paused).await
    }
    pub async fn remove(&self, id: &str) -> Result<()> {
        self.engine().await?.remove(id).await?;
        self.tickets
            .lock()
            .unwrap()
            .retain(|_, ticket| ticket.torrent != id);
        Ok(())
    }
    pub fn new(data: PathBuf) -> Self {
        Self {
            data,
            engine: OnceCell::new(),
            tickets: Mutex::new(HashMap::new()),
        }
    }
    /// Supply a platform engine while retaining scoped ticket and stream handling.
    pub fn from_engine(engine: Arc<dyn TorrentEngine>) -> Self {
        Self {
            data: PathBuf::new(),
            engine: OnceCell::new_with(Some(engine)),
            tickets: Mutex::new(HashMap::new()),
        }
    }
    async fn engine(&self) -> Result<&Arc<dyn TorrentEngine>> {
        self.engine
            .get_or_try_init(|| async {
                crate::private_directory(&self.data).map_err(|_| {
                    Error::new(ErrorCode::Media, "Could not create internal media storage.")
                })?;
                Ok(Arc::new(RqbitEngine::embedded(&self.data).await?) as Arc<dyn TorrentEngine>)
            })
            .await
    }
    pub async fn resolve(&self, magnet: String) -> Result<Torrent> {
        self.engine().await?.add(TorrentInput::Magnet(magnet)).await
    }
    pub async fn prioritize_playback(&self, id: &str, file: usize) -> Result<()> {
        self.engine().await?.prioritize_playback(id, file).await
    }
    pub async fn ticket(&self, id: &str, file: usize) -> Result<MediaTicket> {
        if !self
            .engine()
            .await?
            .details(id)?
            .files
            .iter()
            .any(|f| f.index == file)
        {
            return Err(Error::new(ErrorCode::NotFound, "Torrent file not found."));
        }
        let token = uuid::Uuid::new_v4().simple().to_string();
        self.engine().await?.keep_file(id, file).await?;
        let mut tickets = self.tickets.lock().unwrap();
        tickets.retain(|_, ticket| ticket.expires > Instant::now());
        if tickets.len() >= 256 {
            return Err(Error::new(
                ErrorCode::Busy,
                "Too many active media tickets.",
            ));
        }
        tickets.insert(
            token.clone(),
            Ticket {
                torrent: id.into(),
                file,
                expires: Instant::now() + Duration::from_secs(21600),
            },
        );
        Ok(MediaTicket {
            direct_path: format!("/media/{token}/original"),
            transcode_path: String::new(),
            token,
            expires_in_seconds: 21600,
            transcode_start_ms: 0,
        })
    }
    pub fn revoke(&self, token: &str) {
        self.tickets.lock().unwrap().remove(token);
    }
    pub fn stats(&self, token: &str) -> Option<TorrentStats> {
        let tickets = self.tickets.lock().ok()?;
        let ticket = tickets.get(token).filter(|t| t.expires > Instant::now())?;
        self.engine.get()?.stats(&ticket.torrent, ticket.file)
    }
    pub async fn open(&self, uri: &str) -> Result<MediaFile> {
        let token = uri
            .strip_prefix("madari-internal://")
            .ok_or_else(|| Error::new(ErrorCode::InvalidInput, "Invalid internal media URI."))?;
        let (id, file) = {
            let tickets = self.tickets.lock().unwrap();
            let ticket = tickets
                .get(token)
                .filter(|t| t.expires > Instant::now())
                .ok_or_else(|| {
                    Error::new(ErrorCode::Forbidden, "Internal media ticket expired.")
                })?;
            (ticket.torrent.clone(), ticket.file)
        };
        self.engine().await?.open(&id, file).await
    }
    pub async fn shutdown(&self) {
        if let Some(engine) = self.engine.get() {
            engine.shutdown().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_engine_starts_without_tcp_listeners() {
        // Isolate socket inspection from other tests' HTTP fixtures.
        if std::env::var_os("MADARI_INTERNAL_SOCKET_TEST").is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "internal_media::tests::embedded_engine_starts_without_tcp_listeners",
                    "--nocapture",
                ])
                .env("MADARI_INTERNAL_SOCKET_TEST", "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        runtime.block_on(async {
            let media = InternalMedia::new(dir.path().join("media"));
            media.engine().await.unwrap();
            let sockets: std::collections::HashSet<String> = std::fs::read_dir("/proc/self/fd")
                .unwrap()
                .flatten()
                .filter_map(|entry| std::fs::read_link(entry.path()).ok())
                .filter_map(|target| {
                    target
                        .to_str()?
                        .strip_prefix("socket:[")?
                        .strip_suffix(']')
                        .map(str::to_owned)
                })
                .collect();
            // UDP sockets are used for outgoing tracker requests, never a media API.
            for table in ["/proc/self/net/tcp", "/proc/self/net/tcp6"] {
                for line in std::fs::read_to_string(table).unwrap().lines().skip(1) {
                    let fields: Vec<_> = line.split_whitespace().collect();
                    if fields.len() > 9 && sockets.contains(fields[9]) {
                        assert_ne!(fields[3], "0A", "embedded engine opened a TCP listener");
                    }
                }
            }
            media.shutdown().await;
        });
    }
}
