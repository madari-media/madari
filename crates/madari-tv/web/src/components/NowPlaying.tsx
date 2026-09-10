import type {ReactNode} from 'react';
import {useEffect, useState} from 'react';
import {Banner} from '@astryxdesign/core/Banner';
import {Button} from '@astryxdesign/core/Button';
import {Card} from '@astryxdesign/core/Card';
import {HStack} from '@astryxdesign/core/HStack';
import {Icon} from '@astryxdesign/core/Icon';
import {IconButton} from '@astryxdesign/core/IconButton';
import {Popover} from '@astryxdesign/core/Popover';
import {ProgressBar} from '@astryxdesign/core/ProgressBar';
import {SegmentedControl, SegmentedControlItem} from '@astryxdesign/core/SegmentedControl';
import {Selector} from '@astryxdesign/core/Selector';
import {Slider} from '@astryxdesign/core/Slider';
import {Spinner} from '@astryxdesign/core/Spinner';
import {StackItem} from '@astryxdesign/core/Stack';
import {StatusDot} from '@astryxdesign/core/StatusDot';
import {Text} from '@astryxdesign/core/Text';
import {Thumbnail} from '@astryxdesign/core/Thumbnail';
import {VStack} from '@astryxdesign/core/VStack';
import * as stylex from '@stylexjs/stylex';
import {
  ArrowDownTrayIcon,
  ArrowUpTrayIcon,
  AdjustmentsHorizontalIcon,
  BackwardIcon,
  ChevronDoubleLeftIcon,
  ChevronDoubleRightIcon,
  ForwardIcon,
  PauseIcon,
  PlayIcon,
  StopIcon,
} from '@heroicons/react/24/outline';
import {usePlayer} from './Remote';

