import type {ReactNode} from 'react';
import {createContext, useCallback, useContext, useEffect, useMemo, useRef, useState} from 'react';
import {Card} from '@astryxdesign/core/Card';
import {HStack} from '@astryxdesign/core/HStack';
import {Icon} from '@astryxdesign/core/Icon';
import {IconButton} from '@astryxdesign/core/IconButton';
import {StatusDot} from '@astryxdesign/core/StatusDot';
import {Text} from '@astryxdesign/core/Text';
import {VStack} from '@astryxdesign/core/VStack';
import * as stylex from '@stylexjs/stylex';
import {
  ArrowsPointingOutIcon,
  ArrowDownIcon,
  ArrowLeftIcon,
  ArrowRightIcon,
  ArrowUpIcon,
  ArrowUturnLeftIcon,
  BackwardIcon,
  Bars3Icon,
  ChevronDoubleLeftIcon,
  ChevronDoubleRightIcon,
  ForwardIcon,
  PlayIcon,
  SpeakerWaveIcon,
  SpeakerXMarkIcon,
} from '@heroicons/react/24/outline';
import {usePlayerSocket, type PlayerState} from '../player';
import {NowPlayingBar} from './NowPlaying';

/** The keys the TV accepts. Kept in step with `REMOTE_COMMANDS` in `web.rs`. */
export type RemoteCommand =
  | 'up'
  | 'down'
  | 'left'
  | 'right'
  | 'select'
  | 'back'
  | 'play'
  | 'pause'
  | 'play_pause'
  | 'next'
  | 'previous'
  | 'seek_forward'
  | 'seek_back'
  | 'volume_up'
  | 'volume_down';

const PANEL_WIDTH = 336;
const VIEWPORT_GAP = 16;
/** Keep at least this much of the panel on screen when it is dragged down. */
const KEEP_VISIBLE = 148;

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), Math.max(min, max));
}

const styles = stylex.create({
  window: {
    position: 'fixed',
    zIndex: 40,
    width: PANEL_WIDTH,
  },
  // Only the header drags: the keys below must never be swallowed by a drag.
  handle: {
    cursor: 'grab',
    touchAction: 'none',
    userSelect: 'none',
    ':active': {cursor: 'grabbing'},
  },
  grip: {
    color: 'var(--color-text-secondary)',
  },
  // The panel grows with its tab, but never past the viewport.
  body: {
    maxHeight: 'min(560px, calc(100dvh - 260px))',
  },
  // A ring rather than a grid of buttons: the arrows sit in the edge cells and
  // the middle cell is the select key, which is what makes it read as a D-pad.
  pad: {
    display: 'grid',
    gridTemplateColumns: 'repeat(3, 1fr)',
    gridTemplateRows: 'repeat(3, 1fr)',
    placeItems: 'center',
    width: 190,
    height: 190,
    borderRadius: '50%',
    borderWidth: 1,
    borderStyle: 'solid',
    borderColor: 'var(--color-border)',
    backgroundColor: 'var(--color-background-muted)',
    boxShadow: 'var(--shadow-low)',
  },
  up: {gridArea: '1 / 2'},
  left: {gridArea: '2 / 1'},
  right: {gridArea: '2 / 3'},
  down: {gridArea: '3 / 2'},
  select: {
    gridArea: '2 / 2',
    width: 62,
    height: 62,
    borderRadius: '50%',
  },
  arrow: {
    width: 50,
    height: 50,
    borderRadius: '50%',
    color: 'var(--color-text-primary)',
    backgroundColor: {
      default: 'transparent',
      ':hover': 'var(--color-background-surface)',
      ':active': 'var(--color-accent-muted)',
    },
  },
  // One pill per cluster, so transport and volume read as groups rather than as
  // a row of loose buttons.
  pill: {
    display: 'flex',
    alignItems: 'center',
    gap: 'var(--spacing-1)',
    padding: 'var(--spacing-1)',
    borderRadius: 'var(--radius-full)',
    borderWidth: 1,
    borderStyle: 'solid',
    borderColor: 'var(--color-border)',
    backgroundColor: 'var(--color-background-muted)',
  },
});

interface RemoteApi {
  isOpen: boolean;
  toggle: () => void;
  close: () => void;
}
interface PlayerApi {
  state: PlayerState | null;
  connected: boolean;
  command: (action: string, extra?: Record<string, unknown>) => boolean;
}

const RemoteContext = createContext<RemoteApi | null>(null);
const PlayerContext = createContext<PlayerApi | null>(null);

/**
 * Two contexts on purpose: the player streams state twice a second, and only the
 * panel that shows it should re-render that often — not the page behind it.
 */
