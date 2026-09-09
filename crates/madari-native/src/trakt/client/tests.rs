use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn fixture(
    responses: Vec<(u16, &'static str, Value)>,
) -> (Client, tokio::task::JoinHandle<Vec<String>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut requests = Vec::new();
        for (status, headers, body) in responses {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let n = stream.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&bytes[..end]);
                    let len = head
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|n| n.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            requests.push(String::from_utf8(bytes).unwrap());
            let body = body.to_string();
            let response = format!(
                "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n{headers}\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
        requests
    });
    let mut client = Client::new(Credentials {
        client_id: "test-client".into(),
        client_secret: "test-secret".into(),
        redirect_uri: "urn:ietf:wg:oauth:2.0:oob".into(),
    })
    .unwrap();
    client.api = origin.clone();
    client.auth = origin;
    (client, task)
}
fn tokens() -> Tokens {
    Tokens {
        access_token: "test-access".into(),
        refresh_token: "test-refresh".into(),
        created_at: now(),
        expires_in: 604800,
    }
}
#[tokio::test]
async fn device_pending_slowdown_success_and_refresh_use_correct_payloads() {
    let (client,task)=fixture(vec![
        (200,"",json!({"device_code":"private-code","user_code":"ABCD1234","verification_url":"https://auth.trakt.tv/activate","expires_in":600,"interval":5})),
        (400,"",json!({})),(429,"Retry-After: 11\r\n",json!({})),
        (200,"",json!({"access_token":"new-access","refresh_token":"new-refresh","created_at":now(),"expires_in":604800})),
        (200,"",json!({"access_token":"rotated-access","refresh_token":"rotated-refresh","created_at":now(),"expires_in":604800})),
    ]).await;
    let code = client.device_code().await.unwrap();
    assert!(matches!(client.poll(&code).await.unwrap(), Poll::Pending));
    assert!(matches!(
        client.poll(&code).await.unwrap(),
        Poll::SlowDown(11)
    ));
    let Poll::Authorized(tokens) = client.poll(&code).await.unwrap() else {
        panic!()
    };
    let updated = client.refresh(&tokens).await.unwrap();
    assert_eq!(updated.refresh_token, "rotated-refresh");
    let requests = task.await.unwrap();
    assert!(requests[0].starts_with("POST /oauth/device/code"));
    assert!(!requests[0].contains("client_secret"));
    assert!(requests[1].contains("\"code\":\"private-code\""));
    assert!(requests[4].contains("\"refresh_token\":\"new-refresh\""));
    assert!(requests[4].contains("\"grant_type\":\"refresh_token\""));
}
#[tokio::test]
async fn pagination_follows_headers_even_when_first_page_is_short() {
    let (client, task) = fixture(vec![
        (200, "X-Pagination-Page-Count: 2\r\n", json!([{"id":1}])),
        (200, "X-Pagination-Page-Count: 2\r\n", json!([{"id":2}])),
    ])
    .await;
    let rows = client.pages("/sync/watchlist", &tokens()).await.unwrap();
    assert_eq!(rows.len(), 2);
    let requests = task.await.unwrap();
    assert!(requests[1].contains("page=2"));
    assert!(requests[1].contains("authorization: Bearer test-access"));
}
#[tokio::test]
async fn invalid_activation_urls_and_denial_do_not_leak_response_bodies() {
    let (client,task)=fixture(vec![(200,"",json!({"device_code":"secret-code","user_code":"code","verification_url":"https://evil.example/activate","expires_in":600,"interval":5}))]).await;
    assert!(client.device_code().await.is_err());
    task.await.unwrap();
    let (client, task) = fixture(vec![(418, "", json!({"private":"do-not-show"}))]).await;
    let code:DeviceCode=serde_json::from_value(json!({"device_code":"secret","user_code":"code","verification_url":"https://trakt.tv/activate","expires_in":600,"interval":5})).unwrap();
    let Err(error) = client.poll(&code).await else {
        panic!()
    };
    assert!(!error.message.contains("do-not-show"));
    assert!(error.message.contains("declined"));
    task.await.unwrap();
}

