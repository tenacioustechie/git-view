import { useCallback, useEffect, useState } from 'react';

export interface ViewPreferences {
  openingView: 'changes' | 'history';
  showCommitDetails: boolean;
  changesWidth: number;
  branchesWidth: number;
  historyWidth: number;
}
export const VIEW_STORAGE_KEY = import.meta.env.VITE_NATIVE_SMOKE === '1'
  ? 'git-view.native-smoke.view-preferences.v1' : 'git-view.view-preferences.v1';
export const DEFAULT_VIEW: ViewPreferences = { openingView: 'changes', showCommitDetails: true, changesWidth: 350, branchesWidth: 200, historyWidth: 360 };
export const PANEL_LIMITS = { changesWidth: [280, 480], branchesWidth: [155, 320], historyWidth: [240, 600] } as const;
export type PanelWidth = keyof typeof PANEL_LIMITS;

function normalize(value: Partial<ViewPreferences>): ViewPreferences {
  const next = { ...DEFAULT_VIEW };
  if (value.openingView === 'changes' || value.openingView === 'history') next.openingView = value.openingView;
  if (typeof value.showCommitDetails === 'boolean') next.showCommitDetails = value.showCommitDetails;
  for (const key of Object.keys(PANEL_LIMITS) as PanelWidth[]) {
    const width = value[key];
    if (typeof width === 'number' && Number.isFinite(width)) {
      const [min, max] = PANEL_LIMITS[key];
      next[key] = Math.round(Math.max(min, Math.min(max, width)));
    }
  }
  return next;
}
export function decodeViewPreferences(raw: string | null): ViewPreferences {
  if (!raw) return { ...DEFAULT_VIEW };
  if (raw.length > 4096) throw new Error('Oversized view preferences');
  const data: unknown = JSON.parse(raw);
  if (!data || typeof data !== 'object' || !('version' in data) || data.version !== 1
    || !('preferences' in data) || !data.preferences || typeof data.preferences !== 'object' || Array.isArray(data.preferences)) throw new Error('Invalid view preferences');
  return normalize(data.preferences);
}

export function useViewPreferences() {
  const [state, setState] = useState(() => {
    try { return { value: decodeViewPreferences(localStorage.getItem(VIEW_STORAGE_KEY)), revision: 0, reset: false, warning: '' }; }
    catch { return { value: { ...DEFAULT_VIEW }, revision: 0, reset: false, warning: 'Saved view preferences could not be loaded. Defaults are in use.' }; }
  });
  const [saveWarning, setSaveWarning] = useState('');
  useEffect(() => {
    if (!state.revision) return;
    try {
      if (state.reset) localStorage.removeItem(VIEW_STORAGE_KEY);
      else localStorage.setItem(VIEW_STORAGE_KEY, JSON.stringify({ version: 1, preferences: state.value }));
      setSaveWarning('');
    } catch { setSaveWarning('View preferences could not be saved. Changes apply here, but older settings may return when you reopen a repository or restart.'); }
  }, [state]);
  const update = useCallback((patch: Partial<ViewPreferences>) => setState(previous => ({
    value: normalize({ ...previous.value, ...patch }), revision: previous.revision + 1, reset: false, warning: '',
  })), []);
  const reset = useCallback(() => setState(previous => ({ value: { ...DEFAULT_VIEW }, revision: previous.revision + 1, reset: true, warning: '' })), []);
  return { preferences: state.value, update, reset, warning: saveWarning || state.warning };
}
