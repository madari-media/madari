//! Shared application logic. Adapters own I/O and platform runtimes.
mod calendar;
mod resume_cache;
pub use calendar::CalendarData;
pub use resume_cache::CachedMetadata;
mod episodes;
mod tracks;
pub use episodes::{continue_episode, episode_available, next_episode, previous_episode};
pub use tracks::preferred_track;
mod continuation;
pub use continuation::{binge_group, same_binge_group};
mod metadata;
pub use metadata::ResolvedMetadata;
mod defaults;
pub use defaults::{DEFAULT_ADDONS, DefaultAddon, DefaultAddonFailure, DefaultAddonOutcome};
mod playback;
use async_trait::async_trait;
use futures::{Stream as AsyncStream, StreamExt, channel::mpsc, stream};
use madari_addon::{Manifest, manifest_url, parse_response, resource_url};
use madari_model::*;
pub use playback::{PlaybackMedia, resume_position};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use url::Url;
use uuid::Uuid;

/// Native adapters cross executor threads; browser adapters may own JS handles.
#[cfg(not(target_arch = "wasm32"))]
pub trait PlatformBound: Send + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync> PlatformBound for T {}
#[cfg(target_arch = "wasm32")]
pub trait PlatformBound {}
#[cfg(target_arch = "wasm32")]
impl<T> PlatformBound for T {}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait Http: PlatformBound {
    async fn get_json(&self, url: Url, allow_local: bool) -> Result<serde_json::Value>;
}

/// Atomic revision-checked writes prevent concurrent clients from losing state.
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait Storage: PlatformBound {
    async fn load(&self) -> Result<Snapshot>;
    async fn compare_and_swap(&self, expected_revision: u64, next: Snapshot) -> Result<()>;
    async fn cached_metadata(&self) -> Result<Vec<CachedMetadata>> {
        Ok(Vec::new())
    }
    async fn cache_metadata(&self, _entries: Vec<CachedMetadata>) -> Result<()> {
        Ok(())
    }
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
pub struct Installation {
    pub installation_id: String,
    pub manifest_url: Url,
    pub manifest: Manifest,
    pub enabled: bool,
    pub allow_local: bool,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub playback_preferences: PlaybackPreferences,
    pub revision: u64,
    /// Whether the curated default addons have been offered to this profile.
    ///
    /// Recorded rather than inferred from `addons` being empty: a first run where one
    /// default could not be fetched would otherwise leave the profile permanently
    /// missing it, and inferring from an empty list would re-add defaults a user
    /// deliberately removed.
    #[serde(default)]
    pub defaults_seeded: bool,
    pub addons: Vec<Installation>,
    pub library: Vec<LibraryEntry>,
    pub progress: Vec<Progress>,
    #[serde(default)]
    pub hidden_continue: Vec<ItemKey>,
}

/// Public snapshots omit configured transport URLs. Treat manifest data as untrusted.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize)]
pub struct PublicSnapshot {
    pub playback_preferences: PlaybackPreferences,
    pub api_version: u32,
    pub revision: u64,
    pub addons: Vec<AddonSummary>,
    pub library: Vec<LibraryEntry>,
    pub progress: Vec<Progress>,
    pub hidden_continue: Vec<ItemKey>,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize)]
pub struct AddonSummary {
    pub installation_id: String,
    pub allow_local: bool,
    pub enabled: bool,
    pub manifest: Manifest,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize)]
pub struct Change {
    pub revision: u64,
}

pub struct Core {
    http: Arc<dyn Http>,
    storage: Arc<dyn Storage>,
    subscribers: Mutex<Vec<mpsc::Sender<Change>>>,
}

impl Core {
    pub fn new(http: Arc<dyn Http>, storage: Arc<dyn Storage>) -> Self {
        Self {
            http,
            storage,
            subscribers: Mutex::new(Vec::new()),
        }
    }

    pub async fn snapshot(&self) -> Result<PublicSnapshot> {
        let snapshot = self.storage.load().await?;
        Ok(PublicSnapshot {
            playback_preferences: snapshot.playback_preferences,
            api_version: API_VERSION,
            revision: snapshot.revision,
            addons: snapshot
                .addons
                .into_iter()
                .map(|a| AddonSummary {
                    installation_id: a.installation_id,
                    allow_local: a.allow_local,
                    enabled: a.enabled,
                    manifest: a.manifest,
                })
                .collect(),
            library: snapshot.library,
            progress: snapshot.progress,
            hidden_continue: snapshot.hidden_continue,
        })
    }

