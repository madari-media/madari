//! Conservative, local hints for an optional next-episode action.
use crate::app::playback::engine::Chapter;

/// Only skip explicitly marked opening sequences with a known following chapter.
pub fn intro_end(position: u64, chapters: &[Chapter], offset: u64) -> Option<u64> {
    let (chapter, start) = chapters
        .iter()
        .filter_map(|chapter| {
            let start = chapter.start_ms?.checked_add(offset)?;
            (start <= position).then_some((chapter, start))
        })
        .max_by_key(|(_, start)| *start)?;
    if !matches!(
        chapter.title.trim().to_lowercase().as_str(),
        "intro"
            | "intro credits"
            | "intro sequence"
            | "opening"
            | "opening credits"
            | "opening theme"
            | "op"
    ) {
        return None;
    }
    let end = chapters
        .iter()
        .filter_map(|chapter| chapter.start_ms?.checked_add(offset))
        .filter(|end| *end > start)
        .min()?;
    (start < 10 * 60_000 && end - start <= 5 * 60_000 && position < end).then_some(end)
}

pub fn suggest(position: u64, duration: u64, chapters: &[Chapter], offset: u64) -> bool {
    if duration == 0 || position >= duration {
        return false;
    }
    if u128::from(position) * 100 >= u128::from(duration) * 95 {
        return true;
    }
    let credit_start = chapters
        .iter()
        .filter_map(|chapter| {
            let title = chapter.title.to_lowercase();
            let words: Vec<_> = title.split(|c: char| !c.is_alphanumeric()).collect();
            let credits = words.contains(&"credits")
                && !words
                    .iter()
                    .any(|word| matches!(*word, "opening" | "intro" | "post" | "after"));
            if !credits {
                return None;
            }
            chapter
                .start_ms
                .and_then(|start| start.checked_add(offset))
                .filter(|start| {
                    u128::from(*start) * 100 >= u128::from(duration) * 75 && *start < duration
                })
        })
        .min();
    credit_start.is_some_and(|start| position >= start)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter(title: &str, start: u64) -> Chapter {
        Chapter {
            index: 0,
            title: title.into(),
            start_ms: Some(start),
        }
    }

    #[test]
    fn intro_requires_explicit_marker_and_known_end() {
        let chapters = [
            chapter("Cold open", 0),
            chapter("Opening credits", 60_000),
            chapter("Episode", 150_000),
        ];
        assert_eq!(intro_end(59_999, &chapters, 0), None);
        assert_eq!(intro_end(60_000, &chapters, 0), Some(150_000));
        assert_eq!(intro_end(149_999, &chapters, 0), Some(150_000));
        assert_eq!(intro_end(150_000, &chapters, 0), None);
        assert_eq!(intro_end(90_000, &chapters, 30_000), Some(180_000));
        assert_eq!(intro_end(60_000, &[chapter("Intro", 0)], 0), None);
        assert_eq!(
            intro_end(
                60_000,
                &[chapter("Chapter 1", 0), chapter("Episode", 90_000)],
                0
            ),
            None
        );
        assert_eq!(
            intro_end(
                60_000,
                &[chapter("Intro", 0), chapter("Episode", 600_000)],
                0
            ),
            None
        );
    }

    #[test]
    fn credits_trigger_at_marker_and_respect_transcode_offset() {
        let chapters = [chapter("End Credits", 1_000_000)];
        assert!(!suggest(999_999, 1_200_000, &chapters, 0));
        assert!(suggest(1_000_000, 1_200_000, &chapters, 0));
        assert!(!suggest(1_099_999, 1_200_000, &chapters, 100_000));
        assert!(suggest(1_100_000, 1_200_000, &chapters, 100_000));
    }

    #[test]
    fn progress_triggers_at_95_percent_and_ignores_opening_and_post_credits() {
        for title in [
            "Opening credits",
            "Post-credits scene",
            "After credits",
            "Chapter 8",
        ] {
            assert!(!suggest(
                1_000_000,
                1_200_000,
                &[chapter(title, 1_000_000)],
                0
            ));
        }
        assert!(!suggest(1_139_999, 1_200_000, &[], 0));
        assert!(suggest(1_140_000, 1_200_000, &[], 0));
        assert!(!suggest(3_419_999, 3_600_000, &[], 0));
        assert!(suggest(3_420_000, 3_600_000, &[], 0));
        assert!(!suggest(94_999, 100_000, &[], 0));
        assert!(suggest(95_000, 100_000, &[], 0));
        let late_credits = [chapter("End credits", 1_180_000)];
        assert!(!suggest(1_139_999, 1_200_000, &late_credits, 0));
        assert!(suggest(1_140_000, 1_200_000, &late_credits, 0));
        assert!(!suggest(0, 0, &[], 0));
        assert!(!suggest(1_200_000, 1_200_000, &[], 0));
    }
}
