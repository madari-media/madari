//! Native HTTP and SQLite adapters. Blocking SQLite operations run off the executor.
pub mod companion;
pub mod internal_media;
pub mod profiles;
pub mod trakt;
use async_trait::async_trait;
use futures::StreamExt;
use madari_core::{Http, Snapshot, Storage};
use madari_model::*;
use rusqlite::{Connection, params};
use std::{
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::Semaphore;
use url::{Host, Url};

pub struct NativeHttp {
    permits: Semaphore,
}
impl Default for NativeHttp {
    fn default() -> Self {
        Self {
            permits: Semaphore::new(8),
        }
    }
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let o = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.is_unspecified()
                || ip.is_multicast()
                || o[0] == 0
                || o[0] >= 240
                || (o[0] == 100 && (64..=127).contains(&o[1]))
                || (o[0] == 198 && (18..=19).contains(&o[1]))
                || (o[0] == 192 && o[1] == 0 && o[2] == 0))
        }
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return public_ip(v4.into());
            }
            // Admit global unicast only; exclude documentation and transition ranges.
            let s = ip.segments();
            (s[0] & 0xe000) == 0x2000
                && s[0] != 0x2002
                && !(s[0] == 0x2001 && (s[1] < 0x0200 || s[1] == 0x0db8))
        }
    }
}

impl NativeHttp {
    async fn fetch(&self, url: Url, allow_local: bool) -> Result<Vec<u8>> {
        for attempt in 0..3 {
            let result = self.fetch_once(url.clone(), allow_local).await;
            if attempt == 2
                || !result
                    .as_ref()
                    .is_err_and(|e| matches!(e.code, ErrorCode::Network | ErrorCode::Timeout))
            {
                return result;
            }
            tokio::time::sleep(Duration::from_millis(300 * (1 << attempt))).await;
        }
        unreachable!()
    }
    async fn fetch_once(&self, mut url: Url, allow_local: bool) -> Result<Vec<u8>> {
        let _permit = self
            .permits
            .acquire()
            .await
            .map_err(|_| Error::new(ErrorCode::Network, "HTTP adapter closed"))?;
        for _ in 0..6 {
            if !matches!(url.scheme(), "http" | "https")
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err(Error::new(
                    ErrorCode::Forbidden,
                    "URL scheme or userinfo is not allowed",
                ));
            }
            let port = url
                .port_or_known_default()
                .ok_or_else(|| Error::new(ErrorCode::InvalidInput, "missing HTTP port"))?;
            let host = url
                .host()
                .ok_or_else(|| Error::new(ErrorCode::InvalidInput, "missing HTTP host"))?;
            let addresses: Vec<SocketAddr> = match host {
                Host::Ipv4(ip) => vec![SocketAddr::new(ip.into(), port)],
                Host::Ipv6(ip) => vec![SocketAddr::new(ip.into(), port)],
                Host::Domain(name) => tokio::net::lookup_host((name, port))
                    .await
                    .map_err(|_| Error::new(ErrorCode::Network, "addon DNS lookup failed"))?
                    .collect(),
            };
            if addresses.is_empty() {
                return Err(Error::new(
                    ErrorCode::Network,
                    "addon has no resolved address",
                ));
            }
            if !allow_local && addresses.iter().any(|a| !public_ip(a.ip())) {
                return Err(Error::new(
                    ErrorCode::Forbidden,
                    "local-network addon requires explicit permission",
                ));
            }
            // Pin validated DNS answers; validate redirects manually so every hop obeys policy.
            let mut builder = reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(20));
            if let Host::Domain(name) = host {
                builder = builder.resolve_to_addrs(name, &addresses);
            }
            let client = builder
                .build()
                .map_err(|_| Error::new(ErrorCode::Network, "HTTP client initialization failed"))?;
            let response = client
                .get(url.clone())
                .send()
                .await
                .map_err(|_| Error::new(ErrorCode::Network, "addon request failed"))?;
            if matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| {
                        Error::new(
                            ErrorCode::InvalidResponse,
                            "addon redirect has no valid destination",
                        )
                    })?;
                url = url.join(location).map_err(|_| {
                    Error::new(ErrorCode::InvalidResponse, "invalid addon redirect")
                })?;
                continue;
            }
            if !response.status().is_success() {
                return Err(Error::new(
                    if matches!(
                        response.status().as_u16(),
                        408 | 429 | 500 | 502 | 503 | 504
                    ) {
                        ErrorCode::Network
                    } else {
                        ErrorCode::InvalidResponse
                    },
                    format!("addon returned HTTP {}", response.status().as_u16()),
                ));
            }
            const LIMIT: usize = 4 * 1024 * 1024;
            if response.content_length().is_some_and(|n| n > LIMIT as u64) {
                return Err(Error::new(
                    ErrorCode::ResponseTooLarge,
                    "addon response exceeds 4 MiB",
                ));
            }
            let mut bytes = Vec::new();
            let mut chunks = response.bytes_stream();
            while let Some(chunk) = chunks.next().await {
                let chunk = chunk
                    .map_err(|_| Error::new(ErrorCode::Network, "addon response interrupted"))?;
                if bytes.len() + chunk.len() > LIMIT {
                    return Err(Error::new(
                        ErrorCode::ResponseTooLarge,
                        "addon response exceeds 4 MiB",
                    ));
                }
                bytes.extend_from_slice(&chunk);
            }
            return Ok(bytes);
        }
        Err(Error::new(
            ErrorCode::InvalidResponse,
            "too many addon redirects",
        ))
    }
}