    /// Subscribe before reading a snapshot, ignore revisions <= snapshot.revision.
    /// A slow subscriber is disconnected on overflow and must subscribe/snapshot again.
    /// Dropping the receiver unsubscribes; revisions are persisted but events are not.
    pub fn subscribe(&self) -> mpsc::Receiver<Change> {
        let (tx, rx) = mpsc::channel(32);
        let mut subscribers = self.subscribers.lock().expect("subscriber lock poisoned");
        subscribers.retain(|s| !s.is_closed());
        subscribers.push(tx);
        rx
    }

    async fn mutate(&self, change: impl Fn(&mut Snapshot) -> Result<()>) -> Result<u64> {
        for _ in 0..8 {
            let mut snapshot = self.storage.load().await?;
            let previous = snapshot.revision;
            change(&mut snapshot)?;
            snapshot.revision = previous
                .checked_add(1)
                .ok_or_else(|| Error::new(ErrorCode::Storage, "revision exhausted"))?;
            let revision = snapshot.revision;
            match self.storage.compare_and_swap(previous, snapshot).await {
                Ok(()) => {
                    self.subscribers
                        .lock()
                        .expect("subscriber lock poisoned")
                        .retain_mut(|s| s.try_send(Change { revision }).is_ok());
                    return Ok(revision);
                }
                Err(e) if e.code == ErrorCode::Conflict => continue,
                Err(e) => return Err(e),
            }
        }
        Err(Error::new(
            ErrorCode::Conflict,
            "state changed; retry command",
        ))
    }

