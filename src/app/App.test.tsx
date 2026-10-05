import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { api } from '../api/client';
import { withNavigationBlock, navigationBlocked } from '../api/navigation';
import type { LaunchRequest, Project, RepositoryOpen } from '../api/types';
import { fixtures } from '../test/fixtures';
import { RECENT_STORAGE_KEY } from './useRecentWorkspaces';
import { App } from './App';
import { open } from '@tauri-apps/plugin-dialog';

vi.mock('../api/client', () => ({ api: { getLaunchRequest: vi.fn(), ackLaunchRequest: vi.fn(), openRepository: vi.fn(), openProject: vi.fn() }, errorMessage: String }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock('../features/groups/GroupView', () => ({ GroupView: ({ project, onProjectChange, onOpenRepository }: { project: Project; onProjectChange: (next: Project) => void; onOpenRepository: (next: RepositoryOpen) => void }) => <div>Project fixture
  <button onClick={() => onProjectChange({ ...project, name: 'Renamed project' })}>Mock project refresh</button>
  <button onClick={() => onProjectChange({ ...project, projectId: 'copy', displayPath: '/projects/copy.json' })}>Mock save as</button>
  <button onClick={() => onOpenRepository({ repositoryId: 'member', displayPath: '/repos/member', status: {} as RepositoryOpen['status'] })}>Mock member open</button>
</div> }));
vi.mock('../features/groups/DiscoveryView', () => ({ DiscoveryView: ({ initialRoot, initialDepth }: { initialRoot: string; initialDepth: number }) => <div>Scan {initialRoot} depth {initialDepth}</div> }));
vi.mock('../features/repository/RepositoryView', () => ({ RepositoryView: () => <div>Repository fixture</div> }));

const scan: LaunchRequest = { requestId: 'scan', kind: 'discovery', path: '/scan', scanDepth: 0, error: null };
const tick = async () => { await act(async () => { await vi.advanceTimersByTimeAsync(500); }); };
beforeEach(() => { vi.useFakeTimers(); vi.resetAllMocks(); localStorage.clear(); vi.mocked(api.getLaunchRequest).mockResolvedValue(null); });
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.useRealTimers(); });

const repo: RepositoryOpen = { repositoryId: 'repo', displayPath: '/repos/example', status: fixtures.clean };
const project: Project = { projectId: 'project', displayPath: '/projects/work.json', name: 'Work', revision: 'r1', content: { version: 1, name: 'Work', repositories: [] }, members: [], stale: false, error: null };
const saved = () => JSON.parse(localStorage.getItem(RECENT_STORAGE_KEY)!).items;

it('shows an empty Recent menu and records successful canonical picker opens, surviving remount', async () => {
  const view = render(<App />);
  fireEvent.click(screen.getByRole('button', { name: 'Recent' }));
  expect(screen.getByText(/No recent workspaces yet/)).toBeTruthy();
  vi.mocked(open).mockResolvedValue('/repos/example/nested');
  vi.mocked(api.openRepository).mockResolvedValue(repo);
  fireEvent.click(screen.getByRole('button', { name: 'Workspace' }));
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Open repository' })); });
  expect(saved()).toEqual([{ kind: 'repository', name: 'example', path: '/repos/example' }]);
  view.unmount(); render(<App />);
  fireEvent.click(screen.getByRole('button', { name: 'Recent' }));
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Open repository example: /repos/example' })); });
  expect(api.openRepository).toHaveBeenLastCalledWith('/repos/example');
  expect(screen.getByText('Repository fixture')).toBeTruthy();
  expect(screen.queryByRole('region', { name: 'Recent' })).toBeNull();
  expect(saved()).toHaveLength(1);
});

it('records CLI project opens, member opens and Save as without promoting background project refreshes', async () => {
  vi.mocked(api.getLaunchRequest).mockResolvedValueOnce({ requestId: 'p', kind: 'project', path: '/projects/work.json', error: null });
  vi.mocked(api.openProject).mockResolvedValue(project);
  render(<App />); await tick();
  expect(saved()[0]).toEqual({ kind: 'project', name: 'Work', path: '/projects/work.json' });
  fireEvent.click(screen.getByRole('button', { name: 'Mock member open' }));
  fireEvent.click(screen.getByRole('button', { name: 'Back to Work' }));
  fireEvent.click(screen.getByRole('button', { name: 'Mock project refresh' }));
  expect(saved().map((item: { name: string }) => item.name)).toEqual(['member', 'Renamed project']);
  fireEvent.click(screen.getByRole('button', { name: 'Mock save as' }));
  expect(saved()[0].path).toBe('/projects/copy.json');
  fireEvent.click(screen.getByRole('button', { name: 'Recent' }));
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Open project Renamed project: /projects/work.json' })); });
  expect(api.openProject).toHaveBeenLastCalledWith('/projects/work.json', true);
  expect(saved()[0].name).toBe('Work');
});

