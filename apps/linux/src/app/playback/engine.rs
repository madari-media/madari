//! Normal libmpv calls stay on this worker; GTK only owns the OpenGL render API.
use crate::app::playback::Target;
use libmpv2::{
    Format, Mpv,
    events::{Event, PropertyData},
};
use std::{
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

pub use madari_model::MediaTrack as Track;
#[derive(Clone, Debug, PartialEq)]
pub struct Chapter {
    pub index: i64,
    pub title: String,
    pub start_ms: Option<u64>,
}
pub enum Command {
    Start(Box<Target>),
    Run(Vec<String>),
    Set(String, String),
    #[cfg(test)]
    Reattach,
    Shutdown,
}
pub enum Notice {
    Loaded,
    Number(String, f64),
    Flag(String, bool),
    Tracks(Vec<Track>),
    Chapters(Vec<Chapter>),
    Ended(bool),
    Error(String),
    Warning(String),
}
#[derive(Clone)]
pub struct Engine(
    pub mpsc::Sender<Command>,
    tokio_util::sync::CancellationToken,
);
impl Engine {
    pub fn shutdown(&self) {
        self.1.cancel();
        let _ = self.0.send(Command::Shutdown);
    }
    pub fn command(&self, args: &[&str]) {
        let _ = self
            .0
            .send(Command::Run(args.iter().map(|v| v.to_string()).collect()));
    }
    pub fn set(&self, name: &str, value: impl ToString) {
        let _ = self.0.send(Command::Set(name.into(), value.to_string()));
    }
}
#[cfg(test)]
pub fn start(mpv: Arc<Mpv>) -> (Engine, mpsc::Receiver<Notice>) {
    start_with_media(mpv, None)
}
pub fn start_with_media(
    mpv: Arc<Mpv>,
    media: Option<(
        Arc<madari_native::internal_media::InternalMedia>,
        tokio::runtime::Handle,
    )>,
) -> (Engine, mpsc::Receiver<Notice>) {
    let cancel = tokio_util::sync::CancellationToken::new();
    let cancel_io = cancel.clone();
    let (send, receive) = mpsc::channel();
    let (events, notices) = mpsc::channel();
    std::thread::Builder::new()
        .name("madari-player".into())
        .spawn(move || {
            // Keep registration alive until stop has released all demuxer streams.
            let _protocol = match media {
                Some((media, runtime)) => {
                    match crate::app::playback::internal_stream::register(
                        &mpv, media, runtime, cancel_io,
                    ) {
                        Ok(protocol) => Some(protocol),
                        Err(_) => {
                            let _ = events.send(Notice::Error(
                                "Could not initialize internal media playback.".into(),
                            ));
                            return;
                        }
                    }
                }
                None => None,
            };
            let numbers = [
                "time-pos",
                "duration",
                "volume",
                "speed",
                "cache-buffering-state",
                "demuxer-cache-time",
                "demuxer-cache-duration",
                "sub-scale",
                "sub-delay",
                "audio-delay",
            ];
            let flags = [
                "pause",
                "paused-for-cache",
                "seeking",
                "seekable",
                "mute",
                "eof-reached",
            ];
            for (id, name) in numbers.iter().enumerate() {
                let _ = mpv.observe_property(name, Format::Double, id as u64);
            }
            for (id, name) in flags.iter().enumerate() {
                let _ = mpv.observe_property(name, Format::Flag, 100 + id as u64);
            }
            let _ = mpv.observe_property("track-list/count", Format::Int64, 300);
            let mut subtitles = Vec::new();
            let mut preferences = madari_model::PlaybackPreferences::default();
            let mut auto_audio = true;
            let mut auto_subtitles = true;
            let mut tracks_due = true;
            let mut last_tracks = Instant::now();
            loop {
                match receive.recv_timeout(Duration::from_millis(20)) {
                    Ok(Command::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                        let _ = mpv.command("stop", &[]);
                        break;
                    }
                    Ok(Command::Start(target)) => {
                        preferences = target.context.preferences.clone();
                        auto_audio = true;
                        auto_subtitles = true;
                        let result = (|| {
                            let preferences = &target.context.preferences;
                            mpv.set_property("alang", preferences.audio_languages.join(","))?;
                            mpv.set_property("slang", preferences.subtitle_languages.join(","))?;
                            mpv.set_property("aid", "auto")?;
                            if !preferences.subtitle_languages.is_empty() {
                                mpv.set_property("subs-with-matching-audio", "yes")?;
                            }
                            mpv.set_property(
                                "sid",
                                if preferences.subtitles_enabled {
                                    "auto"
                                } else {
                                    "no"
                                },
                            )?;
                            let headers = target
                                .headers
                                .iter()
                                .map(|(k, v)| {
                                    let h = format!("{k}: {v}");
                                    format!("%{}%{h}", h.len())
                                })
                                .collect::<Vec<_>>()
                                .join(",");
                            if !headers.is_empty() {
                                mpv.set_property("http-header-fields", headers)?;
                            }
                            if target.headers.is_empty() {
                                subtitles = target.subtitles;
                            }
                            mpv.set_property(
                                "start",
                                format!("{:.3}", target.resume_ms as f64 / 1000.0),
                            )?;
                            mpv.command("loadfile", &[&target.url, "replace"])
                        })();
                        if result.is_err() {
                            let _ = events
                                .send(Notice::Error("Could not open this video source.".into()));
                        }
                    }
                    Ok(Command::Set(name, value)) => {
                        if name == "aid" {
                            auto_audio = false;
                        }
                        if name == "sid" {
                            auto_subtitles = false;
                        }
                        if mpv.set_property(&name, value).is_err() {
                            let _ = events.send(Notice::Warning(format!(
                                "Could not change {}.",
                                name.replace('-', " ")
                            )));
                        }
                        tracks_due = true;
                    }
                    Ok(Command::Run(args)) => {
                        if args.first().is_some_and(|a| a == "cycle" || a == "set") {
                            if args.get(1).is_some_and(|a| a == "aid") {
                                auto_audio = false;
                            }
                            if args.get(1).is_some_and(|a| a == "sid") {
                                auto_subtitles = false;
                            }
                        }
                        if args.first().is_some_and(|a| a == "sub-add") {
                            auto_subtitles = false;
                        }
                        if let Some((name, args)) = args.split_first()
                            && mpv
                                .command(name, &args.iter().map(String::as_str).collect::<Vec<_>>())
                                .is_err()
                        {
                            let _ = events.send(Notice::Warning(
                                "That action is unavailable for this stream.".into(),
                            ));
                        }
                        tracks_due = true;
                    }
                    #[cfg(test)]
                    Ok(Command::Reattach) => {
                        // Freeing a render context disables its video output. Reopen
                        // the output after GTK has created the replacement GL context.
                        let result = mpv
                            .set_property("vid", "no")
                            .and_then(|_| mpv.set_property("vo", "libmpv"))
                            .and_then(|_| mpv.set_property("vid", "auto"));
                        if result.is_err() {
                            let _ = events
                                .send(Notice::Error("Could not restore the video output.".into()));
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => (),
                }
                for _ in 0..128 {
                    let Some(event) = mpv.wait_event(0.0) else {
                        break;
                    };
                    let notice = match event {
                        Ok(Event::FileLoaded) => {
                            for sub in subtitles.drain(..) {
                                if url::Url::parse(&sub.url)
                                    .is_ok_and(|u| matches!(u.scheme(), "http" | "https"))
                                {
                                    let _ = mpv.command(
                                        "sub-add",
                                        &[
                                            &sub.url,
                                            "auto",
                                            &crate::app::playback::languages::name(&sub.lang),
                                            &sub.lang,
                                        ],
                                    );
                                }
                            }
                            tracks_due = true;
                            Some(Notice::Loaded)
                        }
                        Ok(Event::PropertyChange {
                            name: "track-list/count",
                            ..
                        }) => {
                            tracks_due = true;
                            None
                        }
                        Ok(Event::PropertyChange {
                            name,
                            change: PropertyData::Double(v),
                            ..
                        }) if v.is_finite() => Some(Notice::Number(name.into(), v)),
                        Ok(Event::PropertyChange {
                            name,
                            change: PropertyData::Flag(v),
                            ..
                        }) => Some(Notice::Flag(name.into(), v)),
                        Ok(Event::EndFile(reason))
                            if reason == libmpv2::mpv_end_file_reason::Eof =>
                        {
                            Some(Notice::Ended(true))
                        }
                        Ok(Event::EndFile(reason))
                            if reason == libmpv2::mpv_end_file_reason::Error =>
                        {
                            Some(Notice::Error(
                                "This source could not be played. Try another source.".into(),
                            ))
                        }
                        _ => None,
                    };
                    if let Some(notice) = notice {
                        let _ = events.send(notice);
                    }
                }
                if tracks_due && last_tracks.elapsed() > Duration::from_millis(300) {
                    last_tracks = Instant::now();
                    tracks_due = false;
                    let tracks: Vec<Track> = (0..mpv
                        .get_property::<i64>("track-list/count")
                        .unwrap_or(0)
                        .clamp(0, 256))
                        .filter_map(|i| {
                            let prefix = format!("track-list/{i}/");
                            let kind = mpv.get_property::<String>(&format!("{prefix}type")).ok()?;
                            if kind != "audio" && kind != "sub" {
                                return None;
                            }
                            let id = mpv.get_property::<i64>(&format!("{prefix}id")).ok()?;
                            let title = mpv
                                .get_property::<String>(&format!("{prefix}title"))
                                .unwrap_or_default();
                            let lang = mpv
                                .get_property::<String>(&format!("{prefix}lang"))
                                .unwrap_or_default();
                            let codec = mpv
                                .get_property::<String>(&format!("{prefix}codec"))
                                .unwrap_or_default();
                            let selected = mpv
                                .get_property::<bool>(&format!("{prefix}selected"))
                                .unwrap_or(false);
                            let flag = |name: &str| {
                                mpv.get_property::<bool>(&format!("{prefix}{name}"))
                                    .unwrap_or(false)
                            };
                            let normalized = title.to_ascii_lowercase();
                            let words: Vec<_> =
                                normalized.split(|c: char| !c.is_alphanumeric()).collect();
                            let hearing_impaired = flag("hearing-impaired")
                                || words.contains(&"sdh")
                                || words.contains(&"cc");
                            let forced = flag("forced") || words.contains(&"forced");
                            let visual_impaired = flag("visual-impaired")
                                || normalized.contains("audio description")
                                || normalized.contains("descriptive audio");
                            let commentary = words.contains(&"commentary");
                            let mut display = track_label(&kind, id, &title, &lang, &codec);
                            for (enabled, badge) in [
                                (hearing_impaired, "SDH"),
                                (forced, "Forced"),
                                (visual_impaired, "Audio description"),
                                (commentary, "Commentary"),
                            ] {
                                if enabled {
                                    display.push_str(&format!(" · {badge}"));
                                }
                            }
                            Some(Track {
                                id,
                                kind: kind.clone(),
                                title: display,
                                language: lang,
                                hearing_impaired,
                                forced,
                                visual_impaired,
                                commentary,
                                selected,
                            })
                        })
                        .collect();
                    for (kind, property, automatic) in [
                        ("audio", "aid", auto_audio),
                        (
                            "sub",
                            "sid",
                            auto_subtitles && preferences.subtitles_enabled,
                        ),
                    ] {
                        if automatic
                            && let Some(id) =
                                madari_core::preferred_track(&tracks, &preferences, kind)
                            && !tracks
                                .iter()
                                .any(|t| t.kind == kind && t.id == id && t.selected)
                            && mpv.set_property(property, id).is_ok()
                        {
                            tracks_due = true;
                        }
                    }
                    let _ = events.send(Notice::Tracks(tracks));
                    let chapters = (0..mpv
                        .get_property::<i64>("chapter-list/count")
                        .unwrap_or(0)
                        .clamp(0, 500))
                        .map(|i| Chapter {
                            index: i,
                            start_ms: mpv
                                .get_property::<f64>(&format!("chapter-list/{i}/time"))
                                .ok()
                                .filter(|time| time.is_finite() && *time >= 0.0)
                                .map(|time| (time * 1000.0) as u64),
                            title: mpv
                                .get_property::<String>(&format!("chapter-list/{i}/title"))
                                .ok()
                                .filter(|v| !v.is_empty())
                                .unwrap_or_else(|| format!("Chapter {}", i + 1)),
                        })
                        .collect();
                    let _ = events.send(Notice::Chapters(chapters));
                }
            }
        })
        .expect("create player command worker");
    (Engine(send, cancel), notices)
}
fn track_label(kind: &str, id: i64, title: &str, lang: &str, codec: &str) -> String {
    let language = crate::app::playback::languages::name(lang);
    let title = if (title.eq_ignore_ascii_case(lang) || title.eq_ignore_ascii_case(&language))
        && !lang.is_empty()
    {
        ""
    } else {
        title
    };
    let parts: Vec<_> = [title, language.as_str(), codec]
        .into_iter()
        .filter(|s| !s.trim().is_empty())
        .collect();
    if parts.is_empty() {
        format!("{} {id}", if kind == "sub" { "Subtitle" } else { "Audio" })
    } else {
        parts.join(" · ")
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn track_labels_keep_language_and_have_fallbacks() {
        assert_eq!(
            super::track_label("audio", 2, "Director’s commentary", "eng", "aac"),
            "Director’s commentary · English · aac"
        );
        assert_eq!(super::track_label("sub", 7, "", "", ""), "Subtitle 7");
        assert_eq!(super::track_label("sub", 1, "EN", "en", ""), "English");
        assert_eq!(
            super::track_label("sub", 1, "English", "eng", ""),
            "English"
        );
    }
}

#[cfg(test)]
pub(crate) fn video_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("captions.srt");
    std::fs::write(
        &sub,
        "1\n00:00:00,000 --> 00:00:10,000\nReal subtitle & audio track test\n",
    )
    .unwrap();
    let video = dir.path().join("tracks.mkv");
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=160x90:rate=20:duration=12",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=12",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:duration=12",
            "-i",
        ])
        .arg(&sub)
        .args([
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-map",
            "2:a",
            "-map",
            "3:s",
            "-map",
            "3:s",
            "-metadata:s:a:0",
            "language=eng",
            "-metadata:s:a:1",
            "language=fra",
            "-metadata:s:s:0",
            "language=eng",
            "-metadata:s:s:1",
            "language=fra",
            "-c:v",
            "libx264",
            "-threads",
            "1",
            "-preset",
            "ultrafast",
            "-c:a",
            "aac",
            "-c:s",
            "srt",
            "-y",
        ])
        .arg(&video)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    (dir, video)
}
#[cfg(test)]
mod playback_tests {
    use super::*;
    fn wait(events: &mpsc::Receiver<Notice>, mut matches: impl FnMut(&Notice) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            let notice = events.recv_timeout(Duration::from_millis(500));
            if let Ok(notice) = notice {
                if let Notice::Error(e) | Notice::Warning(e) = &notice {
                    panic!("{e}");
                }
                if matches(&notice) {
                    return;
                }
            }
        }
        panic!("Timed out waiting for the real player to confirm the action");
    }
    #[test]
    fn real_video_supports_tracks_seek_speed_and_subtitle_controls() {
        let (dir, video) = video_fixture();
        let mpv = Arc::new(
            Mpv::with_initializer(|i| {
                i.set_option("vo", "null")?;
                i.set_option("ao", "null")?;
                i.set_option("terminal", "no")?;
                Ok(())
            })
            .unwrap(),
        );
        let (engine, events) = start(mpv);
        engine
            .0
            .send(Command::Start(Box::new(Target {
                title: "Test metadata title".into(),
                context: crate::app::playback::PlaybackContext {
                    preferences: madari_model::PlaybackPreferences {
                        audio_description: madari_model::TrackPreference::Avoid,
                        subtitle_forced: madari_model::TrackPreference::Prefer,
                        audio_languages: vec!["de".into(), "fr".into(), "en".into()],
                        subtitle_languages: vec!["de".into(), "en".into()],
                        subtitles_enabled: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                url: video.to_string_lossy().into(),
                headers: Default::default(),
                resume_ms: 1000,
                offset_ms: 0,
                subtitles: vec![],
            })))
            .unwrap();
        let mut audio = 0;
        let mut subtitle = 0;
        wait(&events, |n| {
            if let Notice::Tracks(tracks) = n {
                let audios: Vec<_> = tracks.iter().filter(|t| t.kind == "audio").collect();
                let subs: Vec<_> = tracks.iter().filter(|t| t.kind == "sub").collect();
                if audios.len() == 2 && subs.len() == 2 {
                    assert!(
                        audios[1].selected,
                        "French should win when German is unavailable"
                    );
                    assert!(subs[0].selected, "English subtitles should be selected");
                    audio = audios[0].id;
                    subtitle = subs[1].id;
                    true
                } else {
                    false
                }
            } else {
                false
            }
        });
        engine.set("pause", "yes");
        wait(
            &events,
            |n| matches!(n,Notice::Flag(name,true) if name=="pause"),
        );
        engine.set("speed", "1.5");
        wait(
            &events,
            |n| matches!(n,Notice::Number(name,v) if name=="speed" && (*v-1.5).abs()<0.01),
        );
        engine.set("aid", audio);
        wait(
            &events,
            |n| matches!(n,Notice::Tracks(t) if t.iter().any(|t|t.kind=="audio" && t.id==audio && t.selected)),
        );
        engine.set("sid", subtitle);
        wait(
            &events,
            |n| matches!(n,Notice::Tracks(t) if t.iter().any(|t|t.kind=="sub" && t.id==subtitle && t.selected)),
        );
        engine.set("sub-scale", "1.3");
        wait(
            &events,
            |n| matches!(n,Notice::Number(name,v) if name=="sub-scale" && (*v-1.3).abs()<0.01),
        );
        engine.set("audio-delay", "0.25");
        wait(
            &events,
            |n| matches!(n,Notice::Number(name,v) if name=="audio-delay" && (*v-0.25).abs()<0.01),
        );
        engine.command(&["seek", "3", "absolute+exact"]);
        wait(
            &events,
            |n| matches!(n,Notice::Number(name,v) if name=="time-pos" && *v>=2.9),
        );
        engine.command(&[
            "sub-add",
            dir.path().join("captions.srt").to_str().unwrap(),
            "select",
        ]);
        wait(
            &events,
            |n| matches!(n,Notice::Tracks(t) if t.iter().filter(|t|t.kind=="sub").count()==3),
        );
        engine.0.send(Command::Shutdown).unwrap();
    }
}