#[async_trait]
impl Http for NativeHttp {
    async fn get_json(&self, url: Url, allow_local: bool) -> Result<serde_json::Value> {
        let bytes = self.get_bytes(url, allow_local).await?;
        serde_json::from_slice(&bytes)
            .map_err(|_| Error::new(ErrorCode::InvalidResponse, "addon returned invalid JSON"))
    }
}
impl NativeHttp {
    /// Artwork redirects are followed only after revalidating every hop.
    pub async fn get_artwork(&self, url: Url, allow_local: bool) -> Result<Vec<u8>> {
        tokio::time::timeout(Duration::from_secs(25), self.fetch(url, allow_local))
            .await
            .map_err(|_| Error::new(ErrorCode::Timeout, "image request timed out"))?
    }
    /// Bounded HTTP bytes with the same transport and local-network policy as addons.
    pub async fn get_bytes(&self, url: Url, allow_local: bool) -> Result<Vec<u8>> {
        tokio::time::timeout(Duration::from_secs(25), self.fetch(url, allow_local))
            .await
            .map_err(|_| Error::new(ErrorCode::Timeout, "request timed out"))?
    }
}

pub struct SqliteStorage {
    connection: Arc<Mutex<Connection>>,
}
fn storage_error(_: impl std::fmt::Display) -> Error {
    Error::new(ErrorCode::Storage, "local storage operation failed")
}

/// Creates an owner-only directory on Unix. Windows deployments must set a private ACL.
pub fn private_directory(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

impl SqliteStorage {
    pub async fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        tokio::task::spawn_blocking(move || {
            let mut conn = Connection::open(path).map_err(storage_error)?;
            conn.busy_timeout(Duration::from_secs(5)).map_err(storage_error)?;
            let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).map_err(storage_error)?;
            if version > 1 { return Err(Error::new(ErrorCode::Storage, "database schema is newer than this application")); }
            conn.pragma_update(None, "journal_mode", "WAL").map_err(storage_error)?;
            let tx = conn.transaction().map_err(storage_error)?;
            tx.execute_batch("CREATE TABLE IF NOT EXISTS metadata_cache (cache_key TEXT PRIMARY KEY, fetched_at INTEGER NOT NULL, data TEXT NOT NULL); CREATE TABLE IF NOT EXISTS core_state (id INTEGER PRIMARY KEY CHECK(id=1), revision INTEGER NOT NULL, data TEXT NOT NULL); PRAGMA user_version=1;").map_err(storage_error)?;
            tx.execute("INSERT OR IGNORE INTO core_state VALUES (1, 0, ?1)", [serde_json::to_string(&Snapshot::default()).map_err(storage_error)?]).map_err(storage_error)?;
            tx.commit().map_err(storage_error)?;
            Ok(Self { connection: Arc::new(Mutex::new(conn)) })
        }).await.map_err(storage_error)?
    }
}