it('retains the current workspace and recent order when reopening fails; Clear recent deletes no files', async () => {
  localStorage.setItem(RECENT_STORAGE_KEY, JSON.stringify({ version: 1, items: [{ kind: 'project', name: 'Missing', path: '/missing.json' }] }));
  vi.mocked(api.getLaunchRequest).mockResolvedValueOnce({ requestId: 'r', kind: 'repository', path: repo.displayPath, error: null });
  vi.mocked(api.openRepository).mockResolvedValue(repo);
  vi.mocked(api.openProject).mockRejectedValue(new Error('File not found'));
  render(<App />); await tick();
  const before = saved();
  fireEvent.click(screen.getByRole('button', { name: 'Recent' }));
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Open project Missing: /missing.json' })); });
  expect(screen.getByRole('alert').textContent).toContain('File not found');
  expect(screen.getByText('Repository fixture')).toBeTruthy();
  expect(saved()).toEqual(before);
  fireEvent.click(screen.getByRole('button', { name: 'Recent' }));
  fireEvent.click(screen.getByRole('button', { name: 'Clear recent' }));
  expect(localStorage.getItem(RECENT_STORAGE_KEY)).toBeNull();
  expect(screen.getByText('Repository fixture')).toBeTruthy();
});

it('does not restore a cleared project entry during background refresh', async () => {
  vi.mocked(api.getLaunchRequest).mockResolvedValueOnce({ requestId: 'p', kind: 'project', path: project.displayPath, error: null });
  vi.mocked(api.openProject).mockResolvedValue(project);
  render(<App />); await tick();
  fireEvent.click(screen.getByRole('button', { name: 'Recent' }));
  fireEvent.click(screen.getByRole('button', { name: 'Clear recent' }));
  fireEvent.click(screen.getByRole('button', { name: 'Mock project refresh' }));
  expect(localStorage.getItem(RECENT_STORAGE_KEY)).toBeNull();
});

it('ignores late Recent responses after newer navigation and blocks Recent while a modal is open', async () => {
  localStorage.setItem(RECENT_STORAGE_KEY, JSON.stringify({ version: 1, items: [{ kind: 'repository', name: 'example', path: repo.displayPath }] }));
  let finish!: (next: RepositoryOpen) => void;
  vi.mocked(api.openRepository).mockImplementation(() => new Promise(resolve => { finish = resolve; }));
  render(<App />);
  fireEvent.click(screen.getByRole('button', { name: 'Recent' }));
  const modal = document.createElement('dialog'); modal.open = true; document.body.append(modal);
  fireEvent.click(screen.getByRole('button', { name: 'Open repository example: /repos/example' }));
  expect(api.openRepository).not.toHaveBeenCalled(); modal.remove();
  fireEvent.click(screen.getByRole('button', { name: 'Recent' }));
  fireEvent.click(screen.getByRole('button', { name: 'Open repository example: /repos/example' }));
  expect(navigationBlocked()).toBe(true);
  fireEvent.click(screen.getByRole('link', { name: 'Git View' }));
  await act(async () => { finish({ ...repo, displayPath: '/repos/different' }); });
  expect(navigationBlocked()).toBe(false);
  expect(screen.queryByText('Repository fixture')).toBeNull();
  expect(saved()).toEqual([{ kind: 'repository', name: 'example', path: repo.displayPath }]);
});

it('keeps opening actions in Workspace and closes it before opening the native picker', async () => {
  vi.mocked(open).mockResolvedValue(null);
  render(<App />);
  expect(screen.queryByRole('button', { name: 'Open repository' })).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'Workspace' }));
  expect(screen.getByRole('button', { name: 'Open project' })).toBeTruthy();
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Open repository' })); });
  expect(open).toHaveBeenCalledWith({ directory: true, multiple: false, title: 'Open repository' });
  expect(screen.queryByRole('region', { name: 'Workspace' })).toBeNull();
  expect(api.openRepository).not.toHaveBeenCalled();
  expect(localStorage.getItem(RECENT_STORAGE_KEY)).toBeNull();
});

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
