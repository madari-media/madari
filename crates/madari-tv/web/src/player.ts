import {useCallback, useEffect, useRef, useState} from 'react';
import {getToken, refreshSession} from './auth';

export interface PlayerTrack {
  id: number;
  kind: 'audio' | 'sub';
  label: string;
  language: string;
  codecs: string;
  channels: number;
  selected: boolean;
}
export interface PlayerChoice {
  id: number;
  label: string;
  current: boolean;
}
export interface TorrentStats {
  state: string;
  downloaded: number;
  total: number;
  peers: number;
  download_speed: number;
  upload_speed: number;
}
/** Everything the TV publishes about its player. Every field is optional. */
export interface PlayerState {
  active?: boolean;
  title?: string;
  episode?: string;
  source?: string;
  position_ms?: number;
  duration_ms?: number;
  buffered_ms?: number;
  playing?: boolean;
  buffering?: boolean;
  seekable?: boolean;
  live?: boolean;
  ended?: boolean;
  error?: string;
  speed?: number;
  resize?: number;
  subtitle_size?: string;
  /** Episode still, else the title's poster. Absolute URL from the addon. */
  poster?: string;
  background?: string;
  tracks?: PlayerTrack[];
  sources?: PlayerChoice[];
  episodes?: PlayerChoice[];
  torrent?: TorrentStats | null;
}

export type PlayerAction = string;

/**
 * The TV streams its player state over a socket and takes player actions back
 * on the same one. Reconnects on its own, refreshing the token when the socket
 * keeps being refused, which is what an expired JWT looks like from here.
 */
export function usePlayerSocket(enabled: boolean) {
  const [state, setState] = useState<PlayerState | null>(null);
  const [connected, setConnected] = useState(false);
  const socket = useRef<WebSocket | null>(null);

  useEffect(() => {
    if (!enabled) return;
    let closed = false;
    let attempts = 0;
    let timer: number | undefined;

    async function connect() {
      const token = getToken();
      if (!token || closed) return;
      const scheme = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const ws = new WebSocket(`${scheme}//${window.location.host}/api/ws`);
      socket.current = ws;
      // A handshake cannot carry a header, so the token goes in the first frame.
      ws.onopen = () => ws.send(JSON.stringify({type: 'auth', token}));
      ws.onmessage = (event) => {
        let message: {type?: string; state?: PlayerState | null};
        try {
          message = JSON.parse(event.data as string);
        } catch {
          return;
        }
        if (message.type === 'player') {
          setState(message.state ?? null);
          setConnected(true);
        }
      };
      ws.onclose = () => {
        setConnected(false);
        if (closed) return;
        attempts += 1;
        // A rejected token looks exactly like a refused socket from here, so
        // renew before giving up on the pairing.
        if (attempts % 2 === 0) refreshSession().catch(() => {});
        timer = window.setTimeout(connect, Math.min(1000 * attempts, 10_000));
      };
      ws.onerror = () => ws.close();
    }

    connect();
    return () => {
      closed = true;
      if (timer) window.clearTimeout(timer);
      socket.current?.close();
      socket.current = null;
    };
  }, [enabled]);

  /** Returns false when the socket is down, so callers can surface it. */
  const command = useCallback((action: PlayerAction, extra: Record<string, unknown> = {}) => {
    const ws = socket.current;
    if (!ws || ws.readyState !== WebSocket.OPEN) return false;
    ws.send(JSON.stringify({type: 'player', action, ...extra}));
    return true;
  }, []);

  return {state, connected, command};
}
