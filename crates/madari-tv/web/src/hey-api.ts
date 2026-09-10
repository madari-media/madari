import type {CreateClientConfig} from './client/client.gen';

/**
 * Same-origin requests to the TV that served this page. The bearer token is
 * attached per request in `auth.ts`, so pairing and logout need no rebuild.
 */
export const createClientConfig: CreateClientConfig = (config) => ({
  ...config,
  baseUrl: window.location.origin,
});
