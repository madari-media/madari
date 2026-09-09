//! Explicit media identities and confirmed scrobble outcomes.
use super::{Client, Tokens, invalid};
use madari_model::{ItemKey, Result, Video};
use serde_json::{Value, json};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Start,
    Pause,
    Stop,
}
impl Action {
    fn path(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Pause => "pause",
            Self::Stop => "stop",
        }
    }
}
#[derive(Clone)]
pub struct Scrobble {
    pub media: Media,
    pub action: Action,
    pub progress: f64,
}
#[derive(Clone)]
pub struct Media {
    body: Value,
    resolved: std::sync::Arc<tokio::sync::OnceCell<Value>>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Watching,
    Paused,
    Watched,
}
impl Media {
    pub fn from_video(key: &ItemKey, video_id: &str, episodes: &[Video]) -> Option<Self> {
        let id = &key.item_id;
        let ids = if let Some(number) = id
            .strip_prefix("tt")
            .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        {
            json!({"imdb":format!("tt{number}")})
        } else if let Some(number) = id.strip_prefix("tmdb:").and_then(|n| n.parse::<u64>().ok()) {
            json!({"tmdb":number})
        } else if let Some(number) = id
            .strip_prefix("trakt:")
            .and_then(|n| n.parse::<u64>().ok())
        {
            json!({"trakt":number})
        } else {
            return None;
        };
        let body = match key.content_type.as_str() {
            "movie" => json!({"movie":{"ids":ids}}),
            "series" => {
                let episode = episodes.iter().find(|v| v.id == video_id)?;
                json!({"show":{"ids":ids},"episode":{"season":episode.season?,"number":episode.episode?}})
            }
            _ => return None,
        };
        Some(Self {
            body,
            resolved: Default::default(),
        })
    }
}
impl Client {
    pub(crate) async fn scrobble(&self, tokens: &Tokens, event: &Scrobble) -> Result<Outcome> {
        if !event.progress.is_finite() || !(0.0..=100.0).contains(&event.progress) {
            return Err(invalid("Invalid Trakt playback percentage."));
        }
        let mut body = event
            .media
            .resolved
            .get_or_try_init(|| async {
                if event.media.body.get("movie").is_some() {
                    return Ok(event.media.body.clone());
                }
                let ids = &event.media.body["show"]["ids"];
                let show = if let Some(id) = ids["trakt"].as_u64() {
                    id.to_string()
                } else if let Some(id) = ids["imdb"].as_str() {
                    id.to_owned()
                } else if let Some(id) = ids["tmdb"].as_u64() {
                    let matches: Vec<Value> = super::client::decode(
                        self.get(&format!("/search/tmdb/{id}?type=show"), tokens, 1)
                            .await?,
                    )
                    .await?;
                    matches
                        .iter()
                        .find(|m| m["show"]["ids"]["tmdb"].as_u64() == Some(id))
                        .and_then(|m| m["show"]["ids"]["trakt"].as_u64())
                        .ok_or_else(|| invalid("Trakt could not match this show."))?
                        .to_string()
                } else {
                    return Err(invalid("Trakt could not identify this show."));
                };
                let season = event.media.body["episode"]["season"]
                    .as_u64()
                    .ok_or_else(|| invalid("Missing episode season."))?;
                let number = event.media.body["episode"]["number"]
                    .as_u64()
                    .ok_or_else(|| invalid("Missing episode number."))?;
                let episode: Value = super::client::decode(
                    self.get(
                        &format!("/shows/{show}/seasons/{season}/episodes/{number}"),
                        tokens,
                        1,
                    )
                    .await?,
                )
                .await?;
                if episode["season"].as_u64() != Some(season)
                    || episode["number"].as_u64() != Some(number)
                {
                    return Err(invalid(
                        "Trakt returned a different episode; playback was not synced.",
                    ));
                }
                let id = episode["ids"]["trakt"]
                    .as_u64()
                    .ok_or_else(|| invalid("Trakt did not return an episode ID."))?;
                Ok(json!({"episode":{"ids":{"trakt":id}}}))
            })
            .await?
            .clone();
        body["progress"] = json!(event.progress);
        let response = self
            .scrobble_request(tokens, event.action.path(), body)
            .await?;
        if response.status().as_u16() == 409 && event.action == Action::Stop {
            let value: Value = super::client::decode(response).await?;
            if value["watched_at"].is_string() {
                return Ok(Outcome::Watched);
            }
            return Err(invalid("Trakt could not confirm this watched item."));
        }
        super::client::check(&response)?;
        let value: Value = super::client::decode(response).await?;
        match (event.action, value["action"].as_str()) {
            (Action::Start, Some("start")) => Ok(Outcome::Watching),
            (Action::Pause, Some("pause")) | (Action::Stop, Some("pause")) => Ok(Outcome::Paused),
            (Action::Stop, Some("scrobble")) => Ok(Outcome::Watched),
            _ => Err(invalid("Trakt did not confirm the playback update.")),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn episode_payload_uses_show_id_and_actual_season_episode() {
        let key = ItemKey {
            installation_id: "addon".into(),
            content_type: "series".into(),
            item_id: "tt123".into(),
        };
        let video: Video =
            serde_json::from_value(json!({"id":"opaque-video","season":2,"episode":4})).unwrap();
        let media = Media::from_video(&key, "opaque-video", &[video]).unwrap();
        assert_eq!(
            media.body,
            json!({"show":{"ids":{"imdb":"tt123"}},"episode":{"season":2,"number":4}})
        );
        assert!(Media::from_video(&key, "tt123:2:4", &[]).is_none());
        assert!(
            Media::from_video(
                &ItemKey {
                    item_id: "custom-id".into(),
                    ..key
                },
                "custom-id",
                &[]
            )
            .is_none()
        );
    }
}
