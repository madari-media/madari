import {useCallback, useEffect, useMemo, useState} from 'react';
import {useMutation, useQuery, useQueryClient} from '@tanstack/react-query';
import {useToast} from '@astryxdesign/core/Toast';
import {
  configureAddonMutation,
  createProfileMutation,
  installAddonMutation,
  lockProfileMutation,
  logoutMutation,
  overviewOptions,
  overviewQueryKey,
  pairMutation,
  remoteMutation,
  removeAddonMutation,
  reorderAddonsMutation,
  selectProfileMutation,
  setAddonEnabledMutation,
  setPreferencesMutation,
  shareAddonMutation,
  updateProfileMutation,
} from './client/@tanstack/react-query.gen';
import type {Overview, PlaybackPreferences} from './client';
import {clearSession, hasSession, setSession} from './auth';

export interface Controller {
  paired: boolean;
  overview: Overview | undefined;
  isLoading: boolean;
  busy: boolean;
  error: string | null;
  clearError: () => void;
  pair: (code: string) => Promise<void>;
  disconnect: () => Promise<void>;
  refresh: () => void;
  selectProfile: (id: string, pin: string) => Promise<void>;
  lockProfile: () => Promise<void>;
  createProfile: (name: string, pin: string, kids: boolean) => Promise<void>;
  updateProfile: (name: string, pin: string) => Promise<void>;
  setPreferences: (preferences: PlaybackPreferences) => Promise<void>;
  installAddon: (url: string, allowLocal: boolean) => Promise<void>;
  configureAddon: (id: string, url: string, allowLocal: boolean) => Promise<void>;
  setAddonEnabled: (id: string, enabled: boolean) => Promise<void>;
  reorderAddons: (ids: string[]) => Promise<void>;
  shareAddon: (id: string, targetId: string, pin: string) => Promise<void>;
  removeAddon: (id: string) => Promise<void>;
  sendRemote: (command: string) => Promise<void>;
}

/** The server throws the JSON error body, so surface its `error` field. */
function messageOf(cause: unknown): string {
  if (cause && typeof cause === 'object' && 'error' in cause) {
    const value = (cause as {error: unknown}).error;
    if (typeof value === 'string' && value) return value;
  }
  if (cause instanceof Error && cause.message) return cause.message;
  return 'The TV could not save this change.';
}

/**
 * All server state lives in the generated `overview` query. Every mutation
 * returns the refreshed overview, so we write it straight into the query cache
 * instead of invalidating and refetching.
 */
