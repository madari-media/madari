import type {PlaybackPreferences, TrackPreference} from './client';

/** Fully-populated view of the generated (all-optional, defaulted) preferences. */
export interface Preferences {
  subtitle_sdh: TrackPreference;
  subtitle_forced: TrackPreference;
  audio_description: TrackPreference;
  audio_commentary: TrackPreference;
  audio_languages: string[];
  subtitle_languages: string[];
  subtitles_enabled: boolean;
}

/** The API defaults every field, so fill in the documented defaults for the form. */
export function normalizePreferences(value: PlaybackPreferences | null | undefined): Preferences {
  return {
    subtitle_sdh: value?.subtitle_sdh ?? 'any',
    subtitle_forced: value?.subtitle_forced ?? 'any',
    audio_description: value?.audio_description ?? 'any',
    audio_commentary: value?.audio_commentary ?? 'any',
    audio_languages: value?.audio_languages ?? [],
    subtitle_languages: value?.subtitle_languages ?? [],
    subtitles_enabled: value?.subtitles_enabled ?? true,
  };
}