export function useRemote(): RemoteApi {
  const value = useContext(RemoteContext);
  if (!value) throw new Error('useRemote must be used inside RemoteProvider');
  return value;
}
export function usePlayer(): PlayerApi {
  const value = useContext(PlayerContext);
  if (!value) throw new Error('usePlayer must be used inside RemoteProvider');
  return value;
}

/** Owns the remote window and the socket the TV streams its player over. */
export function RemoteProvider({
  disabled,
  onCommand,
  children,
}: {
  disabled: boolean;
  onCommand: (command: RemoteCommand) => void;
  children: ReactNode;
}) {
  const [isOpen, setIsOpen] = useState(false);
  const handler = useRef(onCommand);
  handler.current = onCommand;
  const {state, connected, command} = usePlayerSocket(true);

  const close = useCallback(() => setIsOpen(false), []);
  const toggle = useCallback(() => setIsOpen((value) => !value), []);
  const remote = useMemo(() => ({isOpen, toggle, close}), [isOpen, toggle, close]);
  const player = useMemo(() => ({state, connected, command}), [state, connected, command]);
  const send = useCallback((key: RemoteCommand) => handler.current(key), []);

  return (
    <RemoteContext.Provider value={remote}>
      <PlayerContext.Provider value={player}>
        {children}
        {isOpen ? <RemoteWindow disabled={disabled} onCommand={send} onClose={close} /> : null}
        {/* The player bar is docked to the viewport, so the page needs room under it. */}
        {player.state?.active ? <VStack aria-hidden="true" height={104} /> : null}
        <NowPlayingBar />
      </PlayerContext.Provider>
    </RemoteContext.Provider>
  );
}

/** One key. `label` is both the tooltip and the accessible name. */
function key(
  label: string,
  command: RemoteCommand,
  icon: ReactNode,
  onCommand: (command: RemoteCommand) => void,
  disabled: boolean,
  variant: 'primary' | 'secondary' | 'ghost' = 'ghost',
  xstyle?: stylex.StyleXStyles,
) {
  return (
    <IconButton
      label={label}
      icon={icon}
      variant={variant}
      size="lg"
      tooltip={label}
      isDisabled={disabled}
      xstyle={xstyle}
      onClick={() => onCommand(command)}
    />
  );
}

/**
 * The remote itself: a directional ring, a transport cluster and volume.
 *
 * Keys are queued by the server and picked up by the TV's poll loop, so this is
 * deliberately fire-and-forget — nothing here waits on a round trip.
 */
function RemotePad({
  disabled,
  onCommand,
}: {
  disabled: boolean;
  onCommand: (command: RemoteCommand) => void;
}) {
  return (
    <VStack gap={4} hAlign="center">
      <VStack gap={0} xstyle={styles.pad}>
        {key('Up', 'up', <Icon icon={ArrowUpIcon} size="sm" />, onCommand, disabled, 'ghost', styles.up)}
        {key('Left', 'left', <Icon icon={ArrowLeftIcon} size="sm" />, onCommand, disabled, 'ghost', styles.left)}
        <IconButton
          label="Select"
          icon={<Icon icon="check" size="md" />}
          variant="primary"
          tooltip="Select"
          isDisabled={disabled}
          xstyle={styles.select}
          onClick={() => onCommand('select')}
        />
        {key('Right', 'right', <Icon icon={ArrowRightIcon} size="sm" />, onCommand, disabled, 'ghost', styles.right)}
        {key('Down', 'down', <Icon icon={ArrowDownIcon} size="sm" />, onCommand, disabled, 'ghost', styles.down)}
      </VStack>

      <HStack xstyle={styles.pill}>
        {key('Previous', 'previous', <Icon icon={ChevronDoubleLeftIcon} size="sm" />, onCommand, disabled)}
        {key('Rewind', 'seek_back', <Icon icon={BackwardIcon} size="sm" />, onCommand, disabled)}
        {key('Play or pause', 'play_pause', <Icon icon={PlayIcon} size="md" />, onCommand, disabled, 'primary', styles.select)}
        {key('Fast forward', 'seek_forward', <Icon icon={ForwardIcon} size="sm" />, onCommand, disabled)}
        {key('Next', 'next', <Icon icon={ChevronDoubleRightIcon} size="sm" />, onCommand, disabled)}
      </HStack>

      <HStack gap={2} align="center" justify="center">
        {key('Back', 'back', <Icon icon={ArrowUturnLeftIcon} size="sm" />, onCommand, disabled, 'secondary')}
        <HStack xstyle={styles.pill}>
          {key('Volume down', 'volume_down', <Icon icon={SpeakerXMarkIcon} size="sm" />, onCommand, disabled)}
          {key('Volume up', 'volume_up', <Icon icon={SpeakerWaveIcon} size="sm" />, onCommand, disabled)}
        </HStack>
      </HStack>

      <Text type="supporting" color="secondary">
        Arrows, Enter and Backspace work too.
      </Text>
    </VStack>
  );
}

