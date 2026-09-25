import { useCallback, useEffect, useState } from 'react';

export type RecentWorkspace = { kind: 'repository' | 'project'; path: string; name: string };
export const RECENT_STORAGE_KEY = 'git-view.recent-workspaces.v1';
export const RECENT_LIMIT = 20;
const same = (a: RecentWorkspace, b: RecentWorkspace) => a.kind === b.kind && a.path === b.path;
const valid = (value: unknown): value is RecentWorkspace => {
  if (!value || typeof value !== 'object') return false;
  const item = value as Partial<RecentWorkspace>;
  return (item.kind === 'repository' || item.kind === 'project') && typeof item.path === 'string'
    && item.path.length > 0 && item.path.length <= 32768 && !item.path.includes('\0')
    && (/^\//.test(item.path) || /^[A-Za-z]:[\\/]/.test(item.path) || /^\\\\/.test(item.path))
    && typeof item.name === 'string' && item.name.trim().length > 0 && item.name.length <= 4096;
};

export function decodeRecent(raw: string | null): RecentWorkspace[] {
  if (!raw) return [];
  if (raw.length > 1_000_000) throw new Error('Recent list exceeds storage limit');
  const data: unknown = JSON.parse(raw);
  if (!data || typeof data !== 'object' || !('version' in data) || data.version !== 1 || !('items' in data) || !Array.isArray(data.items)) throw new Error('Invalid recent list');
  const items: RecentWorkspace[] = [];
  for (const item of data.items) {
    if (valid(item) && !items.some(existing => same(existing, item))) items.push({ kind: item.kind, path: item.path, name: item.name });
    if (items.length === RECENT_LIMIT) break;
  }
  return items;
}

export function useRecentWorkspaces() {
  const [state, setState] = useState(() => {
    try { return { items: decodeRecent(localStorage.getItem(RECENT_STORAGE_KEY)), revision: 0, warning: '' }; }
    catch { return { items: [] as RecentWorkspace[], revision: 0, warning: 'Saved recent items could not be loaded. New opens will start a fresh list.' }; }
  });
  const [saveWarning, setSaveWarning] = useState('');
  // Keep state updaters pure; persistence is a separate effect (also safe under StrictMode).
  useEffect(() => {
    if (!state.revision) return;
    try {
      if (state.items.length) localStorage.setItem(RECENT_STORAGE_KEY, JSON.stringify({ version: 1, items: state.items }));
      else localStorage.removeItem(RECENT_STORAGE_KEY);
      setSaveWarning('');
    } catch { setSaveWarning('Recent changes could not be saved. This list is session-only; older saved entries may return after restart.'); }
  }, [state]);
  const remember = useCallback((item: RecentWorkspace, updateOnly = false) => {
    if (!valid(item)) return;
    setState(previous => {
      const existing = previous.items.find(value => same(value, item));
      if (updateOnly && (!existing || existing.name === item.name)) return previous;
      return { items: updateOnly ? previous.items.map(value => same(value, item) ? item : value)
        : [item, ...previous.items.filter(value => !same(value, item))].slice(0, RECENT_LIMIT), revision: previous.revision + 1, warning: '' };
    });
  }, []);
  const clear = useCallback(() => setState(previous => ({ items: [], revision: previous.revision + 1, warning: '' })), []);
  return { items: state.items, warning: saveWarning || state.warning, remember, clear };
}
