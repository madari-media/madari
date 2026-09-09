pub(in crate::app) mod controls;
pub(in crate::app) mod embedded;
pub(in crate::app) mod engine;
pub(in crate::app) mod internal_stream;
pub(in crate::app) mod languages;
pub(in crate::app) mod up_next;

use madari_model::Subtitle;
use std::collections::BTreeMap;
pub struct Target {
    pub title: String,
    pub context: PlaybackContext,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub resume_ms: u64,
    pub offset_ms: u64,
    pub subtitles: Vec<Subtitle>,
}
#[cfg(test)]
#[path = "tests/process.rs"]
mod process_tests;

#[derive(Clone, Default)]
pub struct PlaybackContext {
    pub metadata: Option<madari_model::Meta>,
    pub preferences: madari_model::PlaybackPreferences,
    pub provider: String,
    pub group: Option<String>,
    pub current_source: String,
    pub attempted: std::collections::HashSet<String>,
    pub transcode: bool,
    pub progress: Vec<madari_model::Progress>,
    pub episodes: Vec<madari_model::Video>,
}
pub fn fingerprint(provider: &str, source: &madari_model::Stream) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "{:x}",
        Sha256::digest(format!(
            "{provider}:{}",
            serde_json::to_string(source).unwrap_or_default()
        ))
    )
}

pub(in crate::app) mod trakt;
