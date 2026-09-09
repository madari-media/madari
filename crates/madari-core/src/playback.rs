use crate::{Core, PlatformBound, prepare_playback};
use async_trait::async_trait;
use http::header::{HeaderName, HeaderValue};
use madari_addon::torrent;
use madari_model::*;
use std::collections::BTreeMap;
use url::Url;

/// Restart completed or nearly finished videos instead of seeking to the credits.
pub fn resume_position(progress: &Progress) -> u64 {
    if progress.completed
        || progress.duration_ms.is_some_and(|duration| {
            duration > 0 && u128::from(progress.position_ms) * 100 > u128::from(duration) * 95
        })
    {
        0
    } else {
        progress.position_ms
    }
}

/// Runtime media delivery is supplied by the companion (or a future local Linux
/// adapter). The shared core owns source, selection and resume policies.
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait PlaybackMedia: PlatformBound {
    async fn resolve_torrent(&self, magnet: String) -> Result<Torrent>;
    async fn create_torrent_ticket(
        &self,
        id: &str,
        file: usize,
        resume_ms: u64,
    ) -> Result<MediaTicket>;
}

fn invalid(message: &str) -> Error {
    Error::new(ErrorCode::InvalidInput, message)
}

fn validate_request(request: &PreparePlaybackRequest) -> Result<()> {
    match (&request.key, &request.video_id) {
        (Some(key), Some(video))
            if !key.installation_id.is_empty()
                && !key.content_type.is_empty()
                && !key.item_id.is_empty()
                && !video.is_empty() => {}
        (None, None) => (),
        _ => {
            return Err(invalid(
                "provide a nonempty key and video_id together, or omit both",
            ));
        }
    }
    let source = &request.source;
    let count = [
        source.info_hash.is_some(),
        source.url.is_some(),
        source.yt_id.is_some(),
        source.external_url.is_some(),
    ]
    .into_iter()
    .filter(|b| *b)
    .count()
        + [
            "nzbUrl", "rarUrls", "zipUrls", "7zipUrls", "tgzUrls", "tarUrls",
        ]
        .into_iter()
        .filter(|key| source.extra.get(*key).is_some_and(|v| !v.is_null()))
        .count();
    if count > 1 {
        return Err(invalid("stream contains conflicting primary source forms"));
    }
    Ok(())
}

fn request_headers(source: &Stream) -> Result<BTreeMap<String, String>> {
    let Some(hints) = source.behavior_hints.get("proxyHeaders") else {
        return Ok(BTreeMap::new());
    };
    let object = hints
        .as_object()
        .ok_or_else(|| invalid("proxyHeaders must be an object"))?;
    let Some(request) = object.get("request") else {
        return Ok(BTreeMap::new());
    };
    let request = request
        .as_object()
        .ok_or_else(|| invalid("proxyHeaders.request must be an object"))?;
    let mut headers = BTreeMap::new();
    for (name, value) in request {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| invalid("invalid source request header name"))?;
        let value = value
            .as_str()
            .ok_or_else(|| invalid("source request header values must be strings"))?;
        HeaderValue::from_str(value).map_err(|_| invalid("invalid source request header value"))?;
        if headers.insert(name.to_string(), value.to_owned()).is_some() {
            return Err(invalid("duplicate source request header name"));
        }
    }
    Ok(headers)
}

fn direct_url(input: &str, external: bool) -> Result<Url> {
    let url = Url::parse(input).map_err(|_| invalid("invalid source URL"))?;
    let allowed = if external {
        matches!(url.scheme(), "http" | "https")
    } else {
        matches!(
            url.scheme(),
            "http" | "https" | "ftp" | "ftps" | "rtmp" | "rtmps" | "rtsp" | "rtsps"
        )
    };
    if !allowed || url.host_str().is_none() {
        return Err(Error::new(
            ErrorCode::UnsupportedTransport,
            "source transport has no supported player integration",
        ));
    }
    Ok(url)
}

fn select_file(
    torrent: &Torrent,
    user_index: Option<usize>,
    addon_index: Option<usize>,
) -> Result<(TorrentFile, FileSelectionReason)> {
    let (file, reason) = if let Some(index) = user_index.or(addon_index) {
        (
            torrent
                .files
                .iter()
                .find(|f| f.index == index)
                .ok_or_else(|| invalid("selected file index is not present in the torrent"))?,
            if user_index.is_some() {
                FileSelectionReason::UserOverride
            } else {
                FileSelectionReason::AddonIndex
            },
        )
    } else {
        (
            torrent
                .files
                .iter()
                .max_by(|a, b| a.length.cmp(&b.length).then_with(|| b.index.cmp(&a.index)))
                .ok_or_else(|| invalid("torrent contains no files"))?,
            FileSelectionReason::LargestFile,
        )
    };
    if file.length == 0 {
        return Err(invalid("selected torrent file is empty"));
    }
    Ok((file.clone(), reason))
}

