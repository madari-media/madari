import {client} from './client/client.gen';

const TOKEN_KEY = 'madari-tv-token';
const EXPIRY_KEY = 'madari-tv-expires';

/**
 * The credential is a JWT signed with a key the TV keeps on disk, so it stays
 * valid across a TV app restart, a page reload and a browser restart. It lives
 * in localStorage for exactly that reason.
 */
export function getToken(): string {
  return localStorage.getItem(TOKEN_KEY) ?? '';
}
export function hasSession(): boolean {
  return getToken().length > 0;
}
export function setSession(token: string, expiresIn: number): void {
  if (!token) return;
  localStorage.setItem(TOKEN_KEY, token);
  if (expiresIn > 0) localStorage.setItem(EXPIRY_KEY, String(Date.now() + expiresIn * 1000));
}
export function clearSession(): void {
  localStorage.removeItem(TOKEN_KEY);
  localStorage.removeItem(EXPIRY_KEY);
}

/**
 * Renew this long before the token expires. The window only has to outlast one
 * poll cycle, because the overview query keeps the session warm while the page
 * is open.
 */
const EARLY_MS = 5 * 60 * 1000;

function isExpiringSoon(): boolean {
  const at = Number(localStorage.getItem(EXPIRY_KEY) ?? 0);
  return at === 0 || at - Date.now() < EARLY_MS;
}
/** Pairing is the only call that must not carry a token. */
function isAuthPath(url: string): boolean {
  return url.includes('/api/pair');
}

let refreshing: Promise<boolean> | null = null;
/** Single-flight renewal so concurrent requests share one exchange. */
export function refreshSession(): Promise<boolean> {
  const token = getToken();
  if (!token) return Promise.resolve(false);
  refreshing ??= fetch('/api/session/refresh', {
    method: 'POST',
    headers: {'Content-Type': 'application/json', Authorization: `Bearer ${token}`},
    body: '{}',
  })
    .then(async (response) => {
      if (!response.ok) return false;
      const body = (await response.json()) as {token: string; expires_in: number};
      setSession(body.token, body.expires_in);
      return true;
    })
    .catch(() => false)
    .finally(() => {
      refreshing = null;
    });
  return refreshing;
}

export function configureAuth(): void {
  client.interceptors.request.use(async (request) => {
    if (isAuthPath(request.url)) return request;
    // Renew before sending rather than after a guaranteed 401.
    if (getToken() && isExpiringSoon()) await refreshSession();
    const token = getToken();
    if (token) request.headers.set('Authorization', `Bearer ${token}`);
    else request.headers.delete('Authorization');
    return request;
  });
  client.interceptors.response.use(async (response, request) => {
    if (response.status !== 401 || isAuthPath(request.url)) return response;
    const renewed = await refreshSession();
    if (!renewed) {
      // The token no longer verifies (new signing key or expired): pair again.
      clearSession();
      return response;
    }
    // Only reads are safe to replay; a mutation reports its own error once.
    if (request.method !== 'GET') return response;
    const retry = new Request(request);
    retry.headers.set('Authorization', `Bearer ${getToken()}`);
    return fetch(retry);
  });
}
