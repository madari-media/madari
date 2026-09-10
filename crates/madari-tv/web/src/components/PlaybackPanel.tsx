import {
  AdjustmentsHorizontalIcon,
  ChatBubbleBottomCenterTextIcon,
  LanguageIcon,
  SpeakerWaveIcon,
} from '@heroicons/react/24/outline';
import {SegmentedControl, SegmentedControlItem} from '@astryxdesign/core/SegmentedControl';
import {Switch} from '@astryxdesign/core/Switch';
import {VStack} from '@astryxdesign/core/VStack';
import type {TrackPreference} from '../client';
import type {Preferences} from '../preferences';
import {TRACK_PREFERENCES} from '../languages';
import {LanguageEditor} from './LanguageEditor';
import {SettingsCard, SettingsRow} from './settings';

export function PlaybackPanel({
  preferences,
  busy,
  onSave,
}: {
  preferences: Preferences;
  busy: boolean;
  onSave: (next: Preferences) => void;
}) {
  return (
    <VStack gap={5}>
      <SettingsCard title="Subtitles">
        <SettingsRow
          title="Enable subtitles by default"
          description="Turn subtitles on whenever a video starts with a matching track."
          icon={ChatBubbleBottomCenterTextIcon}
          control={
            <Switch
              label="Enable subtitles by default"
              isLabelHidden
              value={preferences.subtitles_enabled}
              onChange={(value) => onSave({...preferences, subtitles_enabled: value})}
            />
          }
        />
      </SettingsCard>

      <SettingsCard title="Track preferences">
        {TRACK_PREFERENCES.map(({key, label}) => (
          <SettingsRow
            key={key}
            title={label}
            icon={AdjustmentsHorizontalIcon}
            control={
              <SegmentedControl
                label={label}
                size="sm"
                value={preferences[key]}
                isDisabled={busy}
                onChange={(value) => onSave({...preferences, [key]: value as TrackPreference})}
              >
                <SegmentedControlItem value="any" label="No preference" />
                <SegmentedControlItem value="prefer" label="Prefer" />
                <SegmentedControlItem value="avoid" label="Avoid" />
              </SegmentedControl>
            }
          />
        ))}
      </SettingsCard>

      <SettingsCard title="Languages">
        <LanguageEditor
          label="Audio languages"
          description="The first language available in a video is picked automatically."
          icon={SpeakerWaveIcon}
          value={preferences.audio_languages}
          busy={busy}
          onChange={(languages) => onSave({...preferences, audio_languages: languages})}
        />
        <LanguageEditor
          label="Subtitle languages"
          description="Used when subtitles are on and the video offers several tracks."
          icon={LanguageIcon}
          value={preferences.subtitle_languages}
          busy={busy}
          onChange={(languages) => onSave({...preferences, subtitle_languages: languages})}
        />
      </SettingsCard>
    </VStack>
  );
}