export function useController(): Controller {
  const queryClient = useQueryClient();
  const toast = useToast();
  const [paired, setPaired] = useState(() => hasSession());
  const [error, setError] = useState<string | null>(null);

  const overviewQuery = useQuery({
    ...overviewOptions(),
    enabled: paired,
    retry: false,
    refetchInterval: paired ? 15_000 : false,
  });

  const store = useCallback(
    (data: Overview | undefined) => {
      if (data) queryClient.setQueryData(overviewQueryKey(), data);
    },
    [queryClient],
  );

  const fail = useCallback(
    (cause: unknown) => {
      // The interceptor clears storage when a refresh fails, so no stored
      // session here means the pairing is gone rather than a validation error.
      if (!hasSession()) {
        setPaired(false);
        queryClient.clear();
        toast({body: 'Pairing expired. Enter the TV code again.', type: 'error'});
        return;
      }
      setError(messageOf(cause));
    },
    [queryClient, toast],
  );

  // A stale or expired token must fall back to pairing instead of a stuck loading state.
  useEffect(() => {
    if (overviewQuery.error) fail(overviewQuery.error);
  }, [overviewQuery.error, fail]);

  const ok = useCallback(
    (message: string) => (data: Overview) => {
      store(data);
      setError(null);
      toast({body: message});
    },
    [store, toast],
  );

  const pair = useMutation({
    ...pairMutation(),
    // The generated mutation returns the JWT; store it before the overview query runs.
    onSuccess: (data) => {
      setSession(data.token, data.expires_in);
      setError(null);
      setPaired(true);
    },
    onError: fail,
  });
  const select = useMutation({...selectProfileMutation(), onSuccess: ok('Profile opened.'), onError: fail});
  const lock = useMutation({...lockProfileMutation(), onSuccess: ok('Profile locked.'), onError: fail});
  const create = useMutation({...createProfileMutation(), onSuccess: ok('Profile created.'), onError: fail});
  const update = useMutation({...updateProfileMutation(), onSuccess: ok('Profile saved.'), onError: fail});
  const preferences = useMutation({...setPreferencesMutation(), onSuccess: ok('Playback preferences saved.'), onError: fail});
  const install = useMutation({...installAddonMutation(), onSuccess: ok('Addon installed.'), onError: fail});
  const configure = useMutation({...configureAddonMutation(), onSuccess: ok('Addon reconfigured.'), onError: fail});
  const enabled = useMutation({...setAddonEnabledMutation(), onSuccess: ok('Addon updated.'), onError: fail});
  const reorder = useMutation({...reorderAddonsMutation(), onSuccess: ok('Addon order saved.'), onError: fail});
  const share = useMutation({...shareAddonMutation(), onSuccess: ok('Addon shared.'), onError: fail});
  const remove = useMutation({...removeAddonMutation(), onSuccess: ok('Addon removed.'), onError: fail});
  // Fire-and-forget: a remote key must not put the whole UI into a busy state.
  const remote = useMutation({...remoteMutation(), onError: fail});
  const forgetSession = useCallback(() => {
    clearSession();
    setPaired(false);
    queryClient.clear();
  }, [queryClient]);
  const logout = useMutation({...logoutMutation(), onSettled: forgetSession, onError: forgetSession});

  const busy =
    pair.isPending ||
    select.isPending ||
    lock.isPending ||
    create.isPending ||
    update.isPending ||
    preferences.isPending ||
    install.isPending ||
    configure.isPending ||
    enabled.isPending ||
    reorder.isPending ||
    share.isPending ||
    remove.isPending ||
    logout.isPending;

  return useMemo(
    () => ({
      paired,
      overview: overviewQuery.data,
      isLoading: paired && overviewQuery.isLoading,
      busy,
      error,
      clearError: () => setError(null),
      pair: async (code) => {
        await pair.mutateAsync({body: {code}});
      },
      disconnect: async () => {
        await logout.mutateAsync({});
      },
      refresh: () => {
        void overviewQuery.refetch();
      },
      selectProfile: async (id, pin) => {
        await select.mutateAsync({body: {id, pin}});
      },
      lockProfile: async () => {
        await lock.mutateAsync({});
      },
      createProfile: async (name, pin, kids) => {
        await create.mutateAsync({body: {name, pin, kids}});
      },
      updateProfile: async (name, pin) => {
        await update.mutateAsync({body: {name, pin}});
      },
      setPreferences: async (next) => {
        await preferences.mutateAsync({body: next});
      },
      installAddon: async (url, allowLocal) => {
        await install.mutateAsync({body: {url, allow_local: allowLocal}});
      },
      configureAddon: async (id, url, allowLocal) => {
        await configure.mutateAsync({path: {id}, body: {url, allow_local: allowLocal}});
      },
      setAddonEnabled: async (id, value) => {
        await enabled.mutateAsync({path: {id}, body: {enabled: value}});
      },
      reorderAddons: async (ids) => {
        await reorder.mutateAsync({body: {ids}});
      },
      shareAddon: async (id, targetId, pin) => {
        await share.mutateAsync({path: {id}, body: {target_id: targetId, pin}});
      },
      removeAddon: async (id) => {
        await remove.mutateAsync({path: {id}});
      },
      sendRemote: async (command) => {
        await remote.mutateAsync({body: {command}});
      },
    }),
    [
      paired, overviewQuery.data, overviewQuery.isLoading, overviewQuery.refetch,
      busy, error, pair, select, lock, create, update, preferences, install,
      configure, enabled, reorder, share, remove, logout, remote,
    ],
  );
}
