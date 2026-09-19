import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { api } from '../api/client';
import { withNavigationBlock, navigationBlocked } from '../api/navigation';
import type { LaunchRequest, RepositoryOpen } from '../api/types';
import { App } from './App';

vi.mock('../api/client', () => ({ api: { getLaunchRequest: vi.fn(), ackLaunchRequest: vi.fn(), openRepository: vi.fn() }, errorMessage: String }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock('../features/groups/GroupView', () => ({ GroupView: () => <div>Project fixture</div> }));
vi.mock('../features/groups/DiscoveryView', () => ({ DiscoveryView: ({ initialRoot, initialDepth }: { initialRoot: string; initialDepth: number }) => <div>Scan {initialRoot} depth {initialDepth}</div> }));
vi.mock('../features/repository/RepositoryView', () => ({ RepositoryView: () => <div>Repository fixture</div> }));

const scan: LaunchRequest = { requestId: 'scan', kind: 'discovery', path: '/scan', scanDepth: 0, error: null };
const tick = async () => { await act(async () => { await vi.advanceTimersByTimeAsync(500); }); };
beforeEach(() => { vi.useFakeTimers(); vi.resetAllMocks(); vi.mocked(api.getLaunchRequest).mockResolvedValue(null); });
afterEach(() => { cleanup(); vi.useRealTimers(); });

it('delivers queued depth-zero discovery only after a native dialog closes', async () => {
  let close!: () => void;
  const dialog = withNavigationBlock(() => new Promise<void>(resolve => { close = resolve; }));
  vi.mocked(api.getLaunchRequest).mockResolvedValue(scan);
  render(<App />);
  await tick();
  expect(api.getLaunchRequest).not.toHaveBeenCalled();
  close(); await dialog; await tick();
  expect(screen.getByText('Scan /scan depth 0')).toBeTruthy();
  expect(api.ackLaunchRequest).toHaveBeenCalledWith('scan');
});

it('rechecks modal visibility after asynchronous launch loading and retries without acknowledgement', async () => {
  let resolve!: (value: LaunchRequest) => void;
  vi.mocked(api.getLaunchRequest).mockReturnValueOnce(new Promise(done => { resolve = done; }));
  render(<App />);
  const modal = document.createElement('dialog'); modal.open = true; document.body.append(modal);
  await act(async () => { resolve(scan); });
  expect(api.ackLaunchRequest).not.toHaveBeenCalled();
  expect(screen.queryByText('Scan /scan depth 0')).toBeNull();
  modal.remove();
  vi.mocked(api.getLaunchRequest).mockResolvedValue(scan);
  await tick();
  expect(api.ackLaunchRequest).toHaveBeenCalledWith('scan');
});

it('does not replace newer user navigation with a stale repository response', async () => {
  let resolve!: (value: RepositoryOpen) => void;
  vi.mocked(api.getLaunchRequest).mockResolvedValueOnce({ requestId: 'repo', kind: 'repository', path: '/repo', error: null });
  vi.mocked(api.openRepository).mockReturnValueOnce(new Promise(done => { resolve = done; }));
  render(<App />); await tick();
  fireEvent.click(screen.getByRole('link', { name: 'Git View' }));
  await act(async () => { resolve({ repositoryId: 'repo' } as RepositoryOpen); });
  expect(screen.queryByText('Repository fixture')).toBeNull();
  expect(api.ackLaunchRequest).not.toHaveBeenCalled();
});

it('keeps nested dialog blocking until both finish and releases it on rejection', async () => {
  let close!: () => void;
  const first = withNavigationBlock(() => new Promise<void>(resolve => { close = resolve; }));
  await expect(withNavigationBlock(async () => { throw new Error('dialog failed'); })).rejects.toThrow('dialog failed');
  expect(navigationBlocked()).toBe(true);
  close(); await first;
  expect(navigationBlocked()).toBe(false);
});
