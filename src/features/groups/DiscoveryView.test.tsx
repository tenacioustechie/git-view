// Renderer-only tests: native discovery, persistence, and dialogs are mocked.
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { open, save } from '@tauri-apps/plugin-dialog';
import { api } from '../../api/client';
import { navigationBlocked } from '../../api/navigation';
import type { DiscoveryResult, Project } from '../../api/types';
import { fixtures } from '../../test/fixtures';
import { DiscoveryView } from './DiscoveryView';
import { GroupView } from './GroupView';

vi.mock('../../api/client', () => ({ api: { discoverRepositories: vi.fn(), createProject: vi.fn(), saveProject: vi.fn(), getOperations: vi.fn(), refreshProject: vi.fn() }, errorMessage: (cause: unknown) => cause instanceof Error ? cause.message : String(cause) }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(), save: vi.fn() }));
const result: DiscoveryResult = { rootPath: '/work', depth: 3, candidates: [{ repositoryId: 'api', displayPath: '/work/api', label: 'API' }, { repositoryId: 'web', displayPath: '/work/web', label: 'Web' }], issues: [], truncated: false, visitedDirectories: 5 };
const project: Project = { projectId: 'work', revision: 'revision-1', displayPath: '/work/work.git-view.json', name: 'Work', content: { version: 1, name: 'Work', repositories: [{ path: './api', label: 'Original label' }] }, members: [{ kind: 'ready', memberId: 'api-member', repositoryId: 'api', label: 'Original label', displayPath: '/work/api', status: fixtures.clean }], stale: false, error: null };
const checkbox = (name: string) => screen.getByRole('checkbox', { name }) as HTMLInputElement;
async function scan() { fireEvent.click(screen.getByRole('button', { name: 'Scan repositories' })); await screen.findByRole('checkbox', { name: 'Include API' }); }
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.discoverRepositories).mockResolvedValue(result);
  vi.mocked(api.createProject).mockResolvedValue(project);
  vi.mocked(api.saveProject).mockResolvedValue(project);
  vi.mocked(api.getOperations).mockResolvedValue([]);
  vi.mocked(api.refreshProject).mockResolvedValue(project);
});
afterEach(cleanup);

