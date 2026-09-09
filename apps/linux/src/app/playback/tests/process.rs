//! Linux native mpv process, controlled only through private JSON IPC.
use madari_core::Core;
use madari_model::{Error, ErrorCode, ItemKey, Progress, Result};
use serde_json::{Value, json};
use std::{collections::BTreeMap, process::Stdio, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    process::Command,
    sync::oneshot,
    time::{Instant, timeout},
};

use super::Target;

fn error(message: &str) -> Error {
    Error::new(ErrorCode::Media, message)
}
async fn send(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    command: Value,
    id: u64,
) -> Result<()> {
    let mut bytes = serde_json::to_vec(&json!({"command":command,"request_id":id}))
        .map_err(|_| error("Could not encode player command."))?;
    bytes.push(b'\n');
    writer
        .write_all(&bytes)
        .await
        .map_err(|_| error("The player connection closed."))
}
fn milliseconds(value: &Value) -> Option<u64> {
    value
        .as_f64()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| (v * 1000.0) as u64)
}

async fn play_inner(
    core: Arc<Core>,
    key: ItemKey,
    video_id: String,
    target: Target,
    mut stop: oneshot::Receiver<()>,
    headless: bool,
) -> Result<()> {
    // A 0700 temporary directory keeps the IPC socket inaccessible to other users.
    let directory = tempfile::Builder::new()
        .prefix("madari-player-")
        .tempdir()
        .map_err(|_| error("Could not create player socket directory."))?;
    let socket = directory.path().join("ipc");
    let mut command = Command::new(std::env::var_os("MADARI_MPV").unwrap_or_else(|| "mpv".into()));
    command
        .env_remove("MADARI_COMPANION_API_KEY")
        .env_remove("MADARI_API_KEY")
        .args([
            "--no-config",
            "--load-scripts=no",
            "--ytdl=no",
            "--terminal=no",
            "--idle=yes",
            "--force-window=yes",
            "--title=Madari",
            "--keep-open=no",
            "--save-position-on-quit=no",
            "--resume-playback=no",
            "--access-references=no",
        ])
        .arg(format!("--input-ipc-server={}", socket.display()))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    if headless {
        command.args(["--vo=null", "--ao=null", "--force-window=no"]);
    }
    let mut child = command
        .spawn()
        .map_err(|_| error("Could not start mpv. Install mpv 0.38 or newer, or set MADARI_MPV."))?;
    let stream = timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(stream) = UnixStream::connect(&socket).await {
                break Ok(stream);
            }
            if child
                .try_wait()
                .map_err(|_| error("Could not check the player."))?
                .is_some()
            {
                break Err(error("mpv exited before playback started."));
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .map_err(|_| error("mpv did not open its control connection."))??;
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    send(&mut writer, json!(["observe_property", 1, "time-pos"]), 1).await?;
    send(&mut writer, json!(["observe_property", 2, "duration"]), 2).await?;
    // URLs and headers go over IPC, never the process command line or logs.
    let headers: Vec<_> = target
        .headers
        .iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect();
    send(
        &mut writer,
        json!(["set_property", "http-header-fields", headers]),
        3,
    )
    .await?;
    send(&mut writer,json!(["loadfile",target.url,"replace",-1,{"start":format!("{:.3}",target.resume_ms as f64/1000.0)}]),4).await?;
    let mut loaded = false;
    let mut position = None;
    let mut duration = None;
    let mut completed = false;
    let mut playback_error = None;
    let mut last_save = Instant::now();
    let loading_deadline = tokio::time::sleep(Duration::from_secs(60));
    tokio::pin!(loading_deadline);
    loop {
        tokio::select! {
            _=&mut stop => break,
            _=&mut loading_deadline, if !loaded => {playback_error=Some(error("Playback did not start within one minute."));break;}
            line=lines.next_line() => {
                let Some(line)=line.map_err(|_|error("Could not read player state."))? else {break;};
                let event:Value=serde_json::from_str(&line).map_err(|_|error("Invalid player response."))?;
                if event.get("request_id").and_then(Value::as_u64).is_some_and(|id|(1..=4).contains(&id)) && event.get("error").and_then(Value::as_str).is_some_and(|v|v!="success") {
                    playback_error=Some(error("mpv rejected the playback options. Check your mpv version."));break;
                }
                match event["event"].as_str() {
                    Some("file-loaded") => {
                        loaded=true;
                        // Embedded tracks use mpv's native selector. External subtitles must be HTTP(S).
                        // Do not send media credentials to subtitle hosts.
                        if target.headers.is_empty() {
                            for subtitle in &target.subtitles {
                                if url::Url::parse(&subtitle.url).is_ok_and(|u|matches!(u.scheme(),"http"|"https") && u.host_str().is_some()) {
                                    send(&mut writer,json!(["sub-add",subtitle.url,"auto",subtitle.lang,subtitle.lang]),10).await?;
                                }
                            }
                        }
                    }
                    Some("property-change") => match event["name"].as_str() {
                        Some("time-pos") => if let Some(ms)=milliseconds(&event["data"]) {position=Some(ms.saturating_add(target.offset_ms));},
                        Some("duration") => if let Some(ms)=milliseconds(&event["data"]) {duration=Some(ms.saturating_add(target.offset_ms));},
                        _=>(),
                    },
                    Some("end-file") => {
                        match event["reason"].as_str() {
                            Some("eof") => {
                                // EOF can also mean a truncated network stream. Require proximity to a known end.
                                completed=loaded && position.zip(duration).is_some_and(|(p,d)|d>0 && p>=d.saturating_sub((d/20).min(1500)));
                                if !loaded {playback_error=Some(error("The source ended without playable media."));}
                                break;
                            }
                            Some("error") => {playback_error=Some(error("mpv could not play this source. Try another source or companion transcoding."));break;}
                            Some("redirect") => {playback_error=Some(error("Playlist redirects are not supported in this player integration."));break;}
                            _=>break,
                        }
                    }
                    Some("shutdown") => break,
                    _=>(),
                }
                if loaded && last_save.elapsed()>=Duration::from_secs(2) {
                    if let Some(position_ms)=position {core.record_progress(Progress { metadata: None,binge_group:None,source_provider:None,key:key.clone(),video_id:video_id.clone(),position_ms:duration.map_or(position_ms,|d|position_ms.min(d)),duration_ms:duration,completed:false}).await?;}
                    last_save=Instant::now();
                }
            }
        }
    }
    let _ = send(&mut writer, json!(["quit"]), 99).await;
    if timeout(Duration::from_secs(3), child.wait()).await.is_err() {
        let _ = child.kill().await;
    }
    if loaded && let Some(position_ms) = position {
        core.record_progress(Progress {
            metadata: None,
            binge_group: None,
            source_provider: None,
            key,
            video_id,
            position_ms: if completed {
                duration.unwrap_or(position_ms)
            } else {
                duration.map_or(position_ms, |d| position_ms.min(d))
            },
            duration_ms: duration,
            completed,
        })
        .await?;
    }
    if let Some(error) = playback_error {
        Err(error)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, routing::get};
    use madari_native::{NativeHttp, SqliteStorage};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn real_mpv_decodes_http_and_persists_resume_stop_and_completion() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("clip.mp4");
        let output = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=160x90:rate=15",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=44100",
                "-t",
                "5",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-movflags",
                "+faststart",
            ])
            .arg(&file)
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let bytes = tokio::fs::read(file).await.unwrap();
        let requests = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = requests.clone();
        let app = Router::new().route(
            "/clip.mp4",
            get(move |headers: axum::http::HeaderMap| {
                let bytes = bytes.clone();
                let seen = seen.clone();
                async move {
                    assert_eq!(headers.get("x-madari-test").unwrap(), "private-header");
                    seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    ([("content-type", "video/mp4")], bytes)
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/clip.mp4", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let db = dir.path().join("state.sqlite");
        let core = Arc::new(Core::new(
            Arc::new(NativeHttp::default()),
            Arc::new(SqliteStorage::open(&db).await.unwrap()),
        ));
        let key = ItemKey {
            installation_id: "provider".into(),
            content_type: "movie".into(),
            item_id: "film".into(),
        };
        let target = || Target {
            title: "Test metadata title".into(),
            context: Default::default(),
            url: url.clone(),
            headers: BTreeMap::from([("X-Madari-Test".into(), "private-header".into())]),
            resume_ms: 1000,
            offset_ms: 0,
            subtitles: vec![],
        };
        let (stop, receive) = oneshot::channel();
        let playback = tokio::spawn(play_inner(
            core.clone(),
            key.clone(),
            "film".into(),
            target(),
            receive,
            true,
        ));
        timeout(Duration::from_secs(15), async {
            loop {
                let state = core.snapshot().await.unwrap();
                if state
                    .progress
                    .first()
                    .is_some_and(|p| p.position_ms >= 2000)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .unwrap();
        stop.send(()).unwrap();
        timeout(Duration::from_secs(5), playback)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let progress = core.snapshot().await.unwrap().progress.remove(0);
        assert!(!progress.completed);
        assert!(progress.position_ms >= 2000);
        let reopened = Arc::new(Core::new(
            Arc::new(NativeHttp::default()),
            Arc::new(SqliteStorage::open(&db).await.unwrap()),
        ));
        let mut resumed = target();
        resumed.resume_ms = progress.position_ms;
        let (_stop, receive) = oneshot::channel();
        timeout(
            Duration::from_secs(15),
            play_inner(
                reopened.clone(),
                key.clone(),
                "film".into(),
                resumed,
                receive,
                true,
            ),
        )
        .await
        .unwrap()
        .unwrap();
        let progress = reopened.snapshot().await.unwrap().progress.remove(0);
        assert!(progress.completed);
        assert!(progress.position_ms >= 4900);
        let mut invalid = target();
        invalid.url = format!("{url}/missing");
        let (_stop, receive) = oneshot::channel();
        assert!(
            timeout(
                Duration::from_secs(10),
                play_inner(reopened.clone(), key, "film".into(), invalid, receive, true)
            )
            .await
            .unwrap()
            .is_err()
        );
        let preserved = reopened.snapshot().await.unwrap().progress.remove(0);
        assert!(preserved.completed);
        assert_eq!(preserved.position_ms, progress.position_ms);
        assert!(requests.load(std::sync::atomic::Ordering::SeqCst) >= 2);
        server.abort();
    }

    #[test]
    fn invalid_timestamps_do_not_become_progress() {
        assert_eq!(milliseconds(&json!(-1)), None);
        assert_eq!(milliseconds(&Value::Null), None);
        assert_eq!(milliseconds(&json!(1.25)), Some(1250));
    }
}
