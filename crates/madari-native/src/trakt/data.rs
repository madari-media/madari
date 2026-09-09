use super::*;
use madari_core::PublicSnapshot;
use madari_model::{ItemKey, Meta, Progress, Resource, ResourceRequest, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Data {
    pub username: String,
    pub synced_at: u64,
    pub watchlist: Vec<Title>,
    /// Most recently watched first, deduplicated by movie or episode.
    pub history: Vec<Title>,
    pub lists: Vec<List>,
    pub skipped: usize,
    #[serde(default)]
    pub artwork_version: u32,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct List {
    pub id: u64,
    pub name: String,
    pub items: Vec<Title>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Title {
    pub trakt_id: u64,
    pub imdb: Option<String>,
    pub tmdb: Option<u64>,
    pub name: String,
    pub content_type: String,
    pub year: Option<u64>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub watched_at: Option<String>,
    #[serde(default)]
    pub details: madari_model::Fields,
}
impl Title {
    pub fn meta(&self) -> Meta {
        let id = self
            .imdb
            .clone()
            .or_else(|| self.tmdb.map(|id| format!("tmdb:{id}")))
            .unwrap_or_else(|| format!("trakt:{}", self.trakt_id));
        let mut extra = self.details.clone();
        if let Some(year) = self.year {
            extra.insert("year".into(), Value::from(year));
        }
        Meta {
            id,
            content_type: self.content_type.clone(),
            name: self.name.clone(),
            videos: Vec::new(),
            extra,
        }
    }
    pub fn identities(&self) -> Vec<String> {
        let mut ids = Vec::new();
        if let Some(imdb) = self.imdb.as_deref().filter(|id| {
            id.starts_with("tt") && id.len() > 2 && id[2..].bytes().all(|b| b.is_ascii_digit())
        }) {
            ids.push(imdb.to_owned());
        }
        if let Some(tmdb) = self.tmdb {
            ids.push(format!("tmdb:{tmdb}"));
        }
        ids.push(format!("trakt:{}", self.trakt_id));
        ids
    }
    pub fn candidates(&self, snapshot: &PublicSnapshot) -> Vec<ItemKey> {
        self.identities()
            .into_iter()
            .filter_map(|id| {
                let request = ResourceRequest {
                    resource: Resource::Meta,
                    content_type: self.content_type.clone(),
                    id: id.clone(),
                    extra: Default::default(),
                };
                let supported: Vec<_> = snapshot
                    .addons
                    .iter()
                    .filter(|a| a.enabled && a.manifest.supports(&request))
                    .collect();
                let existing = snapshot
                    .library
                    .iter()
                    .map(|e| &e.key)
                    .chain(snapshot.progress.iter().map(|p| &p.key))
                    .find(|key| {
                        key.content_type == self.content_type
                            && key.item_id == id
                            && supported
                                .iter()
                                .any(|a| a.installation_id == key.installation_id)
                    });
                let provider = existing
                    .map(|key| key.installation_id.clone())
                    .or_else(|| supported.first().map(|a| a.installation_id.clone()))?;
                Some(ItemKey {
                    installation_id: provider,
                    content_type: self.content_type.clone(),
                    item_id: id,
                })
            })
            .collect()
    }
    pub fn key(&self, snapshot: &PublicSnapshot) -> Option<ItemKey> {
        self.candidates(snapshot).into_iter().next()
    }
    pub fn preview_for(&self, key: &ItemKey) -> Meta {
        Meta {
            id: key.item_id.clone(),
            ..self.meta()
        }
    }
    pub async fn resolve(
        &self,
        core: &madari_core::Core,
    ) -> Result<(ItemKey, madari_core::ResolvedMetadata)> {
        let snapshot = core.snapshot().await?;
        let candidates = self.candidates(&snapshot);
        if candidates.is_empty() {
            return Err(madari_model::Error::new(
                madari_model::ErrorCode::NotFound,
                "No enabled metadata addon supports this Trakt title’s IDs. Enable a movie/series metadata addon in Settings and try again.",
            ));
        }
        let mut attempted = Vec::new();
        for key in candidates {
            attempted.push(key.item_id.clone());
            if let Ok(resolved) = core
                .resolve_addon_metadata(&key, Some(self.preview_for(&key)))
                .await
            {
                return Ok(resolved);
            }
        }
        Err(madari_model::Error::new(
            madari_model::ErrorCode::NotFound,
            format!(
                "The enabled addons could not load this title ({}). Check your connection or metadata addons, then retry.",
                attempted.join(", ")
            ),
        ))
    }
}
impl Data {
    /// Use actual addon episode IDs; never guess video IDs from season numbers.
    /// Existing local positions (including rewatches) always win.
    pub fn completed_progress(
        &self,
        key: &ItemKey,
        meta: &Meta,
        local: &[Progress],
    ) -> Vec<Progress> {
        let mut result = Vec::new();
        for title in self.history.iter().rev() {
            if !title.identities().contains(&key.item_id) || title.content_type != key.content_type
            {
                continue;
            }
            let video_id = if key.content_type == "movie" {
                meta.videos
                    .first()
                    .map(|v| v.id.clone())
                    .unwrap_or_else(|| meta.id.clone())
            } else {
                let Some(video) = meta.videos.iter().find(|v| {
                    title.episode.is_some()
                        && v.season == title.season
                        && v.episode == title.episode
                }) else {
                    continue;
                };
                video.id.clone()
            };
            if local
                .iter()
                .chain(result.iter())
                .any(|p: &Progress| p.key == *key && p.video_id == video_id)
            {
                continue;
            }
            result.push(Progress {
                metadata: Some(meta.preview()),
                binge_group: None,
                source_provider: None,
                key: key.clone(),
                video_id,
                position_ms: 0,
                duration_ms: None,
                completed: true,
            });
        }
        result
    }
}
fn details(media: &Value) -> madari_model::Fields {
    let mut fields = madari_model::Fields::new();
    for (source, target) in [
        ("poster", "poster"),
        ("fanart", "background"),
        ("logo", "logo"),
    ] {
        if let Some(url) = media["images"][source]
            .as_array()
            .and_then(|images| images.iter().filter_map(Value::as_str).find_map(image_url))
        {
            fields.insert(target.into(), url.into());
        }
    }
    for (source, target) in [
        ("overview", "description"),
        ("runtime", "runtime"),
        ("genres", "genres"),
        ("certification", "certification"),
    ] {
        if let Some(value) = media.get(source).filter(|v| !v.is_null()) {
            fields.insert(target.into(), value.clone());
        }
    }
    fields
}
fn image_url(value: &str) -> Option<String> {
    let raw = if value.contains("://") {
        value.to_owned()
    } else {
        format!("https://{value}")
    };
    let url = url::Url::parse(&raw).ok()?;
    (url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url
            .host_str()
            .is_some_and(|host| host == "trakt.tv" || host.ends_with(".trakt.tv")))
    .then(|| url.to_string())
}
fn title(value: &Value) -> Option<Title> {
    // Trakt episodes/seasons belong to their parent show's addon metadata.
    // Never interpret an episode's own ID as a movie or parent-show identity.
    let (media, content_type) = match value["type"].as_str() {
        Some("movie") if value["movie"].is_object() => (&value["movie"], "movie"),
        Some("show" | "season" | "episode") if value["show"].is_object() => {
            (&value["show"], "series")
        }
        None if value["movie"].is_object() => (&value["movie"], "movie"),
        None if value["show"].is_object() => (&value["show"], "series"),
        _ => return None,
    };
    Some(Title {
        trakt_id: media["ids"]["trakt"].as_u64()?,
        imdb: media["ids"]["imdb"]
            .as_str()
            .filter(|id| {
                id.starts_with("tt") && id[2..].bytes().all(|b| b.is_ascii_digit()) && id.len() > 2
            })
            .map(str::to_owned),
        tmdb: media["ids"]["tmdb"].as_u64(),
        name: media["title"].as_str()?.to_owned(),
        content_type: content_type.into(),
        year: media["year"].as_u64(),
        season: value["episode"]["season"]
            .as_u64()
            .or_else(|| value["season"]["number"].as_u64())
            .and_then(|n| n.try_into().ok()),
        episode: value["episode"]["number"]
            .as_u64()
            .and_then(|n| n.try_into().ok()),
        watched_at: value["watched_at"].as_str().map(str::to_owned),
        details: details(media),
    })
}
fn titles(values: Vec<Value>, skipped: &mut usize) -> Vec<Title> {
    let mut seen = HashSet::new();
    values
        .iter()
        .filter_map(|v| {
            let t = title(v);
            if t.is_none() {
                *skipped += 1;
            }
            t
        })
        .filter(|t| seen.insert((t.content_type.clone(), t.trakt_id, t.season, t.episode)))
        .collect()
}
impl Client {
    pub(crate) async fn pull(&self, tokens: &Tokens) -> Result<Data> {
        let username = self.account_name(tokens).await?;
        let mut skipped = 0;
        let watchlist = titles(self.pages("/sync/watchlist", tokens).await?, &mut skipped);
        let history = titles(self.pages("/sync/history", tokens).await?, &mut skipped);
        let mut lists = Vec::new();
        let mut seen = HashSet::new();
        for list in self.pages("/users/me/lists", tokens).await? {
            let id = list["ids"]["trakt"]
                .as_u64()
                .ok_or_else(|| invalid("Trakt returned a list without an ID."))?;
            if !seen.insert(id) {
                continue;
            }
            let name = list["name"].as_str().unwrap_or("Untitled list").to_owned();
            let items = titles(
                self.pages(
                    &format!("/users/me/lists/{id}/items/movie,show,season,episode"),
                    tokens,
                )
                .await?,
                &mut skipped,
            );
            lists.push(List { id, name, items });
        }
        Ok(Data {
            username,
            synced_at: now(),
            watchlist,
            history,
            lists,
            skipped,
            artwork_version: 1,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn trakt_types_map_to_addon_types_and_episode_ids_stay_separate() {
        let movie = json!({"title":"Movie","ids":{"trakt":1,"imdb":"tt100"}});
        let show = json!({"title":"Show","ids":{"trakt":2,"imdb":"tt200"}});
        assert_eq!(
            title(&json!({"type":"movie","movie":movie}))
                .unwrap()
                .content_type,
            "movie"
        );
        for kind in ["show", "season", "episode"] {
            let item=title(&json!({"type":kind,"show":show,"episode":{"season":1,"number":2,"ids":{"imdb":"tt999"}}})).unwrap();
            assert_eq!(item.content_type, "series");
            assert_eq!(item.imdb.as_deref(), Some("tt200"));
        }
        assert!(title(&json!({"type":"movie","show":show})).is_none());
    }
    #[test]
    fn full_metadata_retains_safe_trakt_artwork_and_legacy_cache_loads() {
        let item=title(&json!({"movie":{"title":"Movie","ids":{"trakt":1,"imdb":"tt123"},"overview":"Description","images":{"poster":["walter-r2.trakt.tv/images/poster.webp"],"fanart":["https://walter-r2.trakt.tv/images/back.webp"],"logo":["http://evil.example/image"]}}})).unwrap();
        let meta = item.meta();
        assert_eq!(
            meta.extra["poster"],
            "https://walter-r2.trakt.tv/images/poster.webp"
        );
        assert_eq!(meta.extra["description"], "Description");
        assert!(!meta.extra.contains_key("logo"));
        let mut value = serde_json::to_value(&item).unwrap();
        value.as_object_mut().unwrap().remove("details");
        let legacy: Title = serde_json::from_value(value).unwrap();
        assert!(legacy.details.is_empty());
    }
    #[test]
    fn history_maps_real_episode_ids_and_preserves_rewatch_positions() {
        let show = json!({"title":"Show","ids":{"trakt":42,"imdb":"tt42"}});
        let mut skipped = 0;
        let history = titles(
            vec![
                json!({"show":show,"episode":{"season":1,"number":2}}),
                json!({"show":show,"episode":{"season":1,"number":1}}),
                json!({"show":show,"episode":{"season":1,"number":1}}),
                json!({"person":{}}),
            ],
            &mut skipped,
        );
        assert_eq!(history.len(), 2);
        assert_eq!(skipped, 1);
        let data = Data {
            history,
            ..Default::default()
        };
        let key = ItemKey {
            installation_id: "addon".into(),
            content_type: "series".into(),
            item_id: "tt42".into(),
        };
        let meta:Meta=serde_json::from_value(json!({"id":"tt42","type":"series","name":"Show","videos":[{"id":"custom-A","season":1,"episode":1},{"id":"custom-B","season":1,"episode":2}]})).unwrap();
        let mut local = data.completed_progress(&key, &meta, &[]);
        assert_eq!(
            local
                .iter()
                .map(|p| p.video_id.as_str())
                .collect::<Vec<_>>(),
            vec!["custom-A", "custom-B"]
        );
        local[0].completed = false;
        local[0].position_ms = 500;
        assert!(data.completed_progress(&key, &meta, &local).is_empty());
        assert!(
            data.completed_progress(
                &key,
                &Meta {
                    videos: Vec::new(),
                    ..meta
                },
                &[]
            )
            .is_empty()
        );
    }
}
