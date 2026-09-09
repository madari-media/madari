use super::*;

/// Private, profile-scoped metadata. Never included in public snapshots.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
pub struct CachedMetadata {
    pub key: ItemKey,
    pub resolved: ResolvedMetadata,
    pub fetched_at: u64,
    // Invalidate when addon configuration or local-network permissions change.
    pub sources: Vec<(String, Url, bool)>,
}
const FRESH_SECONDS: u64 = 6 * 60 * 60;
fn merge_cached(primary: &mut ResolvedMetadata, fallback: &ResolvedMetadata) {
    if primary.meta.name.is_empty() {
        primary.meta.name = fallback.meta.name.clone();
    }
    for (field, value) in &fallback.meta.extra {
        if !primary.meta.extra.contains_key(field) {
            primary.meta.extra.insert(field.clone(), value.clone());
            if let Some(provider) = fallback.field_providers.get(field) {
                primary
                    .field_providers
                    .insert(field.clone(), provider.clone());
            }
        }
    }
    for video in &fallback.meta.videos {
        if !primary.meta.videos.iter().any(|v| v.id == video.id) {
            primary.meta.videos.push(video.clone());
        }
    }
}

impl Core {
    /// Batch only cache misses/stale titles through declared lastVideosIds catalogs.
    /// Keep stale metadata on network failure; never replace it with an error.
    pub async fn continue_metadata(
        &self,
        keys: &[ItemKey],
    ) -> Result<Vec<(ItemKey, ResolvedMetadata)>> {
        let snapshot = self.storage.load().await?;
        let cache = self.storage.cached_metadata().await.unwrap_or_default();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let sources: Vec<_> = snapshot
            .addons
            .iter()
            .filter(|a| a.enabled)
            .map(|a| {
                (
                    a.installation_id.clone(),
                    a.manifest_url.clone(),
                    a.allow_local,
                )
            })
            .collect();
        let mut resolved = Vec::new();
        let mut missing = Vec::new();
        for key in keys {
            let cached = cache.iter().find(|c| c.key == *key && c.sources == sources);
            if let Some(cached) =
                cached.filter(|c| now.saturating_sub(c.fetched_at) < FRESH_SECONDS)
            {
                resolved.push((key.clone(), cached.resolved.clone()));
            } else {
                missing.push(key.clone());
            }
        }
        let mut fetched: Vec<(ItemKey, ResolvedMetadata)> = Vec::new();
        let notification_keys: Vec<_> = missing
            .iter()
            .filter(|key| key.content_type != "movie" && key.content_type != "other")
            .cloned()
            .collect();
        for (addon, request) in
            calendar::id_catalog_requests(&snapshot, &notification_keys, "lastVideosIds")
        {
            if let Ok(ResourceData::Catalog(metas)) = self.query_addon(addon, &request).await {
                for meta in metas {
                    for key in missing
                        .iter()
                        .filter(|k| k.item_id == meta.id && k.content_type == meta.content_type)
                    {
                        if let Some((_, existing)) = fetched.iter_mut().find(|(k, _)| k == key) {
                            metadata::fill_missing(
                                &mut existing.meta,
                                meta.clone(),
                                &addon.installation_id,
                                &mut existing.field_providers,
                            );
                        } else {
                            let field_providers = meta
                                .extra
                                .keys()
                                .map(|field| (field.clone(), addon.installation_id.clone()))
                                .collect();
                            fetched.push((
                                key.clone(),
                                ResolvedMetadata {
                                    meta: meta.clone(),
                                    field_providers,
                                },
                            ));
                        }
                    }
                }
            }
        }
        for key in &missing {
            if let Some((_, primary)) = fetched.iter_mut().find(|(k, _)| k == key)
                && let Some(cached) = cache.iter().find(|c| c.key == *key && c.sources == sources)
            {
                merge_cached(primary, &cached.resolved);
            }
            if !fetched.iter().any(|(k, _)| k == key)
                && let Some(stale) = cache.iter().find(|c| c.key == *key && c.sources == sources)
            {
                resolved.push((key.clone(), stale.resolved.clone()));
            }
        }
        if !fetched.is_empty() {
            let entries: Vec<_> = fetched
                .iter()
                .map(|(key, resolved)| CachedMetadata {
                    key: key.clone(),
                    resolved: resolved.clone(),
                    fetched_at: now,
                    sources: sources.clone(),
                })
                .collect();
            // Cache writes are separate from frequently updated viewing progress.
            let _ = self.storage.cache_metadata(entries).await;
        }
        resolved.extend(fetched);
        for key in keys {
            if resolved.iter().any(|(found, _)| found == key) {
                continue;
            }
            let preview = snapshot
                .progress
                .iter()
                .rev()
                .filter(|p| p.key == *key)
                .find_map(|p| p.metadata.as_ref())
                .or_else(|| {
                    snapshot
                        .library
                        .iter()
                        .find(|item| item.key == *key)
                        .and_then(|item| item.metadata.as_ref())
                });
            if let Some(meta) = preview
                .filter(|meta| meta.id == key.item_id && meta.content_type == key.content_type)
            {
                resolved.push((
                    key.clone(),
                    ResolvedMetadata {
                        meta: meta.clone(),
                        field_providers: meta
                            .extra
                            .keys()
                            .map(|field| (field.clone(), key.installation_id.clone()))
                            .collect(),
                    },
                ));
            }
        }
        Ok(resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    #[derive(Default)]
    struct Store(Mutex<Snapshot>, Mutex<Vec<CachedMetadata>>);
    #[async_trait]
    impl Storage for Store {
        async fn cached_metadata(&self) -> Result<Vec<CachedMetadata>> {
            Ok(self.1.lock().unwrap().clone())
        }
        async fn cache_metadata(&self, entries: Vec<CachedMetadata>) -> Result<()> {
            let mut cache = self.1.lock().unwrap();
            cache.retain(|c| !entries.iter().any(|e| e.key == c.key));
            let entries: Vec<CachedMetadata> =
                serde_json::from_slice(&serde_json::to_vec(&entries).unwrap()).unwrap();
            cache.extend(entries);
            Ok(())
        }
        async fn load(&self) -> Result<Snapshot> {
            Ok(self.0.lock().unwrap().clone())
        }
        async fn compare_and_swap(&self, revision: u64, value: Snapshot) -> Result<()> {
            let mut stored = self.0.lock().unwrap();
            if stored.revision != revision {
                return Err(Error::new(ErrorCode::Conflict, "revision"));
            }
            // Exercise the same serialization boundary as SQLite storage.
            *stored = serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
            Ok(())
        }
    }
    #[derive(Default)]
    struct HttpFixture {
        requests: Mutex<Vec<String>>,
        offline: AtomicBool,
    }
    #[async_trait]
    impl Http for HttpFixture {
        async fn get_json(&self, url: Url, _: bool) -> Result<serde_json::Value> {
            self.requests.lock().unwrap().push(url.to_string());
            if self.offline.load(Ordering::SeqCst) {
                return Err(Error::new(ErrorCode::Network, "offline"));
            }
            if url.path() == "/meta/movie/tt1.json" {
                return Ok(
                    serde_json::json!({"meta":{"id":"tt1","type":"movie","name":"Movie tt1","poster":"https://images.example/movie.jpg"}}),
                );
            }
            let metas: Vec<_> = ["tt1","tt2","tt3"].iter().filter(|id| url.as_str().contains(**id)).map(|id| {
                serde_json::json!({"id":id,"type":"series","name":format!("Show {id}"),"poster":"https://images.example/poster.jpg",
                    "videos":[{"id":format!("{id}:1:1"),"name":"Episode name","season":1,"episode":1}]})
            }).collect();
            Ok(serde_json::json!({"metasDetailed":metas}))
        }
    }
    fn key(id: &str) -> ItemKey {
        ItemKey {
            installation_id: "addon".into(),
            content_type: "series".into(),
            item_id: id.into(),
        }
    }
    #[tokio::test]
    async fn durable_movie_cards_survive_an_empty_cache_without_network_requests() {
        let store = Arc::new(Store::default());
        let http = Arc::new(HttpFixture::default());
        let core = Core::new(http.clone(), store.clone());
        let movie = ItemKey {
            content_type: "movie".into(),
            ..key("tt1")
        };
        let meta: Meta = serde_json::from_value(serde_json::json!({"id":"tt1", "type":"movie", "name":"Saved movie", "poster":"https://images.example/movie.jpg"})).unwrap();
        core.record_progress(Progress {
            key: movie.clone(),
            metadata: Some(meta),
            video_id: "tt1".into(),
            position_ms: 1000,
            duration_ms: Some(10000),
            completed: false,
            binge_group: None,
            source_provider: None,
        })
        .await
        .unwrap();
        let mut progress = core.snapshot().await.unwrap().progress[0].clone();
        progress.metadata = None;
        progress.position_ms = 2000;
        core.record_progress(progress).await.unwrap();
        let restarted = Core::new(http.clone(), store);
        let titles = restarted.continue_metadata(&[movie]).await.unwrap();
        assert_eq!(titles[0].1.meta.name, "Saved movie");
        assert_eq!(
            titles[0].1.meta.extra["poster"],
            "https://images.example/movie.jpg"
        );
        assert!(http.requests.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn calendar_uses_only_declared_catalogs_and_saved_titles() {
        let store = Arc::new(Store::default());
        store.0.lock().unwrap().addons.push(Installation {
            installation_id: "addon".into(), manifest_url: "https://fixture.example/manifest.json".parse().unwrap(),
            manifest: serde_json::from_value(serde_json::json!({"id":"fixture","name":"Fixture","version":"1","resources":["catalog"],"types":["series"],"idPrefixes":["tt"],"catalogs":[{"id":"schedule","type":"series","extra":[{"name":"calendarVideosIds","isRequired":true,"optionsLimit":1}]}]})).unwrap(),
            enabled: true, allow_local: false,
        });
        let http = Arc::new(HttpFixture::default());
        let core = Core::new(http.clone(), store);
        core.save_item(LibraryEntry {
            key: key("tt1"),
            title: "Older".into(),
            metadata: None,
        })
        .await
        .unwrap();
        core.save_item(LibraryEntry {
            key: key("tt2"),
            title: "Newest".into(),
            metadata: None,
        })
        .await
        .unwrap();
        let calendar = core.calendar().await.unwrap();
        assert!(calendar.supported);
        assert_eq!(calendar.titles.len(), 1);
        assert_eq!(calendar.titles[0].0.item_id, "tt2");
        let requests = http.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].ends_with("/catalog/series/schedule/calendarVideosIds=tt2.json"));
    }

    #[tokio::test]
    async fn continue_only_uses_declared_last_ids_catalogs_without_meta_fallback() {
        let store = Arc::new(Store::default());
        store.0.lock().unwrap().addons.push(Installation {
            installation_id: "addon".into(), manifest_url: "https://fixture.example/manifest.json".parse().unwrap(),
            manifest: serde_json::from_value(serde_json::json!({"id":"fixture","name":"Fixture","version":"1","resources":["catalog", "meta"],"types":["movie", "series"],"catalogs":[{"id":"last-videos","type":"series","extra":[{"name":"lastVideosIds","isRequired":true}]}]})).unwrap(),
            enabled: true, allow_local: false,
        });
        let http = Arc::new(HttpFixture::default());
        let core = Core::new(http.clone(), store);
        let movie = ItemKey {
            content_type: "movie".into(),
            ..key("tt1")
        };
        let result = core.continue_metadata(&[movie, key("tt1")]).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0.content_type, "series");
        let requests = http.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].contains("/catalog/series/last-videos/lastVideosIds=tt1.json"));
        assert!(!requests[0].contains("/meta/"));
    }

    #[tokio::test]
    async fn movie_details_survive_offline_and_do_not_collide_with_series_ids() {
        let store = Arc::new(Store::default());
        store.0.lock().unwrap().addons.push(Installation {
            installation_id: "addon".into(), manifest_url: "https://fixture.example/manifest.json".parse().unwrap(),
            manifest: serde_json::from_value(serde_json::json!({"id":"fixture","name":"Fixture","version":"1","resources":["catalog", "meta"],"types":["movie", "series"],"catalogs":[{"id":"last-videos","type":"series","extra":[{"name":"lastVideosIds","isRequired":true}]}]})).unwrap(),
            enabled: true, allow_local: false,
        });
        let http = Arc::new(HttpFixture::default());
        let core = Core::new(http.clone(), store.clone());
        let movie = ItemKey {
            content_type: "movie".into(),
            ..key("tt1")
        };
        assert_eq!(
            core.resolve_metadata(&movie, None).await.unwrap().meta.name,
            "Movie tt1"
        );
        core.continue_metadata(&[key("tt1")]).await.unwrap();
        http.offline.store(true, Ordering::SeqCst);
        let restarted = Core::new(http.clone(), store);
        let cached = restarted
            .continue_metadata(&[movie.clone(), key("tt1")])
            .await
            .unwrap();
        assert_eq!(cached.len(), 2);
        assert_eq!(
            cached
                .iter()
                .find(|(k, _)| *k == movie)
                .unwrap()
                .1
                .meta
                .name,
            "Movie tt1"
        );
        assert_eq!(
            cached
                .iter()
                .find(|(k, _)| k.content_type == "series")
                .unwrap()
                .1
                .meta
                .name,
            "Show tt1"
        );
        assert_eq!(
            restarted
                .resolve_metadata(&movie, None)
                .await
                .unwrap()
                .meta
                .name,
            "Movie tt1"
        );
    }
    #[tokio::test]
    async fn batches_only_missing_ids_persists_across_restart_and_keeps_stale_offline() {
        let store = Arc::new(Store::default());
        store.0.lock().unwrap().addons.push(Installation {
            installation_id:"addon".into(),manifest_url:"https://fixture.example/manifest.json".parse().unwrap(),
            manifest:serde_json::from_value(serde_json::json!({"id":"fixture","name":"Fixture","version":"1","resources":["catalog"],"types":["series"],
                "catalogs":[{"id":"last-videos","type":"series","extra":[{"name":"lastVideosIds","isRequired":true,"optionsLimit":100}]}]})).unwrap(),
            enabled:true,allow_local:false,
        });
        let http = Arc::new(HttpFixture::default());
        let core = Core::new(http.clone(), store.clone());
        let first = core
            .continue_metadata(&[key("tt1"), key("tt2")])
            .await
            .unwrap();
        assert_eq!(first.len(), 2);
        assert_eq!(
            first[0].1.meta.videos[0].title.as_deref(),
            Some("Episode name")
        );
        assert_eq!(http.requests.lock().unwrap().len(), 1);
        let restarted = Core::new(http.clone(), store.clone());
        assert_eq!(
            restarted
                .continue_metadata(&[key("tt1"), key("tt2")])
                .await
                .unwrap()
                .len(),
            2
        );
        assert_eq!(http.requests.lock().unwrap().len(), 1);
        restarted
            .continue_metadata(&[key("tt1"), key("tt3")])
            .await
            .unwrap();
        let requests = http.requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 2);
        assert!(requests[1].contains("lastVideosIds=tt3"));
        assert!(!requests[1].contains("tt1"));
        for entry in store.1.lock().unwrap().iter_mut() {
            entry.fetched_at = 0;
        }
        http.offline.store(true, Ordering::SeqCst);
        assert_eq!(
            restarted
                .continue_metadata(&[key("tt1")])
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(store.1.lock().unwrap()[0].fetched_at, 0);
        store.0.lock().unwrap().addons[0].enabled = false;
        assert!(
            restarted
                .continue_metadata(&[key("tt1")])
                .await
                .unwrap()
                .is_empty()
        );
    }
}
