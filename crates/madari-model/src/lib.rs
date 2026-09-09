//! Portable public contracts. No runtime, storage, torrent, or player dependencies.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const API_VERSION: u32 = 1;
pub type Fields = BTreeMap<String, Value>;

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{code:?}: {message}")]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installation_id: Option<String>,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidInput,
    UnsupportedTransport,
    UnsupportedResource,
    ConfigurationRequired,
    NotFound,
    Network,
    Timeout,
    ResponseTooLarge,
    InvalidResponse,
    Storage,
    Conflict,
    Forbidden,
    Media,
    Busy,
}

impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            installation_id: None,
        }
    }
    pub fn for_addon(mut self, id: &str) -> Self {
        self.installation_id = Some(id.to_owned());
        self
    }
}
pub type Result<T> = std::result::Result<T, Error>;

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resource {
    Catalog,
    Meta,
    Stream,
    Subtitles,
    AddonCatalog,
}

impl Resource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Catalog => "catalog",
            Self::Meta => "meta",
            Self::Stream => "stream",
            Self::Subtitles => "subtitles",
            Self::AddonCatalog => "addon_catalog",
        }
    }
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequest {
    pub resource: Resource,
    #[serde(rename = "type")]
    pub content_type: String,
    pub id: String,
    #[serde(default)]
    pub extra: BTreeMap<String, String>,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    pub id: String,
    #[serde(rename = "type")]
    pub content_type: String,
    pub name: String,
    #[serde(default, deserialize_with = "null_as_empty")]
    pub videos: Vec<Video>,
    #[serde(flatten)]
    #[cfg_attr(feature = "openapi", schema(value_type = std::collections::BTreeMap<String, serde_json::Value>))]
    pub extra: Fields,
}

fn null_as_empty<'de, D, T>(deserializer: D) -> std::result::Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}
impl Meta {
    /// Durable card data, separate from expiring episode/resource caches.
    pub fn preview(&self) -> Self {
        Self {
            id: self.id.clone(),
            content_type: self.content_type.clone(),
            name: self.name.clone(),
            videos: Vec::new(),
            extra: self
                .extra
                .iter()
                .filter(|(key, _)| {
                    matches!(
                        key.as_str(),
                        "poster" | "posterShape" | "background" | "logo" | "behaviorHints"
                    )
                })
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        }
    }
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Video {
    pub id: String,
    #[serde(alias = "name")]
    pub title: Option<String>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    #[serde(flatten)]
    #[cfg_attr(feature = "openapi", schema(value_type = std::collections::BTreeMap<String, serde_json::Value>))]
    pub extra: Fields,
}

// Deliberately no Debug: sources/hints can contain credential-bearing URLs.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stream {
    pub name: Option<String>,
    pub title: Option<String>,
    pub url: Option<String>,
    pub info_hash: Option<String>,
    pub file_idx: Option<usize>,
    pub yt_id: Option<String>,
    pub external_url: Option<String>,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub subtitles: Vec<Subtitle>,
    #[serde(default)]
    #[cfg_attr(feature = "openapi", schema(value_type = std::collections::BTreeMap<String, serde_json::Value>))]
    pub behavior_hints: Fields,
    #[serde(flatten)]
    #[cfg_attr(feature = "openapi", schema(value_type = std::collections::BTreeMap<String, serde_json::Value>))]
    pub extra: Fields,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
pub struct Subtitle {
    pub id: Option<String>,
    pub url: String,
    pub lang: String,
    #[serde(flatten)]
    #[cfg_attr(feature = "openapi", schema(value_type = std::collections::BTreeMap<String, serde_json::Value>))]
    pub extra: Fields,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "resource", content = "data", rename_all = "snake_case")]
