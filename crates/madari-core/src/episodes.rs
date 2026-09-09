//! Deterministic episode order and continuation, independent of the player/platform.
use madari_model::{ItemKey, Progress, Video};

pub fn episode_available(video: &Video, today: &str) -> bool {
    video
        .extra
        .get("released")
        .and_then(|v| v.as_str())
        .and_then(|s| s.get(..10))
        .is_none_or(|date| date <= today)
}
pub fn next_episode<'a>(videos: &'a [Video], current: &str, today: &str) -> Option<&'a Video> {
    let current = videos.iter().find(|v| v.id == current)?;
    let order = (current.season?, current.episode?);
    videos
        .iter()
        .filter(|v| {
            v.season.is_some_and(|s| s > 0)
                && v.season.zip(v.episode).is_some_and(|n| n > order)
                && episode_available(v, today)
        })
        .min_by_key(|v| (v.season, v.episode))
}
pub fn previous_episode<'a>(videos: &'a [Video], current: &str, today: &str) -> Option<&'a Video> {
    let current = videos.iter().find(|v| v.id == current)?;
    let order = (current.season?, current.episode?);
    videos
        .iter()
        .filter(|v| {
            v.season.is_some_and(|s| s > 0)
                && v.season.zip(v.episode).is_some_and(|n| n < order)
                && episode_available(v, today)
        })
        .max_by_key(|v| (v.season, v.episode))
}

pub fn continue_episode<'a>(
    videos: &'a [Video],
    progress: &[Progress],
    key: &ItemKey,
    today: &str,
) -> Option<&'a Video> {
    if let Some(last) = progress.iter().rev().find(|p| &p.key == key) {
        if !last.completed {
            if let Some(video) = videos
                .iter()
                .find(|v| v.id == last.video_id && episode_available(v, today))
            {
                return Some(video);
            }
        } else {
            return next_episode(videos, &last.video_id, today);
        }
    }
    videos
        .iter()
        .filter(|v| v.season != Some(0) && episode_available(v, today))
        .min_by_key(|v| (v.season, v.episode))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn video(id: &str, season: u32, episode: u32, released: &str) -> Video {
        serde_json::from_value(
            serde_json::json!({"id":id,"season":season,"episode":episode,"released":released}),
        )
        .unwrap()
    }
    #[test]
    fn next_crosses_seasons_without_specials_or_future_episodes() {
        let videos = vec![
            video("future", 2, 2, "2099-01-01"),
            video("b", 2, 1, "2020-01-01"),
            video("special", 0, 2, "2020-01-01"),
            video("a", 1, 3, "2020-01-01"),
        ];
        assert_eq!(next_episode(&videos, "a", "2026-01-01").unwrap().id, "b");
        assert!(next_episode(&videos, "b", "2026-01-01").is_none());
        assert!(next_episode(&videos, "missing", "2026-01-01").is_none());
        assert_eq!(
            previous_episode(&videos, "b", "2026-01-01").unwrap().id,
            "a"
        );
        assert!(previous_episode(&videos, "a", "2026-01-01").is_none());
    }
    #[test]
    fn continue_resumes_latest_episode_then_advances_when_completed() {
        let videos = vec![
            video("a", 1, 1, "2020-01-01"),
            video("b", 1, 2, "2020-01-01"),
        ];
        let key = ItemKey {
            installation_id: "addon".into(),
            content_type: "series".into(),
            item_id: "show".into(),
        };
        let mut progress = vec![Progress {
            metadata: None,
            binge_group: None,
            source_provider: None,
            key: key.clone(),
            video_id: "a".into(),
            position_ms: 100,
            duration_ms: Some(200),
            completed: false,
        }];
        assert_eq!(
            continue_episode(&videos, &progress, &key, "2026-01-01")
                .unwrap()
                .id,
            "a"
        );
        progress[0].completed = true;
        assert_eq!(
            continue_episode(&videos, &progress, &key, "2026-01-01")
                .unwrap()
                .id,
            "b"
        );
        progress.push(Progress {
            metadata: None,
            binge_group: None,
            source_provider: None,
            video_id: "b".into(),
            ..progress[0].clone()
        });
        assert!(continue_episode(&videos, &progress, &key, "2026-01-01").is_none());
    }
}