#[async_trait]
impl Storage for SqliteStorage {
    async fn cached_metadata(&self) -> Result<Vec<madari_core::CachedMetadata>> {
        let conn = self.connection.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(storage_error)?;
            let mut query = conn
                .prepare("SELECT data FROM metadata_cache ORDER BY fetched_at")
                .map_err(storage_error)?;
            let rows = query
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(storage_error)?;
            let mut entries = Vec::new();
            for row in rows {
                if let Ok(entry) = serde_json::from_str(&row.map_err(storage_error)?) {
                    entries.push(entry);
                }
            }
            Ok(entries)
        })
        .await
        .map_err(storage_error)?
    }
    async fn cache_metadata(&self, entries: Vec<madari_core::CachedMetadata>) -> Result<()> {
        let conn = self.connection.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().map_err(storage_error)?;
            let tx = conn.transaction().map_err(storage_error)?;
            for entry in entries {
                tx.execute("INSERT INTO metadata_cache(cache_key,fetched_at,data) VALUES(?1,?2,?3) ON CONFLICT(cache_key) DO UPDATE SET fetched_at=excluded.fetched_at,data=excluded.data",
                    params![serde_json::to_string(&entry.key).map_err(storage_error)?, entry.fetched_at, serde_json::to_string(&entry).map_err(storage_error)?]).map_err(storage_error)?;
            }
            tx.execute("DELETE FROM metadata_cache WHERE cache_key NOT IN (SELECT cache_key FROM metadata_cache ORDER BY fetched_at DESC LIMIT 100)", []).map_err(storage_error)?;
            tx.commit().map_err(storage_error)?;
            Ok(())
        }).await.map_err(storage_error)?
    }

    async fn load(&self) -> Result<Snapshot> {
        let conn = self.connection.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(storage_error)?;
            let data: String = conn
                .query_row("SELECT data FROM core_state WHERE id=1", [], |r| r.get(0))
                .map_err(storage_error)?;
            serde_json::from_str(&data).map_err(storage_error)
        })
        .await
        .map_err(storage_error)?
    }

    async fn compare_and_swap(&self, expected_revision: u64, next: Snapshot) -> Result<()> {
        if next.revision != expected_revision.saturating_add(1) || next.revision > i64::MAX as u64 {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "invalid snapshot revision",
            ));
        }
        let conn = self.connection.clone();
        tokio::task::spawn_blocking(move || {
            let data = serde_json::to_string(&next).map_err(storage_error)?;
            let conn = conn.lock().map_err(storage_error)?;
            let changed = conn
                .execute(
                    "UPDATE core_state SET revision=?1, data=?2 WHERE id=1 AND revision=?3",
                    params![next.revision, data, expected_revision],
                )
                .map_err(storage_error)?;
            if changed != 1 {
                return Err(Error::new(ErrorCode::Conflict, "snapshot revision changed"));
            }
            Ok(())
        })
        .await
        .map_err(storage_error)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn metadata_cache_survives_restart_without_changing_progress_revision() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.sqlite");
        let storage = SqliteStorage::open(&path).await.unwrap();
        let before = storage.load().await.unwrap().revision;
        let entry = madari_core::CachedMetadata {
            key: ItemKey {
                installation_id: "addon".into(),
                content_type: "series".into(),
                item_id: "tt1".into(),
            },
            resolved: madari_core::ResolvedMetadata {
                meta: Meta {
                    id: "tt1".into(),
                    content_type: "series".into(),
                    name: "Show".into(),
                    videos: vec![],
                    extra: Default::default(),
                },
                field_providers: Default::default(),
            },
            fetched_at: 123,
            sources: vec![],
        };
        storage.cache_metadata(vec![entry.clone()]).await.unwrap();
        storage.cache_metadata(vec![entry]).await.unwrap();
        assert_eq!(storage.load().await.unwrap().revision, before);
        drop(storage);
        let reopened = SqliteStorage::open(path).await.unwrap();
        let cached = reopened.cached_metadata().await.unwrap();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].resolved.meta.name, "Show");
    }

    #[tokio::test]
    async fn transient_gets_retry_but_permanent_errors_do_not() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        for (status, expected, succeeds) in [
            (503, 3, true),
            (502, 3, false),
            (401, 1, false),
            (404, 1, false),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url =
                Url::parse(&format!("http://{}/test", listener.local_addr().unwrap())).unwrap();
            let count = Arc::new(AtomicUsize::new(0));
            let seen = count.clone();
            let server = tokio::spawn(async move {
                loop {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let mut buffer = [0; 4096];
                    let _ = socket.read(&mut buffer).await.unwrap();
                    let attempt = seen.fetch_add(1, Ordering::SeqCst) + 1;
                    let status = if succeeds && attempt == 3 {
                        200
                    } else {
                        status
                    };
                    let response = format!(
                        "HTTP/1.1 {status} Test\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
                    );
                    socket.write_all(response.as_bytes()).await.unwrap();
                }
            });
            let result = NativeHttp::default().get_json(url, true).await;
            assert_eq!(result.is_ok(), succeeds);
            assert_eq!(count.load(Ordering::SeqCst), expected);
            server.abort();
        }
    }

    #[tokio::test]
    async fn state_survives_restart_and_stale_writes_fail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("core.sqlite");
        let storage = SqliteStorage::open(&path).await.unwrap();
        let mut snapshot = storage.load().await.unwrap();
        snapshot.revision = 1;
        snapshot.library.push(LibraryEntry {
            metadata: None,
            key: ItemKey {
                installation_id: "a".into(),
                content_type: "custom".into(),
                item_id: "opaque".into(),
            },
            title: "Film".into(),
        });
        storage.compare_and_swap(0, snapshot.clone()).await.unwrap();
        assert_eq!(
            storage
                .compare_and_swap(0, snapshot)
                .await
                .unwrap_err()
                .code,
            ErrorCode::Conflict
        );
        drop(storage);
        let reopened = SqliteStorage::open(path)
            .await
            .unwrap()
            .load()
            .await
            .unwrap();
        assert_eq!(reopened.revision, 1);
        assert_eq!(reopened.library[0].title, "Film");
    }

    #[tokio::test]
    async fn artwork_and_metadata_follow_validated_redirects() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut buffer = [0; 4096];
                    let n = socket.read(&mut buffer).await.unwrap();
                    let request = String::from_utf8_lossy(&buffer[..n]);
                    let response = if request.starts_with("GET /art ") {
                        "HTTP/1.1 302 Found\r\nLocation: /image\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    } else if request.starts_with("GET /meta ") {
                        "HTTP/1.1 307 Temporary Redirect\r\nLocation: /json\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    } else if request.starts_with("GET /json ") {
                        "HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"meta\":{}}"
                    } else if request.starts_with("GET /loop ") {
                        "HTTP/1.1 308 Permanent Redirect\r\nLocation: /loop\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    } else if request.starts_with("GET /invalid ") {
                        "HTTP/1.1 302 Found\r\nLocation: file:///secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    } else {
                        "HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nimage"
                    };
                    socket.write_all(response.as_bytes()).await.unwrap();
                });
            }
        });
        let http = NativeHttp::default();
        let url = Url::parse(&format!("{origin}/art")).unwrap();
        assert_eq!(http.get_artwork(url.clone(), true).await.unwrap(), b"image");
        assert_eq!(http.get_bytes(url.clone(), true).await.unwrap(), b"image");
        assert_eq!(
            http.get_json(Url::parse(&format!("{origin}/meta")).unwrap(), true)
                .await
                .unwrap(),
            serde_json::json!({"meta": {}})
        );
        for (path, code) in [
            ("loop", ErrorCode::InvalidResponse),
            ("invalid", ErrorCode::Forbidden),
        ] {
            assert_eq!(
                http.get_json(Url::parse(&format!("{origin}/{path}")).unwrap(), true)
                    .await
                    .unwrap_err()
                    .code,
                code
            );
        }
        assert_eq!(
            http.get_artwork(url, false).await.unwrap_err().code,
            ErrorCode::Forbidden
        );
        server.abort();
    }

    #[test]
    fn special_addresses_are_not_public() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "::1",
            "::ffff:127.0.0.1",
            "fc00::1",
            "2002:7f00:1::",
            "64:ff9b::7f00:1",
        ] {
            assert!(!public_ip(ip.parse().unwrap()), "{ip}");
        }
        assert!(public_ip("1.1.1.1".parse().unwrap()));
    }

    #[tokio::test]
    async fn local_http_requires_permission_and_redacts_urls() {
        let error = NativeHttp::default()
            .get_json(
                Url::parse("http://127.0.0.1:9/secret/manifest.json").unwrap(),
                false,
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Forbidden);
        assert!(!error.to_string().contains("secret"));
    }
}