/** Position or duration as a clock. Hours only appear when they exist. */
function clock(ms: number | undefined): string {
  const total = Math.max(0, Math.floor((ms ?? 0) / 1000));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`
    : `${minutes}:${String(seconds).padStart(2, '0')}`;
}

function rate(bytes: number | undefined): string {
  const value = (bytes ?? 0) / 1024;
  return value >= 1024 ? `${(value / 1024).toFixed(1)} MB/s` : `${Math.round(value)} KB/s`;
}

/** The state arrives from the TV, so a wrong shape reads as empty, not fatal. */
function list<T>(value: T[] | undefined): T[] {
  return Array.isArray(value) ? value : [];
}

const styles = stylex.create({
  // Docked to the bottom edge like a media bar, above the page and below dialogs.
  bar: {
    position: 'fixed',
    insetInline: 0,
    insetBlockEnd: 0,
    zIndex: 35,
    // Full-bleed along the bottom edge, so the corners stay square.
    borderRadius: 0,
  },
  scroll: {
    maxHeight: 'min(420px, calc(100dvh - 200px))',
  },
  // A 2:3 poster, the shape the art is actually cut to.
  art: {
    width: 44,
    height: 66,
    flexShrink: 0,
    overflow: 'hidden',
    borderRadius: 'var(--radius-element)',
  },
  // One label column, so the controls stack into a readable column.
  row: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: 'var(--spacing-2)',
  },
});

function Row({label, children}: {label: string; children: ReactNode}) {
  return (
    <HStack gap={2} align="center" justify="between">
      <Text type="supporting" color="secondary">
        {label}
      </Text>
      {children}
    </HStack>
  );
}

const SPEEDS = [0.5, 0.75, 1, 1.25, 1.5, 2];
const RESIZE = ['Fit', 'Zoom', 'Stretch'];

/** Everything beyond transport: tracks, speed, picture and source. */
function PlayerSettings({
  state,
  command,
}: {
  state: NonNullable<ReturnType<typeof usePlayer>['state']>;
  command: (action: string, extra?: Record<string, unknown>) => boolean;
}) {
  const audio = list(state.tracks).filter((track) => track.kind === 'audio');
  const subtitles = list(state.tracks).filter((track) => track.kind === 'sub');
  const selectedAudio = audio.find((track) => track.selected);
  const selectedSubtitle = subtitles.find((track) => track.selected);
  const episodes = list(state.episodes);
  const sources = list(state.sources);
  const torrent = state.torrent;
  const percent =
    torrent && torrent.total > 0 ? Math.min(100, Math.round((torrent.downloaded * 100) / torrent.total)) : 0;

  return (
    <VStack gap={3} isScrollable xstyle={styles.scroll}>
      <Row label="Speed">
        <Selector
          label="Playback speed"
          isLabelHidden
          size="sm"
          width={132}
          value={String(state.speed ?? 1)}
          options={SPEEDS.map((value) => ({value: String(value), label: `${value}×`}))}
          onChange={(value) => command('speed', {value: Number(value)})}
        />
      </Row>

      <Row label="Audio">
        <Selector
          label="Audio track"
          isLabelHidden
          size="sm"
          width={184}
          value={selectedAudio ? String(selectedAudio.id) : 'auto'}
          options={[
            {value: 'auto', label: 'Automatic'},
            ...audio.map((track) => ({
              value: String(track.id),
              label:
                [track.language, track.label, track.channels > 0 ? `${track.channels} ch` : '']
                  .filter(Boolean)
                  .join(' · ') || `Track ${track.id}`,
            })),
          ]}
          onChange={(value) => command('track', {kind: 'audio', id: value === 'auto' ? null : Number(value)})}
        />
      </Row>

      <Row label="Subtitles">
        <Selector
          label="Subtitle track"
          isLabelHidden
          size="sm"
          width={184}
          value={selectedSubtitle ? String(selectedSubtitle.id) : 'off'}
          options={[
            {value: 'off', label: 'Off'},
            ...subtitles.map((track) => ({
              value: String(track.id),
              label: [track.language, track.label].filter(Boolean).join(' · ') || `Track ${track.id}`,
            })),
          ]}
          onChange={(value) => command('track', {kind: 'sub', id: value === 'off' ? null : Number(value)})}
        />
      </Row>

      <Row label="Subtitle size">
        <SegmentedControl
          label="Subtitle size"
          size="sm"
          value={state.subtitle_size ?? 'medium'}
          onChange={(value) => command('subtitle_size', {value})}
        >
          <SegmentedControlItem value="small" label="S" />
          <SegmentedControlItem value="medium" label="M" />
          <SegmentedControlItem value="large" label="L" />
        </SegmentedControl>
      </Row>

      <Row label="Picture">
        <SegmentedControl
          label="Picture size"
          size="sm"
          value={String(state.resize ?? 0)}
          onChange={(value) => command('resize', {value: Number(value)})}
        >
          {RESIZE.map((label, index) => (
            <SegmentedControlItem key={label} value={String(index)} label={label} />
          ))}
        </SegmentedControl>
      </Row>

      {episodes.length > 1 ? (
        <Row label="Episode">
          <Selector
            label="Episode"
            isLabelHidden
            size="sm"
            width={184}
            value={String(episodes.find((entry) => entry.current)?.id ?? 0)}
            options={episodes.map((entry) => ({value: String(entry.id), label: entry.label}))}
            onChange={(value) => command('episode', {id: Number(value)})}
          />
        </Row>
      ) : null}

      {sources.length > 1 ? (
        <Row label="Source">
          <Selector
            label="Source"
            isLabelHidden
            size="sm"
            width={184}
            value={String(sources.find((entry) => entry.current)?.id ?? 0)}
            options={sources.map((entry) => ({value: String(entry.id), label: entry.label}))}
            onChange={(value) => command('source', {id: Number(value)})}
          />
        </Row>
      ) : null}

      {torrent ? (
        <VStack gap={1}>
          <ProgressBar
            label="Torrent progress"
            value={percent}
            hasValueLabel
            formatValueLabel={() => `${percent}%`}
          />
          <HStack justify="between">
            <Text type="supporting" color="secondary">
              {torrent.state} · {torrent.peers} peers
            </Text>
            <HStack gap={2} align="center">
              <HStack gap={1} align="center">
                <Icon icon={ArrowDownTrayIcon} size="xsm" />
                <Text type="supporting" color="secondary">
                  {rate(torrent.download_speed)}
                </Text>
              </HStack>
              <HStack gap={1} align="center">
                <Icon icon={ArrowUpTrayIcon} size="xsm" />
                <Text type="supporting" color="secondary">
                  {rate(torrent.upload_speed)}
                </Text>
              </HStack>
            </HStack>
          </HStack>
        </VStack>
      ) : null}
    </VStack>
  );
}

/**
 * The bar along the bottom while the TV is playing: what is on, transport, and a
 * scrubber — with the rest of the player one tap away.
 */
export function NowPlayingBar() {
  const {state, connected, command} = usePlayer();
  // While the thumb is held, show the dragged position rather than the TV's.
  const [scrub, setScrub] = useState<number | null>(null);
  useEffect(() => {
    if (!state?.playing) setScrub(null);
  }, [state?.playing]);

  if (!state?.active) return null;

  const position = scrub ?? state.position_ms ?? 0;
  const duration = state.duration_ms ?? 0;
  const seekable = Boolean(state.seekable) && !state.live && duration > 0;
  const episodes = list(state.episodes);
  const currentEpisode = episodes.find((entry) => entry.current);
  const index = currentEpisode?.id ?? -1;

  return (
    <Card padding={0} elevation="high" xstyle={styles.bar}>
      <VStack gap={0}>
        {state.error ? <Banner status="error" title="Playback interrupted" description={state.error} /> : null}
        <HStack gap={4} align="center" padding={3}>
          {/* What is on. */}
          <HStack gap={3} align="center" width={250}>
            {state.poster ? <Thumbnail src={state.poster} alt="" xstyle={styles.art} /> : null}
            <VStack gap={0}>
              <Text type="label" maxLines={1}>
                {state.title}
              </Text>
              <Text type="supporting" color="secondary" maxLines={1}>
                {[state.episode, state.source].filter(Boolean).join(' · ')}
              </Text>
              <HStack gap={1} align="center">
                <StatusDot
                  variant={state.playing ? 'success' : 'neutral'}
                  label={state.playing ? 'Playing' : 'Paused'}
                />
                <Text type="supporting" color="secondary" maxLines={1}>
                  {state.playing ? 'Playing on the TV' : 'Paused on the TV'}
                </Text>
              </HStack>
            </VStack>
          </HStack>

          {/* Transport and the scrubber. */}
          <StackItem size="fill">
            <HStack gap={3} align="center">
              <HStack gap={1} align="center">
                <IconButton
                  label="Previous episode"
                  icon={<Icon icon={ChevronDoubleLeftIcon} size="sm" />}
                  variant="ghost"
                  size="sm"
                  tooltip="Previous episode"
                  isDisabled={index <= 0}
                  onClick={() => command('episode', {id: index - 1})}
                />
                <IconButton
                  label="Back 10 seconds"
                  icon={<Icon icon={BackwardIcon} size="sm" />}
                  variant="ghost"
                  size="sm"
                  tooltip="Back 10 seconds"
                  isDisabled={!seekable}
                  onClick={() => command('seek_by', {offset_ms: -10_000})}
                />
                <IconButton
                  label={state.playing ? 'Pause' : 'Play'}
                  icon={<Icon icon={state.playing ? PauseIcon : PlayIcon} size="md" />}
                  variant="primary"
                  size="md"
                  tooltip={state.playing ? 'Pause' : 'Play'}
                  onClick={() => command('play_pause')}
                />
                <IconButton
                  label="Forward 10 seconds"
                  icon={<Icon icon={ForwardIcon} size="sm" />}
                  variant="ghost"
                  size="sm"
                  tooltip="Forward 10 seconds"
                  isDisabled={!seekable}
                  onClick={() => command('seek_by', {offset_ms: 10_000})}
                />
                <IconButton
                  label="Next episode"
                  icon={<Icon icon={ChevronDoubleRightIcon} size="sm" />}
                  variant="ghost"
                  size="sm"
                  tooltip="Next episode"
                  isDisabled={index < 0 || index >= episodes.length - 1}
                  onClick={() => command('episode', {id: index + 1})}
                />
              </HStack>

              <StackItem size="fill">
                <HStack gap={2} align="center">
                  <Text type="supporting" color="secondary">
                    {clock(position)}
                  </Text>
                  <StackItem size="fill">
                    <Slider
                      label="Position"
                      isLabelHidden
                      min={0}
                      max={Math.max(duration, 1)}
                      step={1000}
                      value={position}
                      valueDisplay="none"
                      isDisabled={!seekable}
                      formatValue={clock}
                      onChange={(value: number) => setScrub(value)}
                      onChangeEnd={(value: number) => {
                        command('seek', {position_ms: value});
                        setScrub(null);
                      }}
                    />
                  </StackItem>
                  <Text type="supporting" color="secondary">
                    {state.live ? 'Live' : clock(duration)}
                  </Text>
                </HStack>
              </StackItem>
            </HStack>
          </StackItem>

          {/* Everything else, and the way out. */}
          <HStack gap={1} align="center">
            {state.buffering ? <Spinner size="sm" /> : null}
            <Popover
              placement="above"
              alignment="end"
              width={340}
              label="Player controls"
              content={
                <Card padding={4} elevation="med" width="100%">
                  <PlayerSettings state={state} command={command} />
                </Card>
              }
            >
              <Button
                label="Player controls"
                variant="secondary"
                size="sm"
                icon={<Icon icon={AdjustmentsHorizontalIcon} size="sm" />}
                tooltip="Tracks, speed and picture"
                isDisabled={!connected}
              />
            </Popover>
            <IconButton
              label="Stop and close the player"
              icon={<Icon icon={StopIcon} size="sm" />}
              variant="ghost"
              size="sm"
              tooltip="Stop"
              onClick={() => command('stop')}
            />
          </HStack>
        </HStack>
      </VStack>
    </Card>
  );
}
