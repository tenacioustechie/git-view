import { StrictMode } from 'react';
import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { decodeViewPreferences, DEFAULT_VIEW, useViewPreferences, VIEW_STORAGE_KEY } from './useViewPreferences';

beforeEach(() => localStorage.clear());
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
describe('view preference persistence', () => {
  it('uses defaults without writing on mount, including StrictMode', () => {
    const write = vi.spyOn(Storage.prototype, 'setItem');
    const { result } = renderHook(useViewPreferences, { wrapper: StrictMode });
    expect(result.current.preferences).toEqual(DEFAULT_VIEW);
    expect(write).not.toHaveBeenCalled();
  });
  it('persists only display preferences and restores them on remount', () => {
    const first = renderHook(useViewPreferences, { wrapper: StrictMode });
    act(() => first.result.current.update({ openingView: 'history', showCommitDetails: false, diffLayout: 'split', changesWidth: 420 }));
    const saved = first.result.current.preferences;
    expect(JSON.parse(localStorage.getItem(VIEW_STORAGE_KEY)!)).toEqual({ version: 1, preferences: saved });
    first.unmount();
    const next = renderHook(useViewPreferences);
    expect(next.result.current.preferences).toEqual(saved);
  });
  it('clamps and rounds widths, defaults invalid fields, and discards unknown keys', () => {
    expect(decodeViewPreferences(JSON.stringify({ version: 1, preferences: {
      openingView: 'stashes', showCommitDetails: 'false', changesWidth: 9999, branchesWidth: -20, historyWidth: 340.5, draft: 'Do not save this',
    } }))).toEqual({ ...DEFAULT_VIEW, changesWidth: 480, branchesWidth: 155, historyWidth: 341 });
    expect(decodeViewPreferences(JSON.stringify({ version: 1, preferences: { changesWidth: '400', branchesWidth: null } }))).toEqual(DEFAULT_VIEW);
  });
  it('loads older settings with unified diffs and rejects unknown layout values', () => {
    for (const diffLayout of [undefined, 'unknown', null, 3]) {
      expect(decodeViewPreferences(JSON.stringify({ version: 1, preferences: { diffLayout, openingView: 'history' } }))).toEqual({ ...DEFAULT_VIEW, openingView: 'history' });
    }
  });
  it.each(['{bad', '{"version":2,"preferences":{}}', '{"version":1,"preferences":[]}', 'x'.repeat(4097)])('preserves malformed/unsupported storage until an explicit edit (%#)', raw => {
    localStorage.setItem(VIEW_STORAGE_KEY, raw);
    const { result } = renderHook(useViewPreferences);
    expect(result.current.warning).toContain('could not be loaded');
    expect(localStorage.getItem(VIEW_STORAGE_KEY)).toBe(raw);
    act(() => result.current.update({ historyWidth: 400 }));
    expect(decodeViewPreferences(localStorage.getItem(VIEW_STORAGE_KEY)).historyWidth).toBe(400);
    expect(result.current.warning).toBe('');
  });
  it('reset removes only view preferences, not Recent or other stored values', () => {
    localStorage.setItem('git-view.recent-workspaces.v1', 'keep recent');
    const { result } = renderHook(useViewPreferences);
    act(() => result.current.update({ openingView: 'history', branchesWidth: 310 }));
    act(() => result.current.reset());
    expect(result.current.preferences).toEqual(DEFAULT_VIEW);
    expect(localStorage.getItem(VIEW_STORAGE_KEY)).toBeNull();
    expect(localStorage.getItem('git-view.recent-workspaces.v1')).toBe('keep recent');
  });
  it('handles denied reads/writes/reset without breaking the current view', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => { throw new Error('Denied'); });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('Quota'); });
    vi.spyOn(Storage.prototype, 'removeItem').mockImplementation(() => { throw new Error('Denied'); });
    const { result } = renderHook(useViewPreferences);
    expect(result.current.warning).toContain('could not be loaded');
    act(() => result.current.update({ changesWidth: 410 }));
    expect(result.current.preferences.changesWidth).toBe(410);
    expect(result.current.warning).toContain('could not be saved');
    act(() => result.current.reset());
    expect(result.current.preferences).toEqual(DEFAULT_VIEW);
    expect(result.current.warning).toContain('older settings may return');
  });
});
