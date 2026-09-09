//! Addon-declared continuity identifiers are opaque: never infer quality matches.
use madari_model::Stream;
pub fn binge_group(stream: &Stream) -> Option<&str> {
    stream
        .behavior_hints
        .get("bingeGroup")
        .or_else(|| stream.extra.get("bingeGroup"))
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
}
pub fn same_binge_group(stream: &Stream, expected: &str) -> bool {
    !expected.trim().is_empty() && binge_group(stream) == Some(expected)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_progress_has_no_source_preference() {
        let p: madari_model::Progress = serde_json::from_value(serde_json::json!({
            "key": {"installation_id":"addon", "content_type":"movie", "item_id":"tt1"},
            "video_id":"tt1", "position_ms":12000, "duration_ms":90000, "completed":false
        }))
        .unwrap();
        assert!(p.binge_group.is_none());
        assert!(p.source_provider.is_none());
        assert_eq!(p.position_ms, 12000);
    }
    #[test]
    fn continuity_requires_an_exact_nonempty_group() {
        let s:Stream=serde_json::from_value(serde_json::json!({"name":"Torrentio 1080p","behaviorHints":{"bingeGroup":"torrentio|1080p|WEBRip|x264"}})).unwrap();
        assert!(same_binge_group(&s, "torrentio|1080p|WEBRip|x264"));
        for group in [
            "",
            "torrentio|1080p",
            "Torrentio|1080p|WEBRip|x264",
            "torrentio|1080p|WEBRip|x265",
        ] {
            assert!(!same_binge_group(&s, group));
        }
        let absent: Stream =
            serde_json::from_value(serde_json::json!({"name":"Torrentio 1080p"})).unwrap();
        assert_eq!(binge_group(&absent), None);
    }
}
