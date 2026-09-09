use madari_core::Core;
use madari_media::{Ffmpeg, RqbitEngine, TorrentEngine};
use madari_native::{NativeHttp, SqliteStorage, private_directory};
use madari_server::{AppState, router};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("--openapi") {
        println!("{}", madari_server::openapi::document().to_pretty_json()?);
        return Ok(());
    }
    let key = std::env::var("MADARI_API_KEY").map_err(|_| {
        anyhow::anyhow!("set MADARI_API_KEY to a random secret of at least 32 characters")
    })?;
    let bind: SocketAddr = std::env::var("MADARI_BIND")
        .unwrap_or_else(|_| "127.0.0.1:11470".into())
        .parse()?;
    let data = PathBuf::from(std::env::var_os("MADARI_DATA_DIR").unwrap_or_else(|| "data".into()));
    private_directory(&data)?;
    let storage = Arc::new(SqliteStorage::open(data.join("core.sqlite")).await?);
    let core = Arc::new(Core::new(Arc::new(NativeHttp::default()), storage));
    let torrents = Arc::new(RqbitEngine::new(&data).await?);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    let local = listener.local_addr()?;
    let loopback_ip = if local.ip().is_unspecified() {
        if local.is_ipv4() {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        } else {
            IpAddr::V6(Ipv6Addr::LOCALHOST)
        }
    } else {
        local.ip()
    };
    // A non-loopback-specific listener cannot be used for FFmpeg's private input.
    anyhow::ensure!(
        local.ip().is_unspecified() || local.ip().is_loopback(),
        "bind to loopback or an unspecified address (0.0.0.0 / ::) so FFmpeg can use loopback"
    );
    let internal = format!("http://{}", SocketAddr::new(loopback_ip, local.port()));
    let ffmpeg = Ffmpeg::new(
        std::env::var_os("MADARI_FFMPEG")
            .map(PathBuf::from)
            .unwrap_or_else(|| "ffmpeg".into()),
        2,
    );
    let origins = std::env::var("MADARI_ALLOWED_ORIGINS")
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let url = url::Url::parse(s)?;
            anyhow::ensure!(
                matches!(url.scheme(), "http" | "https") && url.origin().ascii_serialization() == s,
                "CORS entries must be exact HTTP(S) origins"
            );
            Ok(s.parse::<http::HeaderValue>()?)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let state = Arc::new(AppState::new(
        core,
        torrents.clone(),
        ffmpeg,
        &key,
        internal,
    )?);
    eprintln!("Madari companion listening on {local}");
    let result = axum::serve(listener, router(state, origins))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
    torrents.shutdown().await;
    result?;
    Ok(())
}