/**
 * The floating panel: a fixed-width window the user drags by its header. It
 * opens against the right edge, clear of the settings column, and holds both the
 * remote and the TV's live player.
 */
function RemoteWindow({
  disabled,
  onCommand,
  onClose,
}: {
  disabled: boolean;
  onCommand: (command: RemoteCommand) => void;
  onClose: () => void;
}) {
  const [position, setPosition] = useState<{x: number; y: number} | null>(null);
  const drag = useRef<{id: number; dx: number; dy: number} | null>(null);

  // Park it against the right edge, a little above centre, until it is dragged.
  useEffect(() => {
    setPosition({
      x: Math.max(VIEWPORT_GAP, window.innerWidth - PANEL_WIDTH - VIEWPORT_GAP),
      y: Math.max(VIEWPORT_GAP, Math.round(window.innerHeight / 2) - 300),
    });
  }, []);

  // Keep it reachable when the window shrinks underneath it.
  useEffect(() => {
    function onResize() {
      setPosition((current) =>
        current
          ? {
              x: clamp(current.x, VIEWPORT_GAP, window.innerWidth - PANEL_WIDTH - VIEWPORT_GAP),
              y: clamp(current.y, VIEWPORT_GAP, window.innerHeight - KEEP_VISIBLE),
            }
          : current,
      );
    }
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);

  // Keyboard driving, but only while focus is not in a field on the page.
  useEffect(() => {
    const bindings: Record<string, RemoteCommand> = {
      ArrowUp: 'up',
      ArrowDown: 'down',
      ArrowLeft: 'left',
      ArrowRight: 'right',
      Enter: 'select',
      ' ': 'play_pause',
      Backspace: 'back',
      PageUp: 'next',
      PageDown: 'previous',
    };
    function onKeyDown(event: KeyboardEvent) {
      const target = event.target as HTMLElement | null;
      if (target && (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName))) {
        return;
      }
      const command = bindings[event.key];
      if (!command) return;
      event.preventDefault();
      onCommand(command);
    }
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [onCommand]);

  function startDrag(event: React.PointerEvent<HTMLElement>) {
    if (!position) return;
    drag.current = {id: event.pointerId, dx: event.clientX - position.x, dy: event.clientY - position.y};
    event.currentTarget.setPointerCapture(event.pointerId);
  }

  function moveDrag(event: React.PointerEvent<HTMLElement>) {
    const state = drag.current;
    if (!state || state.id !== event.pointerId) return;
    setPosition({
      x: clamp(event.clientX - state.dx, VIEWPORT_GAP, window.innerWidth - PANEL_WIDTH - VIEWPORT_GAP),
      y: clamp(event.clientY - state.dy, VIEWPORT_GAP, window.innerHeight - KEEP_VISIBLE),
    });
  }

  function endDrag(event: React.PointerEvent<HTMLElement>) {
    const state = drag.current;
    if (!state || state.id !== event.pointerId) return;
    drag.current = null;
    event.currentTarget.releasePointerCapture(event.pointerId);
  }

  if (!position) return null;

  return (
    <Card
      padding={4}
      variant="default"
      elevation="high"
      xstyle={styles.window}
      style={{left: position.x, top: position.y}}
    >
      <VStack gap={3}>
        <HStack
          gap={2}
          align="center"
          hAlign="between"
          xstyle={styles.handle}
          onPointerDown={startDrag}
          onPointerMove={moveDrag}
          onPointerUp={endDrag}
          onPointerCancel={endDrag}
        >
          <HStack gap={2} align="center">
            <Icon icon={Bars3Icon} size="sm" xstyle={styles.grip} />
            <StatusDot
              variant={disabled ? 'neutral' : 'success'}
              label={disabled ? 'Remote unavailable' : 'Remote ready'}
            />
            <Text type="label">Madari TV</Text>
          </HStack>
          <HStack gap={1} align="center">
            <IconButton
              label="Reset the panel position"
              icon={<Icon icon={ArrowsPointingOutIcon} size="sm" />}
              variant="ghost"
              size="sm"
              tooltip="Reset position"
              onClick={() =>
                setPosition({
                  x: Math.max(VIEWPORT_GAP, window.innerWidth - PANEL_WIDTH - VIEWPORT_GAP),
                  y: Math.max(VIEWPORT_GAP, Math.round(window.innerHeight / 2) - 300),
                })
              }
            />
            <IconButton
              label="Close the panel"
              icon={<Icon icon="close" size="sm" />}
              variant="ghost"
              size="sm"
              tooltip="Close"
              onClick={onClose}
            />
          </HStack>
        </HStack>

        <VStack gap={4} isScrollable xstyle={styles.body}>
          <RemotePad disabled={disabled} onCommand={onCommand} />
        </VStack>
      </VStack>
    </Card>
  );
}
