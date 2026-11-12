import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { api } from '../api/client';
import type { UpdateCheck } from '../api/types';
import { UpdatesMenu } from './UpdatesMenu';
import { decodeUpdatePreferences, STARTUP_CHECK_DELAY_MS, AUTOMATIC_CHECK_INTERVAL_MS, UPDATE_STORAGE_KEY, useUpdates } from './useUpdates';

vi.mock('../api/client', () => ({ api: { checkForUpdate: vi.fn(), openUpdatePage: vi.fn() }, errorMessage: (e: unknown) => (e as Error).message }));

function Harness() { return <UpdatesMenu updates={useUpdates()} />; }
const newer: UpdateCheck = { currentVersion: '0.2.1', latestVersion: '0.3.0', available: true, releaseName: 'Git View 0.3.0', releaseUrl: 'https://github.com/tenacioustechie/git-view/releases/tag/v0.3.0', publishedAt: '2026-11-10T00:00:00Z', notes: 'New things', notesTruncated: false, checkedAt: '2026-11-12T00:00:00Z' };
const current: UpdateCheck = { ...newer, latestVersion: '0.2.1', available: false, notes: null };
const saved = () => JSON.parse(localStorage.getItem(UPDATE_STORAGE_KEY)!);
const advance = async (ms: number) => { await act(async () => { await vi.advanceTimersByTimeAsync(ms); }); };
const openMenu = () => fireEvent.click(screen.getByRole('button', { name: /^Updates/ }));

beforeEach(() => { vi.useFakeTimers(); vi.resetAllMocks(); localStorage.clear(); });
afterEach(() => { cleanup(); vi.useRealTimers(); });

it('checks automatically after startup by default, highlights a newer version and opens only through the backend', async () => {
  vi.mocked(api.checkForUpdate).mockResolvedValue(newer);
  render(<Harness />);
  await advance(STARTUP_CHECK_DELAY_MS - 1);
  expect(api.checkForUpdate).not.toHaveBeenCalled();
  await advance(1);
  expect(api.checkForUpdate).toHaveBeenCalledTimes(1);
  expect(screen.getByRole('button', { name: 'Updates: new version available' }).textContent).toBe('Update available');
  openMenu();
  expect(screen.getByRole('status').textContent).toContain('Git View 0.3.0 is available. You have 0.2.1.');
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Update Git View…' })); });
  expect(api.openUpdatePage).toHaveBeenCalledTimes(1);
  await advance(AUTOMATIC_CHECK_INTERVAL_MS);
  expect(api.checkForUpdate).toHaveBeenCalledTimes(2);
  // Defaults are not written until the user changes something.
  expect(localStorage.getItem(UPDATE_STORAGE_KEY)).toBeNull();
});

it('skipping removes the highlight but keeps the update reachable, and survives remount', async () => {
  vi.mocked(api.checkForUpdate).mockResolvedValue(newer);
  const view = render(<Harness />);
  await advance(STARTUP_CHECK_DELAY_MS);
  openMenu();
  fireEvent.click(screen.getByRole('button', { name: 'Skip this version' }));
  expect(saved()).toEqual({ version: 1, automatic: true, skippedVersion: '0.3.0' });
  expect(screen.getByRole('button', { name: 'Updates' })).toBeTruthy();
  expect(screen.getByText(/You skipped this version/)).toBeTruthy();
  expect(screen.getByRole('button', { name: 'Update Git View…' })).toBeTruthy();
  view.unmount(); render(<Harness />);
  await advance(STARTUP_CHECK_DELAY_MS);
  expect(screen.getByRole('button', { name: 'Updates' })).toBeTruthy();
  openMenu();
  fireEvent.click(screen.getByRole('button', { name: 'Stop skipping 0.3.0' }));
  expect(screen.getByRole('button', { name: 'Updates: new version available' })).toBeTruthy();
});

it('turning automatic checks off stops scheduled checks; manual checks still work and report up-to-date or errors', async () => {
  vi.mocked(api.checkForUpdate).mockRejectedValueOnce(new Error('Could not reach GitHub.')).mockResolvedValue(current);
  render(<Harness />);
  openMenu();
  fireEvent.click(screen.getByRole('checkbox', { name: /Check automatically/ }));
  expect(saved().automatic).toBe(false);
  await advance(AUTOMATIC_CHECK_INTERVAL_MS * 2);
  expect(api.checkForUpdate).not.toHaveBeenCalled();
  expect(screen.getByRole('status').textContent).toContain('Automatic checks are off.');
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Check for updates' })); });
  expect(screen.getByRole('status').textContent).toContain('Could not check for updates: Could not reach GitHub.');
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Check for updates' })); });
  expect(screen.getByRole('status').textContent).toContain('Git View 0.2.1 is up to date.');
  expect(screen.queryByRole('button', { name: 'Update Git View…' })).toBeNull();
});

it('reports a browser-open failure without calling it a check failure', async () => {
  vi.mocked(api.checkForUpdate).mockResolvedValue(newer);
  vi.mocked(api.openUpdatePage).mockRejectedValue(new Error('No default browser'));
  render(<Harness />);
  await advance(STARTUP_CHECK_DELAY_MS);
  openMenu();
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Update Git View…' })); });
  expect(screen.getByRole('status').textContent).toContain('Could not open the release page: No default browser');
});

it('decodes stored preferences defensively', () => {
  expect(decodeUpdatePreferences(null)).toEqual({ automatic: true, skippedVersion: null });
  expect(decodeUpdatePreferences('{"version":2,"automatic":false}')).toEqual({ automatic: true, skippedVersion: null });
  expect(decodeUpdatePreferences('{"version":1,"automatic":"no","skippedVersion":5}')).toEqual({ automatic: true, skippedVersion: null });
  expect(decodeUpdatePreferences('{"version":1,"automatic":false,"skippedVersion":"1.0.0"}')).toEqual({ automatic: false, skippedVersion: '1.0.0' });
  expect(() => decodeUpdatePreferences('{')).toThrow();
});