    pub async fn install(
        &self,
        installation_id: String,
        input: &str,
        allow_local: bool,
    ) -> Result<u64> {
        if installation_id.is_empty() || installation_id.len() > 128 {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "invalid installation ID",
            ));
        }
        let url = manifest_url(input)?;
        let value = self.http.get_json(url.clone(), allow_local).await?;
        let manifest: Manifest = serde_json::from_value(value)
            .map_err(|_| Error::new(ErrorCode::InvalidResponse, "invalid addon manifest"))?;
        manifest.validate()?;
        if manifest
            .behavior_hints
            .get("configurationRequired")
            .and_then(|v| v.as_bool())
            == Some(true)
        {
            return Err(Error::new(
                ErrorCode::ConfigurationRequired,
                "configure the addon externally, then install its configured manifest URL",
            ));
        }
        let addon = Installation {
            installation_id,
            manifest_url: url,
            manifest,
            enabled: true,
            allow_local,
        };
        self.mutate(|state| {
            if state
                .addons
                .iter()
                .any(|a| a.installation_id == addon.installation_id)
            {
                return Err(Error::new(
                    ErrorCode::Conflict,
                    "installation ID already exists",
                ));
            }
            state.addons.push(addon.clone());
            Ok(())
        })
        .await
    }

    /// Installs [`DEFAULT_ADDONS`] on a profile that has not been offered them yet.
    ///
    /// The one-time marker is only set once every default is in place, so a first run
    /// that could not reach one of them retries it on the next open instead of leaving
    /// a profile permanently without it. Once seeded, defaults a user removes stay
    /// removed.
    pub async fn install_default_addons(&self) -> Result<DefaultAddonOutcome> {
        let snapshot = self.storage.load().await?;
        if snapshot.defaults_seeded {
            return Ok(DefaultAddonOutcome {
                skipped: true,
                installed: Vec::new(),
                failed: Vec::new(),
            });
        }
        let present: Vec<String> = snapshot
            .addons
            .iter()
            .map(|a| a.manifest_url.to_string())
            .collect();
        let mut installed = Vec::new();
        let mut failed = Vec::new();
        for entry in DEFAULT_ADDONS {
            // Already in place from an earlier partial run.
            if present.iter().any(|a| a == entry.url) {
                continue;
            }
            let id = Uuid::new_v4().simple().to_string();
            match self.install(id, entry.url, false).await {
                Ok(_) => installed.push(entry.url.to_owned()),
                Err(error) => failed.push(DefaultAddonFailure {
                    url: entry.url.to_owned(),
                    reason: error.message,
                }),
            }
        }
        let seeded = failed.is_empty();
        if seeded {
            self.mutate(|state| {
                state.defaults_seeded = true;
                Ok(())
            })
            .await?;
        }
        Ok(DefaultAddonOutcome {
            skipped: false,
            installed,
            failed,
        })
    }

    /// Manifest URLs this profile already has installed.
    ///
    /// The public snapshot deliberately omits the manifest URL, and clients need it to
    /// tell a curated addon that is already installed from one that is not.
    pub async fn installed_manifest_urls(&self) -> Result<Vec<String>> {
        Ok(self
            .storage
            .load()
            .await?
            .addons
            .iter()
            .map(|a| a.manifest_url.to_string())
            .collect())
    }

    pub async fn set_enabled(&self, id: &str, enabled: bool) -> Result<u64> {
        self.mutate(|s| {
            let addon = s
                .addons
                .iter_mut()
                .find(|a| a.installation_id == id)
                .ok_or_else(|| Error::new(ErrorCode::NotFound, "addon not installed"))?;
            addon.enabled = enabled;
            Ok(())
        })
        .await
    }

    /// Preserve installation identity when changing its configured endpoint.
    pub async fn configure_addon(&self, id: &str, input: &str, allow_local: bool) -> Result<u64> {
        let url = manifest_url(input)?;
        let manifest: Manifest =
            serde_json::from_value(self.http.get_json(url.clone(), allow_local).await?)
                .map_err(|_| Error::new(ErrorCode::InvalidResponse, "invalid addon manifest"))?;
        manifest.validate()?;
        if manifest
            .behavior_hints
            .get("configurationRequired")
            .and_then(|v| v.as_bool())
            == Some(true)
        {
            return Err(Error::new(
                ErrorCode::ConfigurationRequired,
                "install the configured manifest URL",
            ));
        }
        self.mutate(|s| {
            let addon = s
                .addons
                .iter_mut()
                .find(|a| a.installation_id == id)
                .ok_or_else(|| Error::new(ErrorCode::NotFound, "addon not installed"))?;
            addon.manifest_url = url.clone();
            addon.manifest = manifest.clone();
            addon.allow_local = allow_local;
            Ok(())
        })
        .await
    }

    pub async fn remove_addon(&self, id: &str) -> Result<u64> {
        self.mutate(|s| {
            s.addons.retain(|a| a.installation_id != id);
            Ok(())
        })
        .await
    }

    pub async fn reorder(&self, ids: &[String]) -> Result<u64> {
        self.mutate(|s| {
            let mut remaining = s.addons.clone();
            let mut ordered = Vec::new();
            for id in ids {
                let index = remaining
                    .iter()
                    .position(|a| &a.installation_id == id)
                    .ok_or_else(|| {
                        Error::new(
                            ErrorCode::InvalidInput,
                            "order must contain each installation exactly once",
                        )
                    })?;
                ordered.push(remaining.remove(index));
            }
            if !remaining.is_empty() {
                return Err(Error::new(ErrorCode::InvalidInput, "order is incomplete"));
            }
            s.addons = ordered;
            Ok(())
        })
        .await
    }

    pub async fn save_item(&self, entry: LibraryEntry) -> Result<u64> {
        self.mutate(|s| {
            let mut entry = entry.clone();
            if entry.metadata.is_none() {
                entry.metadata = s
                    .library
                    .iter()
                    .find(|e| e.key == entry.key)
                    .and_then(|e| e.metadata.clone());
            }
            entry.metadata = entry
                .metadata
                .filter(|m| m.id == entry.key.item_id && m.content_type == entry.key.content_type)
                .map(|m| m.preview());
            s.library.retain(|e| e.key != entry.key);
            s.library.push(entry);
            Ok(())
        })
        .await
    }

    pub async fn remove_item(&self, key: &ItemKey) -> Result<u64> {
        self.mutate(|s| {
            s.library.retain(|e| &e.key != key);
            Ok(())
        })
        .await
    }

    /// Hiding a title never erases episode positions or watched history.
    pub async fn set_continue_hidden(&self, key: &ItemKey, hidden: bool) -> Result<u64> {
        self.mutate(|s| {
            s.hidden_continue.retain(|k| k != key);
            if hidden {
                s.hidden_continue.push(key.clone());
            }
            Ok(())
        })
        .await
    }

    pub async fn set_playback_preferences(&self, preferences: PlaybackPreferences) -> Result<u64> {
        for languages in [
            &preferences.audio_languages,
            &preferences.subtitle_languages,
        ] {
            if languages.len() > 20
                || languages.iter().any(|l| {
                    l.is_empty()
                        || l.len() > 24
                        || !l.bytes().all(|b| b.is_ascii_alphabetic() || b == b'-')
                })
            {
                return Err(Error::new(
                    ErrorCode::InvalidInput,
                    "Use up to 20 language codes in preference order.",
                ));
            }
        }
        self.mutate(|s| {
            s.playback_preferences = preferences.clone();
            Ok(())
        })
        .await
    }

    pub async fn record_progress(&self, progress: Progress) -> Result<u64> {
        if progress
            .duration_ms
            .is_some_and(|d| progress.position_ms > d)
        {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "position exceeds duration",
            ));
        }
        self.mutate(|s| {
            let mut progress = progress.clone();
            if progress.metadata.is_none() {
                progress.metadata = s
                    .progress
                    .iter()
                    .rev()
                    .find(|p| p.key == progress.key)
                    .and_then(|p| p.metadata.clone());
            }
            progress.metadata = progress
                .metadata
                .filter(|m| {
                    m.id == progress.key.item_id && m.content_type == progress.key.content_type
                })
                .map(|m| m.preview());
            s.progress
                .retain(|p| p.key != progress.key || p.video_id != progress.video_id);
            s.progress.push(progress);
            Ok(())
        })
        .await
    }

    pub async fn playback_plan(
        &self,
        key: &ItemKey,
        video_id: &str,
        source: Stream,
        capabilities: &PlayerCapabilities,
    ) -> Result<PlaybackPlan> {
        let snapshot = self.storage.load().await?;
        let resume = snapshot
            .progress
            .iter()
            .find(|p| &p.key == key && p.video_id == video_id)
            .map_or(0, resume_position);
        Ok(prepare_playback(source, capabilities, resume))
    }

    pub async fn query(&self, id: &str, request: ResourceRequest) -> Result<ResourceData> {
        let addon = self
            .storage
            .load()
            .await?
            .addons
            .into_iter()
            .find(|a| a.installation_id == id && a.enabled)
            .ok_or_else(|| {
                Error::new(ErrorCode::NotFound, "enabled addon not found").for_addon(id)
            })?;
        self.query_addon(&addon, &request)
            .await
            .map_err(|e| e.for_addon(id))
    }

    async fn query_addon(
        &self,
        addon: &Installation,
        request: &ResourceRequest,
    ) -> Result<ResourceData> {
        if !addon.manifest.supports(request) {
            return Err(Error::new(
                ErrorCode::UnsupportedResource,
                "addon does not declare this request or required extras are missing",
            ));
        }
        let url = resource_url(&addon.manifest_url, request)?;
        parse_response(
            request.resource,
            self.http.get_json(url, addon.allow_local).await?,
        )
    }

    /// Completion order is incremental; ordinal allows clients to preserve addon order.
    /// Dropping this stream cancels its in-flight addon futures.
    pub async fn query_incremental(
        &self,
        request: ResourceRequest,
    ) -> Result<impl AsyncStream<Item = (usize, ProviderResult)> + '_> {
        let addons = self.storage.load().await?.addons;
        let filter_request = request.clone();
        Ok(stream::iter(
            addons
                .into_iter()
                .enumerate()
                .filter(move |(_, a)| a.enabled && a.manifest.supports(&filter_request)),
        )
        .map(move |(ordinal, addon)| {
            let request = request.clone();
            async move {
                let result = self
                    .query_addon(&addon, &request)
                    .await
                    .map_err(|e| e.for_addon(&addon.installation_id));
                (
                    ordinal,
                    ProviderResult {
                        installation_id: addon.installation_id,
                        result,
                    },
                )
            }
        })
        .buffer_unordered(8))
    }

    pub async fn query_all(&self, request: ResourceRequest) -> Result<Vec<ProviderResult>> {
        let mut results: Vec<_> = self.query_incremental(request).await?.collect().await;
        results.sort_by_key(|(ordinal, _)| *ordinal);
        Ok(results.into_iter().map(|(_, r)| r).collect())
    }
}