describe('Discovery renderer', () => {
  it('preserves trailing whitespace in a folder chosen by the native picker', async () => {
    vi.mocked(open).mockResolvedValue('/work/trailing ');
    render(<DiscoveryView onProjectCreated={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Choose folder' }));
    await waitFor(() => expect((screen.getByLabelText('Root folder') as HTMLInputElement).value).toBe('/work/trailing '));
    await scan();
    expect(api.discoverRepositories).toHaveBeenCalledWith('/work/trailing ', 3);
  });

  it('blocks launch navigation until creation and the project callback have completed', async () => {
    vi.mocked(save).mockResolvedValue('/work/chosen.git-view.json');
    let resolveCreate!: (value: Project) => void;
    vi.mocked(api.createProject).mockReturnValue(new Promise(resolve => { resolveCreate = resolve; }));
    const onProjectCreated = vi.fn(() => { expect(navigationBlocked()).toBe(true); });
    render(<DiscoveryView initialRoot="/work" onProjectCreated={onProjectCreated} onCancel={vi.fn()} />);
    await scan(); fireEvent.click(checkbox('Include API'));
    fireEvent.click(screen.getByRole('button', { name: 'Save selected as project (1)' }));
    await waitFor(() => expect(api.createProject).toHaveBeenCalledTimes(1));
    expect(navigationBlocked()).toBe(true);
    expect(onProjectCreated).not.toHaveBeenCalled();
    await act(async () => resolveCreate(project));
    expect(onProjectCreated).toHaveBeenCalledWith(project);
    expect(navigationBlocked()).toBe(false);
  });

  it('reviews partial results and saves only explicitly selected repositories', async () => {
    vi.mocked(api.discoverRepositories).mockResolvedValue({ ...result, truncated: true, issues: [{ path: '/work/private', reason: 'Permission denied' }] });
    vi.mocked(save).mockResolvedValue('/work/chosen.git-view.json');
    const onProjectCreated = vi.fn();
    render(<DiscoveryView initialRoot="/work" onProjectCreated={onProjectCreated} onCancel={vi.fn()} />);
    await scan();
    expect(api.discoverRepositories).toHaveBeenCalledWith('/work', 3);
    expect(screen.getByText(/Partial results/)).toBeTruthy();
    expect(screen.getByText(/Permission denied/)).toBeTruthy();
    expect(checkbox('Include API').checked).toBe(false);
    expect(api.createProject).not.toHaveBeenCalled();
    fireEvent.click(checkbox('Include Web'));
    fireEvent.change(screen.getByLabelText('Project name'), { target: { value: 'Chosen repositories' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save selected as project (1)' }));
    await waitFor(() => expect(api.createProject).toHaveBeenCalledWith('/work/chosen.git-view.json', { version: 1, name: 'Chosen repositories', repositories: [{ path: '/work/web', label: 'Web' }] }));
    expect(onProjectCreated).toHaveBeenCalledWith(project);
  });

  it('retains checked results on rescan and proposes new repositories unchecked', async () => {
    render(<DiscoveryView initialRoot="/work" onProjectCreated={vi.fn()} onCancel={vi.fn()} />);
    await scan();
    fireEvent.click(checkbox('Include API'));
    vi.mocked(api.discoverRepositories).mockResolvedValue({ ...result, candidates: [...result.candidates, { repositoryId: 'new', label: 'New', displayPath: '/work/new' }] });
    fireEvent.click(screen.getByRole('button', { name: 'Rescan' }));
    await screen.findByRole('checkbox', { name: 'Include New' });
    expect(checkbox('Include API').checked).toBe(true);
    expect(checkbox('Include Web').checked).toBe(false);
    expect(checkbox('Include New').checked).toBe(false);
    expect(api.createProject).not.toHaveBeenCalled();
    fireEvent.click(checkbox('Select all discovered repositories'));
    expect(checkbox('Include New').checked).toBe(true);
  });

  it('rejects a stale scan after a root prop change and suppresses duplicate starts', async () => {
    let resolveScan!: (value: DiscoveryResult) => void;
    vi.mocked(api.discoverRepositories).mockReturnValue(new Promise(resolve => { resolveScan = resolve; }));
    const view = render(<DiscoveryView initialRoot="/work" onProjectCreated={vi.fn()} onCancel={vi.fn()} />);
    const start = screen.getByRole('button', { name: 'Scan repositories' });
    fireEvent.click(start); fireEvent.click(start);
    expect(api.discoverRepositories).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('status').textContent).toContain('Scanning repositories');
    view.rerender(<DiscoveryView initialRoot="/other" onProjectCreated={vi.fn()} onCancel={vi.fn()} />);
    await act(async () => resolveScan(result));
    expect(screen.queryByRole('checkbox', { name: 'Include API' })).toBeNull();
    expect((screen.getByLabelText('Root folder') as HTMLInputElement).value).toBe('/other');
  });

  it('resets selection after editing root and ignores a result after depth changes', async () => {
    render(<DiscoveryView initialRoot="/work" onProjectCreated={vi.fn()} onCancel={vi.fn()} />);
    await scan(); fireEvent.click(checkbox('Include API'));
    fireEvent.change(screen.getByLabelText('Root folder'), { target: { value: '/another' } });
    await scan(); expect(checkbox('Include API').checked).toBe(false);
    let resolveScan!: (value: DiscoveryResult) => void;
    vi.mocked(api.discoverRepositories).mockReturnValue(new Promise(resolve => { resolveScan = resolve; }));
    fireEvent.click(screen.getByRole('button', { name: 'Rescan' }));
    fireEvent.change(screen.getByLabelText('Search depth'), { target: { value: '0' } });
    await act(async () => resolveScan(result));
    expect(screen.queryByRole('checkbox', { name: 'Include API' })).toBeNull();
  });

  it('shows scan failures and validates depth before contacting the backend', async () => {
    vi.mocked(api.discoverRepositories).mockRejectedValue(new Error('Root folder is inaccessible'));
    render(<DiscoveryView initialRoot="/work" onProjectCreated={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.change(screen.getByLabelText('Search depth'), { target: { value: '11' } });
    expect((screen.getByRole('button', { name: 'Scan repositories' }) as HTMLButtonElement).disabled).toBe(true);
    expect(api.discoverRepositories).not.toHaveBeenCalled();
    fireEvent.change(screen.getByLabelText('Search depth'), { target: { value: '0' } });
    fireEvent.click(screen.getByRole('button', { name: 'Scan repositories' }));
    expect(await screen.findByRole('alert')).toHaveProperty('textContent', 'Scan failed: Root folder is inaccessible');
    expect(api.discoverRepositories).toHaveBeenCalledWith('/work', 0);
    expect(api.createProject).not.toHaveBeenCalled();
  });

  it('does not write when the save dialog is canceled and exposes native-dialog state', async () => {
    vi.mocked(open).mockResolvedValue('/work');
    let resolveSave!: (path: string | null) => void;
    vi.mocked(save).mockReturnValue(new Promise(resolve => { resolveSave = resolve; }));
    render(<DiscoveryView onProjectCreated={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Choose folder' }));
    await waitFor(() => expect((screen.getByLabelText('Root folder') as HTMLInputElement).value).toBe('/work'));
    expect(navigationBlocked()).toBe(false);
    await scan(); fireEvent.click(checkbox('Include API'));
    fireEvent.click(screen.getByRole('button', { name: 'Save selected as project (1)' }));
    expect(navigationBlocked()).toBe(true);
    await act(async () => resolveSave(null));
    expect(navigationBlocked()).toBe(false);
    expect(api.createProject).not.toHaveBeenCalled(); expect(checkbox('Include API').checked).toBe(true);
  });

  it('suppresses duplicate saves while retaining reviewed selection after a write failure', async () => {
    let resolveSave!: (path: string) => void;
    vi.mocked(save).mockReturnValue(new Promise(resolve => { resolveSave = resolve; }));
    vi.mocked(api.createProject).mockRejectedValue(new Error('Project file already exists'));
    render(<DiscoveryView initialRoot="/work" onProjectCreated={vi.fn()} onCancel={vi.fn()} />);
    await scan(); fireEvent.click(checkbox('Include API'));
    const button = screen.getByRole('button', { name: 'Save selected as project (1)' });
    fireEvent.click(button); fireEvent.click(button);
    expect(save).toHaveBeenCalledTimes(1);
    await act(async () => resolveSave('/work/existing.json'));
    expect(await screen.findByRole('alert')).toHaveProperty('textContent', 'Project file already exists');
    expect(api.createProject).toHaveBeenCalledTimes(1); expect(checkbox('Include API').checked).toBe(true);
  });

  it('adds chosen discoveries to an existing draft while preserving name, labels and revision', async () => {
    render(<GroupView project={project} onOpenRepository={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Edit project' }));
    fireEvent.change(screen.getByLabelText('Project name'), { target: { value: 'Unsaved name' } });
    fireEvent.change(screen.getByLabelText('Label 1'), { target: { value: 'Unsaved label' } });
    fireEvent.click(screen.getByRole('button', { name: 'Discover repositories' }));
    fireEvent.change(screen.getByLabelText('Root folder'), { target: { value: '/work' } });
    await scan();
    expect(checkbox('Include API').disabled).toBe(true);
    fireEvent.click(checkbox('Select all discovered repositories'));
    expect(checkbox('Include Web').checked).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: 'Add selected to draft (1)' }));
    expect((screen.getByLabelText('Project name') as HTMLInputElement).value).toBe('Unsaved name');
    expect((screen.getByLabelText('Label 1') as HTMLInputElement).value).toBe('Unsaved label');
    expect((screen.getByLabelText('Label 2') as HTMLInputElement).value).toBe('Web');
    expect(api.saveProject).not.toHaveBeenCalled(); expect(api.createProject).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'Save project' }));
    await waitFor(() => expect(api.saveProject).toHaveBeenCalledWith('work', 'revision-1', { version: 1, name: 'Unsaved name', repositories: [{ path: './api', label: 'Unsaved label' }, { path: '/work/web', label: 'Web' }] }));
  });

  it('excludes previously added draft repositories and cancels a new scan without saving', async () => {
    render(<GroupView project={project} onOpenRepository={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Discover repositories' }));
    fireEvent.change(screen.getByLabelText('Root folder'), { target: { value: '/work' } });
    await scan(); fireEvent.click(checkbox('Include Web'));
    fireEvent.click(screen.getByRole('button', { name: 'Add selected to draft (1)' }));
    fireEvent.change(screen.getByLabelText('Label 2'), { target: { value: 'Pending web label' } });
    vi.mocked(api.discoverRepositories).mockResolvedValue({ ...result, candidates: [...result.candidates, { repositoryId: 'new', displayPath: '/work/new', label: 'New' }] });
    fireEvent.click(screen.getByRole('button', { name: 'Discover repositories' }));
    fireEvent.change(screen.getByLabelText('Root folder'), { target: { value: '/work' } });
    await scan();
    expect(checkbox('Include Web').disabled).toBe(true);
    expect(checkbox('Include New').checked).toBe(false);
    fireEvent.click(checkbox('Include New'));
    fireEvent.click(screen.getByRole('button', { name: 'Cancel discovery' }));
    expect((screen.getByLabelText('Label 2') as HTMLInputElement).value).toBe('Pending web label');
    expect(screen.queryByLabelText('Label 3')).toBeNull();
    expect(api.saveProject).not.toHaveBeenCalled(); expect(api.createProject).not.toHaveBeenCalled();
  });
});
