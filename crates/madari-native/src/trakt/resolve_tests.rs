use super::*;
use async_trait::async_trait;
use madari_core::{Core, Http, Snapshot, Storage};
use madari_model::{Error, ErrorCode, Result};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
struct Store(Snapshot);
#[async_trait]
impl Storage for Store {
    async fn load(&self) -> Result<Snapshot> {
        Ok(self.0.clone())
    }
    async fn compare_and_swap(&self, _: u64, _: Snapshot) -> Result<()> {
        Ok(())
    }
}
struct HttpFixture {
    requests: Mutex<Vec<String>>,
    fallback: bool,
}
#[async_trait]
impl Http for HttpFixture {
    async fn get_json(&self, url: url::Url, _: bool) -> Result<Value> {
        self.requests.lock().unwrap().push(url.to_string());
        if url.host_str() == Some("broken.example") {
            return Err(Error::new(ErrorCode::Network, "fixture offline"));
        }
        if self.fallback && url.host_str() == Some("imdb.example") {
            return Ok(json!({"meta":null}));
        }
        let id = if url.host_str() == Some("tmdb.example") {
            "tmdb:42"
        } else {
            "tt123"
        };
        let content_type = if matches!(url.host_str(), Some("movies.example" | "wrongtype.example"))
        {
            "movie"
        } else {
            "series"
        };
        Ok(
            json!({"meta":{"id":id,"type":content_type,"name":"Addon title","description":"Full addon details","videos":[{"id":"actual-video-id","season":1,"episode":1,"title":"Episode one"}]}}),
        )
    }
}
fn addon(id: &str, resource: &str, prefix: &str) -> madari_core::Installation {
    serde_json::from_value(json!({"installation_id":id,"manifest_url":format!("https://{id}.example/manifest.json"),"enabled":true,"allow_local":false,
        "manifest":{"id":id,"name":id,"version":"1","resources":[resource],"types":["series"],"idPrefixes":[prefix],"catalogs":[]}})).unwrap()
}
fn title() -> Title {
    Title {
        trakt_id: 7,
        imdb: Some("tt123".into()),
        tmdb: Some(42),
        name: "Trakt preview".into(),
        content_type: "series".into(),
        year: None,
        season: None,
        episode: None,
        watched_at: None,
        details: Default::default(),
    }
}
#[tokio::test]
async fn external_title_uses_real_imdb_metadata_provider_after_failure() {
    let http = Arc::new(HttpFixture {
        requests: Mutex::new(Vec::new()),
        fallback: false,
    });
    let core = Core::new(
        http.clone(),
        Arc::new(Store(Snapshot {
            addons: vec![
                addon("streams", "stream", "tt"),
                addon("broken", "meta", "tt"),
                addon("imdb", "meta", "tt"),
            ],
            ..Default::default()
        })),
    );
    let (key, metadata) = title().resolve(&core).await.unwrap();
    assert_eq!(key.installation_id, "imdb");
    assert_eq!(key.item_id, "tt123");
    assert_eq!(metadata.meta.videos[0].id, "actual-video-id");
    assert_eq!(metadata.meta.name, "Addon title");
    let requests = http.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(
        requests
            .iter()
            .all(|url| url.ends_with("/meta/series/tt123.json"))
    );
    assert!(!requests.iter().any(|url| url.contains("streams.example")));
}
#[tokio::test]
async fn preview_does_not_hide_imdb_miss_and_tmdb_identity_is_tried_next() {
    let http = Arc::new(HttpFixture {
        requests: Mutex::new(Vec::new()),
        fallback: true,
    });
    let core = Core::new(
        http.clone(),
        Arc::new(Store(Snapshot {
            addons: vec![addon("imdb", "meta", "tt"), addon("tmdb", "meta", "tmdb:")],
            ..Default::default()
        })),
    );
    let (key, metadata) = title().resolve(&core).await.unwrap();
    assert_eq!(key.installation_id, "tmdb");
    assert_eq!(key.item_id, "tmdb:42");
    assert_eq!(metadata.meta.id, key.item_id);
    let requests = http.requests.lock().unwrap();
    assert!(requests[0].contains("tt123.json"));
    assert!(requests[1].contains("tmdb"));
}
#[tokio::test]
async fn preview_alone_is_not_success_and_disabled_addons_are_not_used() {
    let http = Arc::new(HttpFixture {
        requests: Mutex::new(Vec::new()),
        fallback: true,
    });
    let mut disabled = addon("tmdb", "meta", "tmdb:");
    disabled.enabled = false;
    let core = Core::new(
        http.clone(),
        Arc::new(Store(Snapshot {
            addons: vec![addon("imdb", "meta", "tt"), disabled],
            ..Default::default()
        })),
    );
    let error = title().resolve(&core).await.err().unwrap();
    assert!(error.message.contains("retry"));
    assert!(error.message.contains("tt123"));
    assert_eq!(http.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn the_same_id_never_crosses_movie_and_series_metadata() {
    let http = Arc::new(HttpFixture {
        requests: Mutex::new(Vec::new()),
        fallback: false,
    });
    let mut movies = addon("movies", "meta", "tt");
    movies.manifest.types = vec!["movie".into()];
    let core = Core::new(
        http.clone(),
        Arc::new(Store(Snapshot {
            addons: vec![
                movies,
                addon("wrongtype", "meta", "tt"),
                addon("imdb", "meta", "tt"),
            ],
            ..Default::default()
        })),
    );
    let (series_key, series) = title().resolve(&core).await.unwrap();
    assert_eq!(series_key.content_type, "series");
    assert_eq!(series_key.installation_id, "imdb");
    assert_eq!(series.meta.content_type, "series");
    {
        let requests = http.requests.lock().unwrap();
        assert!(
            requests
                .iter()
                .all(|r| r.ends_with("/meta/series/tt123.json"))
        );
        assert!(!requests.iter().any(|r| r.contains("movies.example")));
    }
    http.requests.lock().unwrap().clear();
    let mut movie = title();
    movie.content_type = "movie".into();
    let (movie_key, metadata) = movie.resolve(&core).await.unwrap();
    assert_eq!(movie_key.installation_id, "movies");
    assert_eq!(metadata.meta.content_type, "movie");
    assert_eq!(
        *http.requests.lock().unwrap(),
        vec!["https://movies.example/meta/movie/tt123.json".to_owned()]
    );
}
#[tokio::test]
async fn a_correct_id_with_the_wrong_response_type_is_not_a_match() {
    let core = Core::new(
        Arc::new(HttpFixture {
            requests: Mutex::new(Vec::new()),
            fallback: false,
        }),
        Arc::new(Store(Snapshot {
            addons: vec![addon("wrongtype", "meta", "tt")],
            ..Default::default()
        })),
    );
    assert!(title().resolve(&core).await.is_err());
}