#[tokio::test]
async fn pull_includes_all_personal_lists_history_and_watchlist_without_remote_writes() {
    let movie = json!({"title":"Movie","year":2024,"ids":{"trakt":1,"imdb":"tt1"}});
    let show = json!({"title":"Show","ids":{"trakt":2,"imdb":"tt2"}});
    let (client,task)=fixture(vec![
        (200,"",json!({"user":{"username":"fixture-user"}})),
        (200,"",json!([{"movie":movie}])),
        (200,"",json!([{"movie":movie,"watched_at":"2026-09-09T00:00:00.000Z"},{"show":show,"episode":{"season":2,"number":3}}])),
        (200,"",json!([{"name":"Private favorites","ids":{"trakt":88}},{"name":"Weekend","ids":{"trakt":99}}])),
        (200,"X-Pagination-Page-Count: 2\r\n",json!([{"show":show}])),
        (200,"X-Pagination-Page-Count: 2\r\n",json!([{"movie":movie}])),
        (200,"",json!([])),
    ]).await;
    let data = client.pull(&tokens()).await.unwrap();
    assert_eq!(data.username, "fixture-user");
    assert_eq!(data.watchlist.len(), 1);
    assert_eq!(data.history.len(), 2);
    assert_eq!(data.lists.len(), 2);
    assert_eq!(data.lists[0].items.len(), 2);
    assert!(data.lists[1].items.is_empty());
    assert!(data.synced_at > 0);
    let requests = task.await.unwrap();
    assert!(requests.iter().all(|r| r.starts_with("GET ")));
    assert!(requests[5].contains("/users/me/lists/88/items/movie,show,season,episode?page=2"));
}
#[tokio::test]
async fn expired_device_code_does_not_poll_again() {
    let (client, task) = fixture(Vec::new()).await;
    let mut code:DeviceCode=serde_json::from_value(json!({"device_code":"secret","user_code":"code","verification_url":"https://trakt.tv/activate","expires_in":1,"interval":5})).unwrap();
    code.issued_at = std::time::Instant::now() - Duration::from_secs(2);
    assert!(client.poll(&code).await.is_err());
    assert!(task.await.unwrap().is_empty());
}

#[tokio::test]
async fn scrobble_posts_confirmed_states_and_recognizes_duplicates() {
    use crate::trakt::{Action, Media, Outcome, Scrobble};
    let (client, task) = fixture(vec![
        (201, "", json!({"action":"start"})),
        (201, "", json!({"action":"pause"})),
        (201, "", json!({"action":"scrobble"})),
        (
            409,
            "",
            json!({"watched_at":"2026-09-09T00:00:00Z","expires_at":"2026-09-09T01:00:00Z"}),
        ),
        (201, "", json!({"action":"unexpected"})),
    ])
    .await;
    let key = madari_model::ItemKey {
        installation_id: "addon".into(),
        content_type: "movie".into(),
        item_id: "tt123".into(),
    };
    let media = Media::from_video(&key, "tt123", &[]).unwrap();
    for (action, expected) in [
        (Action::Start, Outcome::Watching),
        (Action::Pause, Outcome::Paused),
        (Action::Stop, Outcome::Watched),
        (Action::Stop, Outcome::Watched),
    ] {
        assert_eq!(
            client
                .scrobble(
                    &tokens(),
                    &Scrobble {
                        media: media.clone(),
                        action,
                        progress: 96.0
                    }
                )
                .await
                .unwrap(),
            expected
        );
    }
    assert!(
        client
            .scrobble(
                &tokens(),
                &Scrobble {
                    media,
                    action: Action::Stop,
                    progress: 96.0
                }
            )
            .await
            .is_err()
    );
    let requests = task.await.unwrap();
    for (index, name) in ["start", "pause", "stop", "stop", "stop"]
        .iter()
        .enumerate()
    {
        assert!(requests[index].starts_with(&format!("POST /scrobble/{name}")));
        assert!(requests[index].contains("authorization: Bearer test-access"));
        assert!(requests[index].contains("\"movie\":{\"ids\":{\"imdb\":\"tt123\"}}"));
        assert!(!requests[index].contains("client_secret"));
    }
}

#[tokio::test]
async fn episode_scrobbles_resolve_and_cache_the_episode_id() {
    use crate::trakt::{Action, Media, Scrobble};
    let (client, task) = fixture(vec![
        (200, "", json!({"season":2,"number":4,"ids":{"trakt":777}})),
        (201, "", json!({"action":"start"})),
        (201, "", json!({"action":"pause"})),
    ])
    .await;
    let key = madari_model::ItemKey {
        installation_id: "addon".into(),
        content_type: "series".into(),
        item_id: "tt123".into(),
    };
    let video = serde_json::from_value(json!({"id":"custom-id","season":2,"episode":4})).unwrap();
    let media = Media::from_video(&key, "custom-id", &[video]).unwrap();
    for action in [Action::Start, Action::Pause] {
        client
            .scrobble(
                &tokens(),
                &Scrobble {
                    media: media.clone(),
                    action,
                    progress: 40.0,
                },
            )
            .await
            .unwrap();
    }
    let requests = task.await.unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests[0].starts_with("GET /shows/tt123/seasons/2/episodes/4?"));
    assert!(requests[1].contains("\"episode\":{\"ids\":{\"trakt\":777}}"));
    assert!(!requests[1].contains("\"show\""));
    assert!(requests[2].starts_with("POST /scrobble/pause"));
}