pub fn prepare_playback(
    source: Stream,
    capabilities: &PlayerCapabilities,
    resume_ms: u64,
) -> PlaybackPlan {
    let (disposition, reason) = if source.info_hash.is_some()
        || source
            .url
            .as_ref()
            .is_some_and(|u| u.starts_with("magnet:"))
    {
        if capabilities.torrent && !capabilities.web {
            (
                PlaybackDisposition::MediaService,
                "native torrent engine required",
            )
        } else if capabilities.companion {
            (
                PlaybackDisposition::MediaService,
                "companion server required for torrent delivery",
            )
        } else {
            (
                PlaybackDisposition::Unsupported,
                "torrent delivery is unavailable",
            )
        }
    } else if source.external_url.is_some() {
        if capabilities.external_application {
            (
                PlaybackDisposition::ExternalApplication,
                "external application required",
            )
        } else {
            (
                PlaybackDisposition::Unsupported,
                "external applications are unavailable",
            )
        }
    } else if let Some(url) = source.url.as_ref().and_then(|u| Url::parse(u).ok()) {
        let needs_headers = source.behavior_hints.contains_key("proxyHeaders");
        let not_web_ready = source
            .behavior_hints
            .get("notWebReady")
            .and_then(|v| v.as_bool())
            == Some(true);
        if capabilities.url_schemes.iter().any(|s| s == url.scheme())
            && (!needs_headers || capabilities.request_headers)
            && !(capabilities.web && not_web_ready)
        {
            (
                PlaybackDisposition::Direct,
                "player supports transport; codec support still requires probing",
            )
        } else {
            (
                PlaybackDisposition::Unsupported,
                "source requires a transport, header, or codec capability unavailable in this player",
            )
        }
    } else {
        (
            PlaybackDisposition::Unsupported,
            "source form is preserved but no delivery adapter is available",
        )
    };
    PlaybackPlan {
        disposition,
        reason: reason.into(),
        source,
        resume_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Default)]
    struct MemoryStorage(Mutex<Snapshot>);
    #[async_trait]
    impl Storage for MemoryStorage {
        async fn load(&self) -> Result<Snapshot> {
            Ok(self.0.lock().unwrap().clone())
        }
        async fn compare_and_swap(&self, revision: u64, next: Snapshot) -> Result<()> {
            let mut value = self.0.lock().unwrap();
            if value.revision != revision {
                return Err(Error::new(ErrorCode::Conflict, "stale revision"));
            }
            *value = next;
            Ok(())
        }
    }
    struct NoHttp;
    #[async_trait]
    impl Http for NoHttp {
        async fn get_json(&self, _: Url, _: bool) -> Result<serde_json::Value> {
            panic!("unexpected networking")
        }
    }
    fn core() -> Core {
        Core::new(Arc::new(NoHttp), Arc::new(MemoryStorage::default()))
    }
    fn key(installation: &str) -> ItemKey {
        ItemKey {
            installation_id: installation.into(),
            content_type: "movie".into(),
            item_id: "opaque".into(),
        }
    }

    #[tokio::test]
    async fn hiding_continue_watching_preserves_progress_and_library() {
        let core = core();
        let key = key("a");
        core.save_item(LibraryEntry {
            metadata: None,
            key: key.clone(),
            title: "Show".into(),
        })
        .await
        .unwrap();
        core.record_progress(Progress {
            metadata: None,
            binge_group: None,
            source_provider: None,
            key: key.clone(),
            video_id: "episode:1".into(),
            position_ms: 10000,
            duration_ms: Some(30000),
            completed: false,
        })
        .await
        .unwrap();
        core.set_continue_hidden(&key, true).await.unwrap();
        core.set_continue_hidden(&key, true).await.unwrap();
        let snapshot = core.snapshot().await.unwrap();
        assert_eq!(snapshot.hidden_continue.len(), 1);
        assert_eq!(snapshot.progress[0].position_ms, 10000);
        assert_eq!(snapshot.library.len(), 1);
        core.set_continue_hidden(&key, false).await.unwrap();
        assert!(core.snapshot().await.unwrap().hidden_continue.is_empty());
    }

    #[tokio::test]
    async fn nearly_finished_videos_restart_without_changing_completion() {
        let core = core();
        let source: Stream =
            serde_json::from_value(json!({"url":"https://example.org/video.mp4"})).unwrap();
        let capabilities = PlayerCapabilities {
            url_schemes: vec!["https".into()],
            ..Default::default()
        };
        for (position_ms, duration_ms, expected) in [
            (94_999, Some(100_000), 94_999),
            (95_000, Some(100_000), 95_000),
            (95_001, Some(100_000), 0),
            (100_000, Some(100_000), 0),
            (95_001, None, 95_001),
            (0, Some(0), 0),
            (u64::MAX, Some(u64::MAX), 0),
        ] {
            core.record_progress(Progress {
                metadata: None,
                binge_group: None,
                source_provider: None,
                key: key("a"),
                video_id: "episode:1".into(),
                position_ms,
                duration_ms,
                completed: false,
            })
            .await
            .unwrap();
            assert_eq!(
                core.playback_plan(&key("a"), "episode:1", source.clone(), &capabilities)
                    .await
                    .unwrap()
                    .resume_ms,
                expected,
                "position {position_ms}, duration {duration_ms:?}"
            );
            let saved = core.snapshot().await.unwrap().progress.pop().unwrap();
            assert!(!saved.completed);
            assert_eq!(saved.position_ms, position_ms);
        }
    }

    #[tokio::test]
    async fn progress_is_scoped_and_completion_resets_resume() {
        let core = core();
        let source: Stream =
            serde_json::from_value(json!({"url":"https://example.org/video.mp4"})).unwrap();
        let capabilities = PlayerCapabilities {
            url_schemes: vec!["https".into()],
            ..Default::default()
        };
        let mut progress = Progress {
            metadata: None,
            binge_group: None,
            source_provider: None,
            key: key("a"),
            video_id: "episode:1".into(),
            position_ms: 10000,
            duration_ms: Some(30000),
            completed: false,
        };
        core.record_progress(progress.clone()).await.unwrap();
        assert_eq!(
            core.playback_plan(&key("a"), "episode:1", source.clone(), &capabilities)
                .await
                .unwrap()
                .resume_ms,
            10000
        );
        assert_eq!(
            core.playback_plan(&key("b"), "episode:1", source.clone(), &capabilities)
                .await
                .unwrap()
                .resume_ms,
            0
        );
        progress.completed = true;
        core.record_progress(progress).await.unwrap();
        assert_eq!(
            core.playback_plan(&key("a"), "episode:1", source, &capabilities)
                .await
                .unwrap()
                .resume_ms,
            0
        );
    }

    #[tokio::test]
    async fn slow_subscribers_disconnect_and_can_recover_a_snapshot() {
        let core = core();
        let receiver = core.subscribe();
        for n in 0..40 {
            core.save_item(LibraryEntry {
                metadata: None,
                key: key("a"),
                title: n.to_string(),
            })
            .await
            .unwrap();
        }
        let events: Vec<_> = receiver.collect().await;
        assert!(!events.is_empty() && events.len() < 40);
        let mut fresh = core.subscribe();
        assert_eq!(core.snapshot().await.unwrap().revision, 40);
        core.remove_item(&key("a")).await.unwrap();
        assert_eq!(fresh.next().await.unwrap().revision, 41);
    }

    #[test]
    fn web_torrents_need_companion_and_source_hints_survive() {
        let source: Stream = serde_json::from_value(json!({"infoHash":"0123456789012345678901234567890123456789", "fileIdx":4, "behaviorHints":{"bingeGroup":"test", "future":true}, "futureSourceField":"preserved"})).unwrap();
        let mut capabilities = PlayerCapabilities {
            web: true,
            torrent: true,
            ..Default::default()
        };
        assert_eq!(
            prepare_playback(source.clone(), &capabilities, 0).disposition,
            PlaybackDisposition::Unsupported
        );
        capabilities.companion = true;
        let plan = prepare_playback(source, &capabilities, 0);
        assert_eq!(plan.disposition, PlaybackDisposition::MediaService);
        assert_eq!(plan.source.file_idx, Some(4));
        assert_eq!(plan.source.extra["futureSourceField"], "preserved");
    }
}
