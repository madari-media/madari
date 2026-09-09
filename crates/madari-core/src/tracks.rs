//! Language order takes priority over accessibility/forced-track preferences.
use madari_model::{MediaTrack, PlaybackPreferences, TrackPreference};
fn language(value: &str) -> String {
    let code = value.trim().to_ascii_lowercase().replace('_', "-");
    let mut parts = code.splitn(2, '-');
    let primary = match parts.next().unwrap_or("") {
        "eng" => "en",
        "fra" | "fre" => "fr",
        "deu" | "ger" => "de",
        "spa" => "es",
        "por" => "pt",
        "ita" => "it",
        "hin" => "hi",
        "tam" => "ta",
        "tel" => "te",
        "mal" => "ml",
        "kan" => "kn",
        "mar" => "mr",
        "ben" => "bn",
        "guj" => "gu",
        "pan" => "pa",
        "urd" => "ur",
        "jpn" => "ja",
        "kor" => "ko",
        "zho" | "chi" => "zh",
        "ara" => "ar",
        "rus" => "ru",
        "ukr" => "uk",
        "nld" | "dut" => "nl",
        "pol" => "pl",
        "tur" => "tr",
        "swe" => "sv",
        "dan" => "da",
        "fin" => "fi",
        "nor" => "no",
        "tha" => "th",
        "vie" => "vi",
        "ind" => "id",
        "heb" => "he",
        other => other,
    };
    match parts.next() {
        Some(region) => format!("{primary}-{region}"),
        None => primary.into(),
    }
}
fn penalty(preference: TrackPreference, flag: bool) -> u8 {
    match preference {
        TrackPreference::Any => 0,
        TrackPreference::Prefer => u8::from(!flag),
        TrackPreference::Avoid => u8::from(flag),
    }
}
pub fn preferred_track(
    tracks: &[MediaTrack],
    prefs: &PlaybackPreferences,
    kind: &str,
) -> Option<i64> {
    let (languages, first, second) = match kind {
        "audio" => (
            &prefs.audio_languages,
            prefs.audio_description,
            prefs.audio_commentary,
        ),
        "sub" if prefs.subtitles_enabled => (
            &prefs.subtitle_languages,
            prefs.subtitle_sdh,
            prefs.subtitle_forced,
        ),
        _ => return None,
    };
    if languages.is_empty() && first == TrackPreference::Any && second == TrackPreference::Any {
        return None;
    }
    let selected = tracks.iter().find(|t| t.kind == kind && t.selected);
    let rank = |track: &MediaTrack| {
        let tag = language(&track.language);
        languages
            .iter()
            .enumerate()
            .filter_map(|(i, wanted)| {
                let wanted = language(wanted);
                if wanted == tag {
                    Some((i, 0))
                } else if wanted.split('-').next() == tag.split('-').next() {
                    Some((i, 1))
                } else {
                    None
                }
            })
            .min()
    };
    let have_language = tracks.iter().any(|t| t.kind == kind && rank(t).is_some());
    tracks
        .iter()
        .filter(|t| t.kind == kind)
        .filter_map(|t| {
            let language_rank = if have_language {
                rank(t)?
            } else {
                // Keep the engine's default language when configured languages are unavailable.
                if let Some(selected) = selected {
                    if language(&t.language) != language(&selected.language) {
                        return None;
                    }
                } else if !languages.is_empty() {
                    return None;
                }
                (0, 0)
            };
            let flags = if kind == "audio" {
                (t.visual_impaired, t.commentary)
            } else {
                (t.hearing_impaired, t.forced)
            };
            Some((
                t,
                (
                    language_rank,
                    penalty(first, flags.0) + penalty(second, flags.1),
                    !t.selected,
                ),
            ))
        })
        .min_by_key(|(_, score)| *score)
        .map(|(t, _)| t.id)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn language_preferences_apply_without_any_track_type_preferences() {
        let tracks = vec![
            MediaTrack {
                id: 1,
                kind: "audio".into(),
                language: "eng".into(),
                selected: true,
                ..Default::default()
            },
            MediaTrack {
                id: 2,
                kind: "audio".into(),
                language: "hin".into(),
                ..Default::default()
            },
        ];
        let prefs = PlaybackPreferences {
            audio_languages: vec!["ta".into(), "hi".into(), "en".into()],
            ..Default::default()
        };
        assert_eq!(preferred_track(&tracks, &prefs, "audio"), Some(2));
    }
    #[test]
    fn language_priority_precedes_sdh_and_forced_flags() {
        let tracks = vec![
            MediaTrack {
                id: 1,
                kind: "sub".into(),
                language: "eng".into(),
                selected: true,
                ..Default::default()
            },
            MediaTrack {
                id: 2,
                kind: "sub".into(),
                language: "eng".into(),
                hearing_impaired: true,
                ..Default::default()
            },
            MediaTrack {
                id: 3,
                kind: "sub".into(),
                language: "fra".into(),
                forced: true,
                ..Default::default()
            },
        ];
        let mut p = PlaybackPreferences {
            subtitle_languages: vec!["en".into(), "fr".into()],
            subtitle_sdh: TrackPreference::Prefer,
            ..Default::default()
        };
        assert_eq!(preferred_track(&tracks, &p, "sub"), Some(2));
        p.subtitle_sdh = TrackPreference::Avoid;
        p.subtitle_forced = TrackPreference::Prefer;
        assert_eq!(preferred_track(&tracks, &p, "sub"), Some(1));
        p.subtitle_languages.reverse();
        assert_eq!(preferred_track(&tracks, &p, "sub"), Some(3));
        p.subtitles_enabled = false;
        assert_eq!(preferred_track(&tracks, &p, "sub"), None);
    }
}
