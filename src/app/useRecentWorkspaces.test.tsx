import { act, cleanup, renderHook } from '@testing-library/react';
import { StrictMode } from 'react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { decodeRecent, RECENT_LIMIT, RECENT_STORAGE_KEY, useRecentWorkspaces } from './useRecentWorkspaces';

beforeEach(() => localStorage.clear());
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

it('validates stored entries, drops duplicates and bounds the recent list', () => {
  const item = { kind: 'repository', path: '/repo', name: 'Repo' };
  expect(decodeRecent(JSON.stringify({ version: 1, items: [null, {}, { ...item, kind: 'url' }, { ...item, path: 'relative' }, item, item, { ...item, kind: 'project' }] }))).toEqual([item, { ...item, kind: 'project' }]);
  expect(() => decodeRecent('{bad')).toThrow();
  expect(() => decodeRecent('{"version":2,"items":[]}')).toThrow();
  expect(() => decodeRecent('x'.repeat(1_000_001))).toThrow();
  expect(decodeRecent(JSON.stringify({ version: 1, items: Array.from({ length: 30 }, (_, n) => ({ ...item, path: `/repo/${n}` })) }))).toHaveLength(RECENT_LIMIT);
});

it('keeps newest first, deduplicates exact paths and kinds, and persists without StrictMode duplicates', () => {
  const { result } = renderHook(useRecentWorkspaces, { wrapper: StrictMode });
  act(() => { for (let i = 0; i < 25; i++) result.current.remember({ kind: 'repository', name: `Repo ${i}`, path: `/repo/${i}` }); });
  expect(result.current.items).toHaveLength(20);
  act(() => result.current.remember({ kind: 'repository', name: 'Updated name', path: '/repo/10' }));
  expect(result.current.items[0].name).toBe('Updated name');
  expect(result.current.items.filter(item => item.path === '/repo/10')).toHaveLength(1);
  expect(decodeRecent(localStorage.getItem(RECENT_STORAGE_KEY))).toEqual(result.current.items);
});

it('supports Windows and UNC paths without merging differently cased POSIX paths', () => {
  const { result } = renderHook(useRecentWorkspaces);
  act(() => { for (const path of ['C:\\work\\repo', '\\\\server\\share\\repo', '/Repo', '/repo']) result.current.remember({ kind: 'repository', name: 'Repo', path }); });
  expect(result.current.items).toHaveLength(4);
});

it('recovers from malformed storage and reports unavailable persistence without blocking session use', () => {
  localStorage.setItem(RECENT_STORAGE_KEY, '{bad');
  const { result } = renderHook(useRecentWorkspaces);
  expect(result.current.warning).toContain('could not be loaded');
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('Quota'); });
  act(() => result.current.remember({ kind: 'project', path: '/work.json', name: 'Work' }));
  expect(result.current.items).toHaveLength(1);
  expect(result.current.warning).toContain('could not be saved');
  vi.spyOn(Storage.prototype, 'removeItem').mockImplementation(() => { throw new Error('Denied'); });
  act(() => result.current.clear());
  expect(result.current.items).toHaveLength(0);
  expect(result.current.warning).toContain('older saved entries may return');
});

it('keeps the app usable when reading local storage is denied', () => {
  vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => { throw new Error('Denied'); });
  const { result } = renderHook(useRecentWorkspaces);
  expect(result.current.items).toEqual([]);
  expect(result.current.warning).toContain('could not be loaded');
  act(() => result.current.remember({ kind: 'repository', path: '/repo', name: 'Repo' }));
  expect(result.current.items[0].path).toBe('/repo');
});
