import { useCallback, useEffect, useRef, useState } from 'react';
import { api, errorMessage } from '../api/client';
import type { UpdateCheck } from '../api/types';

const NATIVE_SMOKE = import.meta.env.VITE_NATIVE_SMOKE === '1';
export const UPDATE_STORAGE_KEY = 'git-view.updates.v1';
/** Automatic checks wait briefly after launch so they never compete with opening a workspace. */
export const STARTUP_CHECK_DELAY_MS = 5_000;
export const AUTOMATIC_CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;

export type UpdatePreferences = { automatic: boolean; skippedVersion: string | null };
const DEFAULTS: UpdatePreferences = { automatic: true, skippedVersion: null };

export function decodeUpdatePreferences(raw: string | null): UpdatePreferences {
  if (!raw) return DEFAULTS;
  const data: unknown = JSON.parse(raw);
  if (!data || typeof data !== 'object' || !('version' in data) || data.version !== 1) return DEFAULTS;
  const value = data as { automatic?: unknown; skippedVersion?: unknown };
  return {
    automatic: typeof value.automatic === 'boolean' ? value.automatic : DEFAULTS.automatic,
    skippedVersion: typeof value.skippedVersion === 'string' && value.skippedVersion.length <= 64 ? value.skippedVersion : null,
  };
}

function loadPreferences(): UpdatePreferences {
  try { return decodeUpdatePreferences(localStorage.getItem(UPDATE_STORAGE_KEY)); }
  catch { return DEFAULTS; }
}

export type UpdateState = {
  preferences: UpdatePreferences;
  result: UpdateCheck | null;
  checking: boolean;
  error: string | null;
  /** True when a newer version exists and the user has not skipped it. */
  highlighted: boolean;
  check: () => Promise<void>;
  openReleasePage: () => Promise<void>;
  skip: (version: string | null) => void;
  setAutomatic: (automatic: boolean) => void;
};

export function useUpdates(): UpdateState {
  // Native acceptance must never reach the network, whatever the saved preference.
  const [preferences, setPreferences] = useState<UpdatePreferences>(() => NATIVE_SMOKE ? { ...DEFAULTS, automatic: false } : loadPreferences());
  const [result, setResult] = useState<UpdateCheck | null>(null);
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inFlight = useRef(false);
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);

  const changed = useRef(false);
  const update = useCallback((change: Partial<UpdatePreferences>) => {
    changed.current = true;
    setPreferences(previous => ({ ...previous, ...change }));
  }, []);
  // Keep state updaters pure; persist only user changes, never the defaults.
  useEffect(() => {
    if (!changed.current || NATIVE_SMOKE) return;
    try { localStorage.setItem(UPDATE_STORAGE_KEY, JSON.stringify({ version: 1, ...preferences })); }
    catch { /* The preference stays session-only. */ }
  }, [preferences]);

  const check = useCallback(async () => {
    if (inFlight.current) return;
    inFlight.current = true;
    setChecking(true); setError(null);
    try {
      const next = await api.checkForUpdate();
      if (mounted.current) setResult(next);
    } catch (cause) {
      if (mounted.current) setError(`Could not check for updates: ${errorMessage(cause)}`);
    } finally {
      inFlight.current = false;
      if (mounted.current) setChecking(false);
    }
  }, []);

  useEffect(() => {
    if (!preferences.automatic) return;
    const startup = window.setTimeout(() => void check(), STARTUP_CHECK_DELAY_MS);
    const daily = window.setInterval(() => void check(), AUTOMATIC_CHECK_INTERVAL_MS);
    return () => { window.clearTimeout(startup); window.clearInterval(daily); };
  }, [preferences.automatic, check]);

  const openReleasePage = useCallback(async () => {
    setError(null);
    try { await api.openUpdatePage(); }
    catch (cause) { if (mounted.current) setError(`Could not open the release page: ${errorMessage(cause)}`); }
  }, []);

  const highlighted = Boolean(result?.available && result.latestVersion && result.latestVersion !== preferences.skippedVersion);
  return {
    preferences, result, checking, error, highlighted, check, openReleasePage,
    skip: version => update({ skippedVersion: version }),
    setAutomatic: automatic => update({ automatic }),
  };
}
