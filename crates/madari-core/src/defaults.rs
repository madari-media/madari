//! The addons a fresh profile starts with.
//!
//! Madari installs nothing behind the user's back. These are offered once per profile,
//! recorded in the snapshot by `Core::install_default_addons`, so a default the user
//! removes stays removed.

use serde::Serialize;

/// One curated addon, with the text a client shows before it is installed.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct DefaultAddon {
    pub url: &'static str,
    pub name: &'static str,
    pub description: &'static str,
}

/// Curated addons offered once to a profile.
///
/// Cinemeta supplies the catalogs and metadata that make a new installation useful
/// immediately; OpenSubtitles supplies subtitle discovery. Both are public, key-free
/// Stremio addons. The list lives here so no client has to hardcode the URLs.
pub const DEFAULT_ADDONS: &[DefaultAddon] = &[
    DefaultAddon {
        url: "https://v3-cinemeta.strem.io/manifest.json",
        name: "Cinemeta",
        description: "Catalogs and metadata for movies and series",
    },
    DefaultAddon {
        url: "https://opensubtitles-v3.strem.io/manifest.json",
        name: "OpenSubtitles v3",
        description: "Subtitle search and download",
    },
];

/// One default addon that could not be installed, and why.
#[derive(Debug, Clone, Serialize)]
pub struct DefaultAddonFailure {
    pub url: String,
    /// The core's own message. Reported rather than discarded, because "could not be
    /// installed" is not actionable to a user or a developer.
    pub reason: String,
}

/// What happened when the defaults were offered to a profile.
#[derive(Debug, Clone, Serialize)]
pub struct DefaultAddonOutcome {
    /// The profile already had addons, so nothing was installed.
    pub skipped: bool,
    /// Manifest URLs installed by this call.
    pub installed: Vec<String>,
    /// Defaults that could not be installed. One unreachable addon must not cost the
    /// user the others, so these are reported rather than raised.
    pub failed: Vec<DefaultAddonFailure>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Core, Http, Snapshot, Storage};
    use async_trait::async_trait;
    use madari_model::{Error, ErrorCode, Result};
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use url::Url;

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

    /// Serves a valid manifest, and can be made to fail for the subtitle addon so a
    /// partial first run is covered.
    struct FixtureHttp {
        subtitle_host_reachable: AtomicBool,
    }

    impl FixtureHttp {
        fn new(subtitle_host_reachable: bool) -> Arc<Self> {
            Arc::new(Self {
                subtitle_host_reachable: AtomicBool::new(subtitle_host_reachable),
            })
        }

        fn set_subtitle_host_reachable(&self, reachable: bool) {
            self.subtitle_host_reachable
                .store(reachable, Ordering::Relaxed);
        }
    }

    #[async_trait]
    impl Http for FixtureHttp {
        async fn get_json(&self, url: Url, _: bool) -> Result<serde_json::Value> {
            if url.host_str().unwrap_or_default().contains("opensubtitles")
                && !self.subtitle_host_reachable.load(Ordering::Relaxed)
            {
                return Err(Error::new(ErrorCode::Network, "unreachable"));
            }
            Ok(json!({
                "id": "org.test.addon",
                "version": "1.0.0",
                "name": "Fixture",
                "resources": ["catalog"],
                "types": ["movie"],
                "catalogs": [{"type": "movie", "id": "top", "name": "Top"}],
            }))
        }
    }

    fn core(http: Arc<FixtureHttp>) -> Core {
        Core::new(http, Arc::new(MemoryStorage::default()))
    }

    #[tokio::test]
    async fn installs_every_default_and_then_stops_offering_them() {
        let core = core(FixtureHttp::new(true));
        let outcome = core.install_default_addons().await.unwrap();
        assert!(!outcome.skipped);
        assert_eq!(outcome.installed.len(), DEFAULT_ADDONS.len());
        assert!(outcome.failed.is_empty());
        assert_eq!(
            core.snapshot().await.unwrap().addons.len(),
            DEFAULT_ADDONS.len()
        );

        // Offered once: a default the user later removes stays removed.
        let second = core.install_default_addons().await.unwrap();
        assert!(second.skipped);
        assert!(second.installed.is_empty());
        assert_eq!(
            core.snapshot().await.unwrap().addons.len(),
            DEFAULT_ADDONS.len()
        );
    }

    #[tokio::test]
    async fn a_partial_first_run_is_retried_and_never_leaves_a_gap() {
        let http = FixtureHttp::new(false);
        let core = core(http.clone());
        let first = core.install_default_addons().await.unwrap();
        assert_eq!(first.installed, vec![DEFAULT_ADDONS[0].url]);
        assert_eq!(first.failed.len(), 1);
        assert_eq!(first.failed[0].url, DEFAULT_ADDONS[1].url);
        assert_eq!(first.failed[0].reason, "unreachable");
        assert_eq!(core.snapshot().await.unwrap().addons.len(), 1);

        // The next open finds it reachable and fills the gap rather than deciding the
        // profile is already set up.
        http.set_subtitle_host_reachable(true);
        let second = core.install_default_addons().await.unwrap();
        assert_eq!(second.installed, vec![DEFAULT_ADDONS[1].url]);
        assert!(second.failed.is_empty());
        assert_eq!(
            core.snapshot().await.unwrap().addons.len(),
            DEFAULT_ADDONS.len()
        );
    }
}
