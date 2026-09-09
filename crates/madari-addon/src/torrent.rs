//! Normalize addon torrent hints without depending on a native torrent engine.
use madari_model::{Error, ErrorCode, Result, Stream};
use std::collections::BTreeSet;
use url::Url;

// Do not derive Debug: trackers and display names can carry private information.
pub struct TorrentSource {
    pub info_hash: String,
    pub magnet: String,
}

fn invalid(message: &str) -> Error {
    Error::new(ErrorCode::InvalidInput, message)
}

fn info_hash(value: &str) -> Result<String> {
    if value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(value.to_ascii_lowercase());
    }
    Err(invalid(
        "expected a 40-character hexadecimal BitTorrent v1 info hash",
    ))
}

fn tracker(value: &str) -> Result<String> {
    let url = Url::parse(value).map_err(|_| invalid("invalid torrent tracker URL"))?;
    if !matches!(url.scheme(), "http" | "https" | "udp")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid(
            "torrent tracker must use HTTP(S) or UDP without userinfo or fragment",
        ));
    }
    Ok(url.to_string())
}

/// Rebuild magnets from supported fields. In particular, never forward unbounded
/// `so` ranges to the engine; file selection belongs to Madari's explicit policy.
pub fn normalize_magnet(input: &str) -> Result<TorrentSource> {
    if input.len() > 16 * 1024 {
        return Err(invalid("magnet exceeds 16 KiB"));
    }
    let url = Url::parse(input).map_err(|_| invalid("invalid magnet URL"))?;
    if url.scheme() != "magnet"
        || url.host_str().is_some()
        || !url.path().is_empty()
        || url.fragment().is_some()
    {
        return Err(invalid(
            "expected a magnet query without authority, path or fragment",
        ));
    }
    let mut hash = None;
    let mut trackers = BTreeSet::new();
    let mut display_name = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "xt" => {
                if let Some(value) = value.strip_prefix("urn:btih:") {
                    let next = info_hash(value)?;
                    if hash.as_ref().is_some_and(|previous| previous != &next) {
                        return Err(invalid("magnet contains conflicting v1 info hashes"));
                    }
                    hash = Some(next);
                } // A hybrid magnet's v2 hash is not used by the current v1 engine.
            }
            "tr" => {
                trackers.insert(tracker(&value)?);
            }
            "dn" if display_name.is_none() => display_name = Some(value.into_owned()),
            _ => (),
        }
    }
    let hash =
        hash.ok_or_else(|| invalid("magnet requires a hexadecimal BitTorrent v1 info hash"))?;
    build(hash, trackers, display_name)
}

fn build(hash: String, trackers: BTreeSet<String>, name: Option<String>) -> Result<TorrentSource> {
    if trackers.len() > 64 {
        return Err(invalid("torrent has more than 64 trackers"));
    }
    let mut url = Url::parse("magnet:").expect("static magnet URL");
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("xt", &format!("urn:btih:{hash}"));
        for tracker in trackers {
            query.append_pair("tr", &tracker);
        }
        if let Some(name) = name {
            query.append_pair("dn", &name);
        }
    }
    if url.as_str().len() > 16 * 1024 {
        return Err(invalid("normalized magnet exceeds 16 KiB"));
    }
    Ok(TorrentSource {
        info_hash: hash,
        magnet: url.into(),
    })
}

pub fn from_stream(source: &Stream) -> Result<Option<TorrentSource>> {
    let magnet = source.url.as_deref().filter(|u| {
        u.get(..7)
            .is_some_and(|s| s.eq_ignore_ascii_case("magnet:"))
    });
    if source.info_hash.is_some() && source.url.is_some() {
        return Err(invalid("stream must not contain both infoHash and url"));
    }
    let existing = magnet.map(normalize_magnet).transpose()?;
    let hash = match (source.info_hash.as_deref(), &existing) {
        (Some(hash), _) => info_hash(hash)?,
        (_, Some(existing)) => existing.info_hash.clone(),
        _ => return Ok(None),
    };
    let mut trackers = BTreeSet::new();
    let mut name = None;
    if let Some(existing) = existing {
        for (key, value) in Url::parse(&existing.magnet)
            .expect("normalized magnet")
            .query_pairs()
        {
            if key == "tr" {
                trackers.insert(value.into_owned());
            } else if key == "dn" {
                name = Some(value.into_owned());
            }
        }
    }
    if source.sources.len() > 64 {
        return Err(invalid("stream has more than 64 discovery hints"));
    }
    for hint in &source.sources {
        if let Some(value) = hint.strip_prefix("tracker:") {
            trackers.insert(tracker(value)?);
        }
        // DHT hints and unknown fields remain in the original stream. The current
        // engine discovers peers using its own DHT; no alternate DHT nodes injected.
    }
    build(hash, trackers, name).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const HASH: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn tracker_credentials_are_encoded_once_and_other_hints_preserved() {
        let source: Stream = serde_json::from_value(json!({"infoHash":HASH.to_uppercase(), "fileIdx":2,
            "sources":["tracker:https://tracker.example/a%2Fb/announce?token=a%2Bb", "tracker:udp://tracker.example:80", "dht:other"],
            "behaviorHints":{"bingeGroup":"test"}})).unwrap();
        let normalized = from_stream(&source).unwrap().unwrap();
        let url = Url::parse(&normalized.magnet).unwrap();
        assert_eq!(normalized.info_hash, HASH);
        assert!(
            url.query_pairs().any(
                |(k, v)| k == "tr" && v == "https://tracker.example/a%2Fb/announce?token=a%2Bb"
            )
        );
        assert_eq!(source.sources[2], "dht:other");
    }

    #[test]
    fn unsafe_ranges_and_unknown_magnet_parameters_are_not_forwarded() {
        let source = normalize_magnet(&format!(
            "magnet:?xt=urn:btih:{HASH}&so=0-18446744073709551615&xs=file:///private&dn=Film"
        ))
        .unwrap();
        let url = Url::parse(&source.magnet).unwrap();
        assert!(
            url.query_pairs()
                .all(|(k, _)| matches!(k.as_ref(), "xt" | "dn" | "tr"))
        );
    }

    #[test]
    fn invalid_or_conflicting_sources_fail_without_echoing_secrets() {
        for input in [
            format!("magnet:?xt=urn:btih:{HASH}&xt=urn:btih:{}", "a".repeat(40)),
            "magnet:?xt=urn:btih:secret".into(),
            format!("magnet:?xt=urn:btih:{HASH}&tr=file:///secret"),
        ] {
            let error = normalize_magnet(&input).err().unwrap();
            assert!(!error.to_string().contains("secret"));
        }
    }
}
