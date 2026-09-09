//! Real local HTTP, SQLite, BitTorrent peers, and FFmpeg. No mock player.
use axum::{Json, Router, http::StatusCode, routing::get};
use librqbit::{
    AddTorrent, AddTorrentOptions, CreateTorrentOptions, ListenerOptions, Session, SessionOptions,
    create_torrent, spawn_utils::BlockingSpawner,
};
use madari_core::Core;
use madari_media::{Ffmpeg, RqbitEngine, TorrentEngine};
use madari_native::{NativeHttp, SqliteStorage};
use madari_server::{AppState, router};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::process::Command;

const KEY: &str = "integration-only-key-000000000000000000000000";

async fn serve(app: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async { axum::serve(listener, app).await.unwrap() });
    (address, task)
}

fn offline_options() -> SessionOptions {
    SessionOptions {
        dht: None,
        disable_trackers: true,
        disable_local_service_discovery: true,
        ..Default::default()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn addon_flow_failure_isolation_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/manifest.json")).unwrap();
    let fixture2 = fixture.clone();
    let addon = Router::new()
        .route("/configured/manifest.json", get(move || async { Json(fixture) }))
        .route("/broken/manifest.json", get(move || async { Json(fixture2) }))
        .route("/configured/catalog/movie/all.json", get(|| async { Json(json!({"metas":[{"id":"test:1", "type":"movie", "name":"Test Film"}]})) }))
        .route("/configured/catalog/movie/all/search=film.json", get(|| async { Json(json!({"metas":[{"id":"test:1", "type":"movie", "name":"Test Film"}]})) }))
        .route("/configured/meta/movie/test%3A1.json", get(|| async { Json(json!({"meta":{"id":"test:1", "type":"movie", "name":"Test Film"}})) }))
        .route("/configured/stream/movie/test%3A1.json", get(|| async { Json(json!({"streams":[{"infoHash":"0123456789012345678901234567890123456789", "fileIdx":0, "behaviorHints":{"bingeGroup":"test"}}]})) }))
        .route("/configured/subtitles/movie/test%3A1.json", get(|| async { Json(json!({"subtitles":[{"id":"en", "lang":"eng", "url":"https://example.org/en.srt"}]})) }))
        .route("/configured/addon_catalog/all/community.json", get(|| async { Json(json!({"addons":[{"transportUrl":"https://example.org/manifest.json"}]})) }))
        .fallback(|| async { (StatusCode::BAD_GATEWAY, "unavailable") });
    let (addon_base, addon_task) = serve(addon).await;
    let db = dir.path().join("core.sqlite");
    let core = Arc::new(Core::new(
        Arc::new(NativeHttp::default()),
        Arc::new(SqliteStorage::open(&db).await.unwrap()),
    ));
    let torrents = Arc::new(RqbitEngine::from_session(
        Session::new_with_opts(dir.path().join("downloads"), offline_options())
            .await
            .unwrap(),
    ));
    let state = Arc::new(
        AppState::new(
            core.clone(),
            torrents.clone(),
            Ffmpeg::new("ffmpeg".into(), 1),
            KEY,
            "http://127.0.0.1:1".into(),
        )
        .unwrap(),
    );
    let (base, task) = serve(router(state, vec![])).await;
    let http = reqwest::Client::new();
    let specification: Value = http
        .get(format!("{base}/openapi.json"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(specification["openapi"], "3.1.0");
    assert!(specification["paths"]["/v1/torrents"]["post"].is_object());
    let docs = http
        .get(format!("{base}/docs/"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(docs.contains("swagger-ui"));
    let javascript = http
        .get(format!("{base}/docs/swagger-ui-bundle.js"))
        .send()
        .await
        .unwrap();
    assert_eq!(javascript.status(), StatusCode::OK);
    assert_eq!(
        http.get(format!("{base}/v1/snapshot"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        http.get(format!("{base}/v1/snapshot?api_key={KEY}"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let blocked = http
        .post(format!("{base}/v1/addons"))
        .bearer_auth(KEY)
        .json(&json!({"manifest_url":format!("{addon_base}/configured/manifest.json")}))
        .send()
        .await
        .unwrap();
    assert_eq!(blocked.status(), StatusCode::FORBIDDEN);
    let mut ids = vec![];
    for path in ["configured", "broken", "configured"] {
        let result: Value = http.post(format!("{base}/v1/addons")).bearer_auth(KEY).json(&json!({"manifest_url":format!("{addon_base}/{path}/manifest.json"), "allow_local":true})).send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
        ids.push(result["installation_id"].as_str().unwrap().to_owned());
    }
    assert_ne!(ids[0], ids[2]);
    for (resource, kind, id, extra) in [
        ("catalog", "movie", "all", json!({})),
        ("catalog", "movie", "all", json!({"search":"film"})),
        ("meta", "movie", "test:1", json!({})),
        ("stream", "movie", "test:1", json!({})),
        ("subtitles", "movie", "test:1", json!({})),
        ("addon_catalog", "all", "community", json!({})),
    ] {
        let result: Value = http
            .post(format!("{base}/v1/query"))
            .bearer_auth(KEY)
            .json(&json!({"resource":resource, "type":kind, "id":id, "extra":extra}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(result.as_array().unwrap().len(), 3, "{resource}: {result}");
        assert!(
            result[0]["result"]["Ok"].is_object(),
            "{resource}: {result}"
        );
        assert!(
            result[1]["result"]["Err"].is_object(),
            "{resource}: {result}"
        );
        assert_eq!(result[1]["result"]["Err"]["installation_id"], ids[1]);
        assert!(result[2]["result"]["Ok"].is_object());
    }
    let key = json!({"installation_id":ids[0], "content_type":"movie", "item_id":"test:1"});
    http.put(format!("{base}/v1/library"))
        .bearer_auth(KEY)
        .json(&json!({"key":key, "title":"Test Film"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    http.put(format!("{base}/v1/progress")).bearer_auth(KEY).json(&json!({"key":key, "video_id":"test:1", "position_ms":12500, "duration_ms":60000, "completed":false})).send().await.unwrap().error_for_status().unwrap();
    let direct_source = json!({"url":"https://media.example/film.mp4?token=test-only",
        "behaviorHints":{"proxyHeaders":{"request":{"User-Agent":"Madari"}},"bingeGroup":"test"},
        "subtitles":[{"id":"en","lang":"eng","url":"https://example.org/en.srt"}]});
    let request = json!({"source":direct_source,"capabilities":{"url_schemes":["https"],"request_headers":true},"key":key,"video_id":"test:1"});
    let prepared: Value = http
        .post(format!("{base}/v1/playback/prepare"))
        .bearer_auth(KEY)
        .json(&request)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(prepared["delivery"]["kind"], "direct");
    assert_eq!(prepared["delivery"]["url"], direct_source["url"]);
    assert_eq!(
        prepared["delivery"]["request_headers"]["user-agent"],
        "Madari"
    );
    assert_eq!(prepared["plan"]["resume_ms"], 12500);
    let mut cannot_headers = request.clone();
    cannot_headers["capabilities"]["request_headers"] = json!(false);
    let unsupported: Value = http
        .post(format!("{base}/v1/playback/prepare"))
        .bearer_auth(KEY)
        .json(&cannot_headers)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(unsupported["delivery"]["kind"], "unsupported");
    for source in [json!({"ytId":"test"}), json!({"infoHash":"a".repeat(40)})] {
        let unsupported: Value = http
            .post(format!("{base}/v1/playback/prepare"))
            .bearer_auth(KEY)
            .json(&json!({"source":source,"capabilities":{}}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(unsupported["delivery"]["kind"], "unsupported");
    }
    for invalid in [
        json!({"source":{"url":"file:///private"},"capabilities":{"url_schemes":["file"]}}),
        json!({"source":direct_source,"capabilities":{},"key":key}),
        json!({"source":direct_source,"capabilities":{},"file_index":0}),
        json!({"source":{"infoHash":"a".repeat(40),"url":"https://example.org"},"capabilities":{"companion":true}}),
    ] {
        let response = http
            .post(format!("{base}/v1/playback/prepare"))
            .bearer_auth(KEY)
            .json(&invalid)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
    assert!(torrents.list().unwrap().is_empty());
    let snapshot = core.snapshot().await.unwrap();
    assert_eq!(snapshot.revision, 5);
    assert!(
        !serde_json::to_string(&snapshot)
            .unwrap()
            .contains("/configured/")
    );
    task.abort();
    addon_task.abort();
    torrents.shutdown().await;
    drop(core);
    let reopened = Core::new(
        Arc::new(NativeHttp::default()),
        Arc::new(SqliteStorage::open(db).await.unwrap()),
    );
    let snapshot = reopened.snapshot().await.unwrap();
    assert_eq!(snapshot.library.len(), 1);
    assert_eq!(snapshot.progress[0].position_ms, 12500);
    assert_eq!(snapshot.addons.len(), 3); // addon_catalog never auto-installs.
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn real_torrent_range_stream_and_ffmpeg_transcode() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let dir = tempfile::tempdir().unwrap();
        let seed_dir = dir.path().join("seed");
        tokio::fs::create_dir_all(&seed_dir).await.unwrap();
        let video = seed_dir.join("test.mkv");
        let generated = Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=160x90:rate=10",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440",
                "-t",
                "3",
                "-c:v",
                "mpeg4",
                "-c:a",
                "pcm_s16le",
                "-threads",
                "1",
            ])
            .arg(&video)
            .output()
            .await
            .unwrap();
        assert!(
            generated.status.success(),
            "{}",
            String::from_utf8_lossy(&generated.stderr)
        );
        let expected = tokio::fs::read(&video).await.unwrap();
        let torrent = create_torrent(
            &video,
            CreateTorrentOptions {
                name: None,
                trackers: vec![],
                piece_length: Some(16384),
            },
            &BlockingSpawner::new(4),
        )
        .await
        .unwrap();
        let metainfo = torrent.as_bytes().unwrap();
        let seeder = Session::new_with_opts(
            seed_dir,
            SessionOptions {
                listen: Some(ListenerOptions {
                    listen_addr: "127.0.0.1:0".parse().unwrap(),
                    ..Default::default()
                }),
                ..offline_options()
            },
        )
        .await
        .unwrap();
        let seed = seeder
            .add_torrent(
                AddTorrent::from_bytes(metainfo.clone()),
                Some(AddTorrentOptions {
                    overwrite: true,
                    ..Default::default()
                }),
            )
            .await
            .unwrap()
            .into_handle()
            .unwrap();
        seed.wait_until_completed().await.unwrap();
        // BEP 23 compact peer list: real seeder discovered through a local tracker.
        let peer = seeder.listen_addr().unwrap();
        let mut reply = b"d8:intervali30e5:peers6:".to_vec();
        reply.extend_from_slice(&[127, 0, 0, 1]);
        reply.extend_from_slice(&peer.port().to_be_bytes());
        reply.push(b'e');
        let tracker = Router::new().route("/announce", get(move || {
            let reply = reply.clone();
            async move { ([("content-type", "application/x-bittorrent")], axum::body::Bytes::from(reply)) }
        }));
        let (tracker_base, tracker_task) = serve(tracker).await;
        let source = json!({"infoHash":torrent.info_hash().as_string(), "fileIdx":0,
            "sources":[format!("tracker:{tracker_base}/announce")],
            "subtitles":[{"id":"en", "lang":"eng", "url":"https://example.org/en.srt"}],
            "behaviorHints":{"bingeGroup":"integration", "filename":"test.mkv"}});
        let manifest: Value = serde_json::from_str(include_str!("../../../tests/fixtures/manifest.json")).unwrap();
        let addon = Router::new()
            .route("/manifest.json", get(move || { let manifest = manifest.clone(); async move { Json(manifest) } }))
            .route("/stream/movie/test%3A1.json", get(move || { let source = source.clone(); async move { Json(json!({"streams":[source]})) } }));
        let (addon_base, addon_task) = serve(addon).await;
        let downloader = Session::new_with_opts(dir.path().join("downloads"), SessionOptions {
            disable_trackers: false, ..offline_options()
        }).await.unwrap();
        let engine = Arc::new(RqbitEngine::from_session(downloader.clone()));
        let core = Arc::new(Core::new(
            Arc::new(NativeHttp::default()),
            Arc::new(
                SqliteStorage::open(dir.path().join("db.sqlite"))
                    .await
                    .unwrap(),
            ),
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(
            AppState::new(
                core,
                engine.clone(),
                Ffmpeg::new("ffmpeg".into(), 1),
                KEY,
                base.clone(),
            )
            .unwrap(),
        );
        let task =
            tokio::spawn(async { axum::serve(listener, router(state, vec![])).await.unwrap() });
        let http = reqwest::Client::new();
        let installed: Value = http.post(format!("{base}/v1/addons")).bearer_auth(KEY)
            .json(&json!({"manifest_url":format!("{addon_base}/manifest.json"), "allow_local":true}))
            .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
        let installation = installed["installation_id"].as_str().unwrap();
        let streams: Value = http.post(format!("{base}/v1/addons/{installation}/query")).bearer_auth(KEY)
            .json(&json!({"resource":"stream", "type":"movie", "id":"test:1"}))
            .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
        let source = &streams["data"][0];
        let key = json!({"installation_id":installation, "content_type":"movie", "item_id":"test:1"});
        http.put(format!("{base}/v1/progress")).bearer_auth(KEY)
            .json(&json!({"key":key, "video_id":"test:1", "position_ms":750, "duration_ms":3000, "completed":false}))
            .send().await.unwrap().error_for_status().unwrap();
        assert!(engine.list().unwrap().is_empty());
        let input = json!({"source":source, "capabilities":{"companion":true}, "key":key, "video_id":"test:1"});
        let response = http.post(format!("{base}/v1/playback/prepare")).bearer_auth(KEY).json(&input).send().await.unwrap();
        let status = response.status();
        let prepared: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::OK, "{prepared}");
        assert_eq!(prepared["plan"]["resume_ms"], 750);
        assert_eq!(prepared["delivery"]["kind"], "torrent");
        assert_eq!(prepared["delivery"]["selection"], "addon_index");
        assert_eq!(prepared["delivery"]["file"]["length"], expected.len() as u64);
        assert_eq!(prepared["plan"]["source"]["behaviorHints"]["bingeGroup"], "integration");
        assert_eq!(prepared["plan"]["source"]["subtitles"][0]["lang"], "eng");
        let ticket = &prepared["delivery"]["media"];
        assert_eq!(ticket["transcode_start_ms"], 750);
        assert!(ticket["transcode_path"].as_str().unwrap().ends_with("?start=0.750"));
        let id = prepared["delivery"]["torrent"]["id"].as_str().unwrap();
        let handle = downloader.get(librqbit::api::TorrentIdOrHash::parse(id).unwrap()).unwrap();
        assert_eq!(handle.stats().progress_bytes, 0, "preparation must not require a completed download");
        // Linux prepares through its own Core, retaining profile-local resume state.
        let companion = madari_native::companion::Companion::new(&base, KEY.into()).unwrap();
        companion.check().await.unwrap();
        assert!(madari_native::companion::Companion::new(&base, "x".repeat(32)).unwrap().check().await.is_err());
        let desktop = Core::new(Arc::new(NativeHttp::default()), Arc::new(SqliteStorage::open(dir.path().join("linux.sqlite")).await.unwrap()));
        let desktop_key: madari_model::ItemKey = serde_json::from_value(key.clone()).unwrap();
        desktop.record_progress(madari_model::Progress { metadata: None,binge_group:None,source_provider:None,key:desktop_key.clone(),video_id:"test:1".into(),position_ms:1250,duration_ms:Some(3000),completed:false}).await.unwrap();
        let desktop_prepared = desktop.prepare_playback(madari_model::PreparePlaybackRequest {
            source:serde_json::from_value(source.clone()).unwrap(),
            capabilities:madari_model::PlayerCapabilities {companion:true,..Default::default()},
            key:Some(desktop_key),video_id:Some("test:1".into()),file_index:None,
        }, &companion).await.unwrap();
        assert_eq!(desktop_prepared.plan.resume_ms,1250);
        let madari_model::PlaybackDelivery::Torrent {media,..} = desktop_prepared.delivery else {panic!("expected torrent delivery")};
        assert_eq!(media.transcode_start_ms,1250);
        assert!(media.transcode_path.ends_with("?start=1.250"));
        let media_url=companion.media_url(&media.direct_path).unwrap();
        companion.revoke(&media.token).await;
        assert_eq!(http.head(media_url).send().await.unwrap().status(),StatusCode::FORBIDDEN);
        // Invalid selection does not silently play a different file.
        let mut invalid_input = input.clone(); invalid_input["file_index"] = json!(999);
        let invalid = http.post(format!("{base}/v1/playback/prepare")).bearer_auth(KEY).json(&invalid_input).send().await.unwrap();
        assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
        // Repreparing uses the existing torrent and allows the explicit user override.
        let mut override_input = input.clone(); override_input["file_index"] = json!(0);
        let overridden: Value = http.post(format!("{base}/v1/playback/prepare")).bearer_auth(KEY).json(&override_input)
            .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
        assert_eq!(overridden["delivery"]["selection"], "user_override");
        assert_eq!(engine.list().unwrap().len(), 1);
        // Low-level metainfo upload remains compatible with an already managed torrent.
        let added: Value = http.post(format!("{base}/v1/torrents/metainfo")).bearer_auth(KEY).body(metainfo)
            .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
        assert_eq!(added["id"], id);
        let direct = format!("{base}{}", ticket["direct_path"].as_str().unwrap());
        let head = http.head(&direct).send().await.unwrap();
        assert_eq!(head.status(), StatusCode::OK);
        assert_eq!(head.headers()["content-length"], expected.len().to_string());
        let start = expected.len() - 4096;
        let range = http
            .get(&direct)
            .header("Range", format!("bytes={start}-"))
            .send()
            .await
            .unwrap();
        assert_eq!(range.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(&range.bytes().await.unwrap()[..], &expected[start..]);
        let invalid = http
            .get(&direct)
            .header("Range", "bytes=999999999-")
            .send()
            .await
            .unwrap();
        assert_eq!(invalid.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        let full = http
            .get(&direct)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .bytes()
            .await
            .unwrap();
        assert_eq!(&full[..], &expected[..]);
        let transcoded = http
            .get(format!(
                "{base}{}",
                ticket["transcode_path"].as_str().unwrap()
            ))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .bytes()
            .await
            .unwrap();
        let output = dir.path().join("output.mp4");
        tokio::fs::write(&output, transcoded).await.unwrap();
        let probe = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_name",
                "-of",
                "json",
            ])
            .arg(output)
            .output()
            .await
            .unwrap();
        assert!(probe.status.success());
        let probe: Value = serde_json::from_slice(&probe.stdout).unwrap();
        let codecs: Vec<_> = probe["streams"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["codec_name"].as_str().unwrap())
            .collect();
        assert!(
            codecs.contains(&"h264") && codecs.contains(&"aac"),
            "{probe}"
        );
        http.delete(format!(
            "{base}/v1/media/{}",
            ticket["token"].as_str().unwrap()
        ))
        .bearer_auth(KEY)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
        assert_eq!(
            http.get(direct).send().await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
        task.abort();
        engine.shutdown().await;
        seeder.stop().await;
        tracker_task.abort(); addon_task.abort();
    })
    .await
    .expect("real media test timed out");
}