impl Core {
    /// Resolve a chosen addon source into delivery details. No player is launched
    /// and no library/progress is mutated. Media adapters may register a torrent.
    pub async fn prepare_playback(
        &self,
        request: PreparePlaybackRequest,
        media: &dyn PlaybackMedia,
    ) -> Result<PreparedPlayback> {
        let installation = request.key.as_ref().map(|key| key.installation_id.clone());
        self.prepare_inner(request, media)
            .await
            .map_err(|error| match installation {
                Some(id) => error.for_addon(&id),
                None => error,
            })
    }

    async fn prepare_inner(
        &self,
        request: PreparePlaybackRequest,
        media: &dyn PlaybackMedia,
    ) -> Result<PreparedPlayback> {
        validate_request(&request)?;
        let torrent = torrent::from_stream(&request.source)?;
        if request.file_index.is_some() && torrent.is_none() {
            return Err(invalid("file_index applies only to torrent sources"));
        }
        let mut plan = prepare_playback(request.source, &request.capabilities, 0);
        // Recover state before issuing a media ticket. No URL is created if storage fails.
        if let (Some(key), Some(video)) = (&request.key, &request.video_id) {
            plan = self
                .playback_plan(key, video, plan.source, &request.capabilities)
                .await?;
        }
        let delivery = if let Some(source) = torrent {
            if !request.capabilities.companion
                && (!request.capabilities.torrent || request.capabilities.web)
            {
                PlaybackDelivery::Unsupported
            } else {
                let torrent = media.resolve_torrent(source.magnet).await?;
                if !torrent.id.eq_ignore_ascii_case(&source.info_hash) {
                    return Err(Error::new(
                        ErrorCode::Media,
                        "resolved torrent does not match the requested source",
                    ));
                }
                let (file, selection) =
                    select_file(&torrent, request.file_index, plan.source.file_idx)?;
                let ticket = media
                    .create_torrent_ticket(&torrent.id, file.index, plan.resume_ms)
                    .await?;
                plan.disposition = PlaybackDisposition::MediaService;
                plan.reason =
                    "torrent resolved; media ticket ready; codec support still requires probing"
                        .into();
                PlaybackDelivery::Torrent {
                    torrent,
                    file,
                    selection,
                    media: ticket,
                }
            }
        } else if let Some(url) = &plan.source.url {
            // URL validation never fetches direct media. Headers are delivered only to
            // the platform player; companion HTTP proxying is a separate adapter.
            direct_url(url, false)?;
            let headers = request_headers(&plan.source)?;
            if plan.disposition == PlaybackDisposition::Direct {
                PlaybackDelivery::Direct {
                    url: url.clone(),
                    request_headers: headers,
                }
            } else {
                PlaybackDelivery::Unsupported
            }
        } else if let Some(url) = &plan.source.external_url {
            direct_url(url, true)?;
            if plan.disposition == PlaybackDisposition::ExternalApplication {
                PlaybackDelivery::ExternalApplication { url: url.clone() }
            } else {
                PlaybackDelivery::Unsupported
            }
        } else {
            PlaybackDelivery::Unsupported
        };
        Ok(PreparedPlayback { plan, delivery })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn torrent() -> Torrent {
        Torrent {
            id: "a".repeat(40),
            name: None,
            files: vec![
                TorrentFile {
                    index: 5,
                    name: "movie.mkv".into(),
                    length: 100,
                },
                TorrentFile {
                    index: 2,
                    name: "episode.mkv".into(),
                    length: 100,
                },
                TorrentFile {
                    index: 9,
                    name: "sample.mkv".into(),
                    length: 10,
                },
            ],
        }
    }

    #[test]
    fn file_selection_obeys_override_addon_and_deterministic_largest_fallback() {
        let t = torrent();
        let (file, reason) = select_file(&t, None, None).unwrap();
        assert_eq!(file.index, 2);
        assert_eq!(reason, FileSelectionReason::LargestFile);
        assert_eq!(select_file(&t, None, Some(9)).unwrap().0.index, 9);
        assert_eq!(select_file(&t, Some(5), Some(9)).unwrap().0.index, 5);
        assert!(select_file(&t, Some(0), None).is_err());
    }

    #[test]
    fn header_values_are_validated_and_names_normalized_without_secret_errors() {
        let source: Stream = serde_json::from_value(serde_json::json!({"url":"https://example.org", "behaviorHints":{"proxyHeaders":{"request":{"User-Agent":"Madari", "Authorization":"Bearer secret"}}}})).unwrap();
        assert_eq!(
            request_headers(&source).unwrap()["authorization"],
            "Bearer secret"
        );
        let mut invalid = source;
        invalid.behavior_hints.insert(
            "proxyHeaders".into(),
            serde_json::json!({"request":{"X-Test":"secret\r\nHeader: value"}}),
        );
        assert!(
            !request_headers(&invalid)
                .unwrap_err()
                .to_string()
                .contains("secret")
        );
    }

    #[test]
    fn empty_files_and_local_file_transports_are_not_prepared() {
        let mut t = torrent();
        t.files.clear();
        assert!(select_file(&t, None, None).is_err());
        assert!(direct_url("file:///etc/passwd", false).is_err());
        assert!(direct_url("javascript:alert(1)", true).is_err());
    }
}