pub enum ResourceData {
    Catalog(Vec<Meta>),
    Meta(Option<Meta>),
    Stream(Vec<Stream>),
    Subtitles(Vec<Subtitle>),
    AddonCatalog(Vec<Value>),
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
pub struct ProviderResult {
    pub installation_id: String,
    #[cfg_attr(feature = "openapi", schema(value_type = ProviderOutcome))]
    pub result: Result<ResourceData>,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayerCapabilities {
    #[serde(default)]
    pub url_schemes: Vec<String>,
    #[serde(default)]
    pub torrent: bool,
    #[serde(default)]
    pub companion: bool,
    #[serde(default)]
    pub external_application: bool,
    #[serde(default)]
    pub request_headers: bool,
    #[serde(default)]
    pub web: bool,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackDisposition {
    Direct,
    MediaService,
    ExternalApplication,
    Unsupported,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
pub struct PlaybackPlan {
    pub disposition: PlaybackDisposition,
    pub reason: String,
    pub source: Stream,
    pub resume_ms: u64,
}

/// A selected addon source plus optional library identity for saved resume state.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
pub struct PreparePlaybackRequest {
    pub source: Stream,
    pub capabilities: PlayerCapabilities,
    pub key: Option<ItemKey>,
    pub video_id: Option<String>,
    /// Overrides the addon's fileIdx. Without either index, select the largest file.
    pub file_index: Option<usize>,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileSelectionReason {
    UserOverride,
    AddonIndex,
    LargestFile,
}

/// Relative paths are resolved against the companion origin, never the addon origin.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
pub struct MediaTicket {
    pub token: String,
    pub expires_in_seconds: u64,
    pub direct_path: String,
    pub transcode_path: String,
    /// The transcode path starts at this offset; add it to player-relative progress.
    #[serde(default)]
    pub transcode_start_ms: u64,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlaybackDelivery {
    Direct {
        url: String,
        request_headers: BTreeMap<String, String>,
    },
    Torrent {
        torrent: Torrent,
        file: TorrentFile,
        selection: FileSelectionReason,
        media: MediaTicket,
    },
    ExternalApplication {
        url: String,
    },
    Unsupported,
}

/// Materialized transport, not a guarantee that a player's codecs can decode it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
pub struct PreparedPlayback {
    pub plan: PlaybackPlan,
    pub delivery: PlaybackDelivery,
}

/// Opaque identities are scoped to an installation; no implicit cross-provider merge.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemKey {
    pub installation_id: String,
    pub content_type: String,
    pub item_id: String,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Meta>,
    pub key: ItemKey,
    pub title: String,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrackPreference {
    #[default]
    Any,
    Prefer,
    Avoid,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MediaTrack {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub language: String,
    pub selected: bool,
    pub hearing_impaired: bool,
    pub forced: bool,
    pub visual_impaired: bool,
    pub commentary: bool,
}

/// Ordered language preferences are profile-wide; track IDs vary between files.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PlaybackPreferences {
    pub subtitle_sdh: TrackPreference,
    pub subtitle_forced: TrackPreference,
    pub audio_description: TrackPreference,
    pub audio_commentary: TrackPreference,
    pub audio_languages: Vec<String>,
    pub subtitle_languages: Vec<String>,
    pub subtitles_enabled: bool,
}
impl Default for PlaybackPreferences {
    fn default() -> Self {
        Self {
            subtitle_sdh: TrackPreference::Any,
            subtitle_forced: TrackPreference::Any,
            audio_description: TrackPreference::Any,
            audio_commentary: TrackPreference::Any,
            audio_languages: Vec::new(),
            subtitle_languages: Vec::new(),
            subtitles_enabled: true,
        }
    }
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Meta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binge_group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_provider: Option<String>,
    pub key: ItemKey,
    pub video_id: String,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    pub completed: bool,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
pub struct TorrentFile {
    pub index: usize,
    pub name: String,
    pub length: u64,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
pub struct Torrent {
    pub id: String,
    pub name: Option<String>,
    pub files: Vec<TorrentFile>,
}

/// Schema equivalent of serde's externally tagged Result representation.
#[cfg(feature = "openapi")]
#[derive(utoipa::ToSchema)]
pub enum ProviderOutcome {
    Ok(ResourceData),
    Err(Error),
}
