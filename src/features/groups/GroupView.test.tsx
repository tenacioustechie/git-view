// Renderer behavior tests: all native IPC and dialogs are mocked explicitly.
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { api } from '../../api/client';
import { fixtures } from '../../test/fixtures';
import type { BatchPlan, OperationResult, Project, RepositoryRow } from '../../api/types';
import { GroupView } from './GroupView';
import { open, save } from '@tauri-apps/plugin-dialog';

vi.mock('../../api/client', () => ({ api: { refreshProject: vi.fn(), openProject: vi.fn(), saveProject: vi.fn(), createProject: vi.fn(), planBatch: vi.fn(), runBatch: vi.fn(), cancelBatch: vi.fn(), getOperations: vi.fn() }, errorMessage: (error: unknown) => error instanceof Error ? error.message : String(error) }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(), save: vi.fn() }));
const ready = (id: string, branch: string, dirty = false): RepositoryRow => ({ kind: 'ready', memberId: id, repositoryId: id, label: id, displayPath: `/workspace/${id}`, status: { ...(dirty ? fixtures.mixed : fixtures.clean), repositoryId: id, head: { kind: 'branch', name: branch, oid: 'abc' } } });
const project: Project = { projectId: 'project', revision: 'revision-1', displayPath: '/workspace/work.git-view.json', name: 'Work', content: { version: 1, name: 'Work', repositories: [{ path: './api' }, { path: './web' }, { path: './missing' }] }, members: [ready('api', 'main'), ready('web', 'feature', true), { kind: 'unavailable', memberId: 'missing', label: 'missing', displayPath: '/workspace/missing', error: { code: 'missing', message: 'Path does not exist', retryable: false } }], stale: false, error: null };
const plan: BatchPlan = { planId: 'plan-1', action: 'pull', expiresAt: '2099-01-01T00:00:00Z', candidates: [{ memberId: 'api', repositoryId: 'api', label: 'api', eligible: true, destination: 'origin/main', reason: null }, { memberId: 'web', repositoryId: 'web', label: 'web', eligible: false, destination: null, reason: 'Dirty worktree' }, { memberId: 'missing', repositoryId: null, label: 'missing', eligible: false, destination: null, reason: 'Path does not exist' }] };
const result = (id: string, state: OperationResult['state']): OperationResult => ({ ...fixtures.failedAction, operationId: `op-${id}`, batchId: 'batch-1', memberId: id, repositoryId: id, action: 'pull', state, error: state === 'failed' ? { code: 'diverged', message: 'Fast-forward required', retryable: true } : null, reason: state === 'skipped' ? 'Dirty worktree' : null });
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.refreshProject).mockResolvedValue(project);
  vi.mocked(api.getOperations).mockResolvedValue([]);
  vi.mocked(api.planBatch).mockResolvedValue(plan);
  vi.mocked(api.runBatch).mockResolvedValue({ batchId: 'batch-1', operationIds: [] });
});
afterEach(cleanup);

describe('GroupView renderer', () => {
  it('previews every matching member including unavailable and dirty rows; does not run before confirmation', async () => {
    render(<GroupView project={project} onOpenRepository={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Pull all (3)' }));
    const preview = await screen.findByRole('region', { name: 'Batch preview' });
    expect(api.planBatch).toHaveBeenCalledWith({ projectId: 'project', revision: 'revision-1', memberIds: ['api', 'web', 'missing'], action: 'pull', branchFilter: null });
    expect(within(preview).getAllByRole('listitem')).toHaveLength(3);
    expect(preview.textContent).toContain('Dirty worktree');
    expect(preview.textContent).toContain('Path does not exist');
    expect(api.runBatch).not.toHaveBeenCalled();
    fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'web' } });
    expect(within(preview).getAllByRole('listitem')).toHaveLength(3);
    fireEvent.click(within(preview).getByRole('button', { name: 'Run 1 eligible' }));
    await waitFor(() => expect(api.runBatch).toHaveBeenCalledWith('plan-1'));
  });

  it('uses exact branch scope for Pull all and includes hidden checks in Pull selected', async () => {
    render(<GroupView project={project} onOpenRepository={vi.fn()} />);
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select all matching' }));
    fireEvent.change(screen.getByLabelText('Branch'), { target: { value: 'main' } });
    expect(screen.getByText('3 selected · 2 hidden by filters')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Pull all (1)' }));
    await waitFor(() => expect(api.planBatch).toHaveBeenLastCalledWith(expect.objectContaining({ memberIds: ['api'], branchFilter: 'main' })));
    await screen.findByRole('button', { name: 'Close preview' });
    fireEvent.click(screen.getByRole('button', { name: 'Close preview' }));
    fireEvent.click(screen.getByRole('button', { name: 'Pull selected (3)' }));
    await waitFor(() => expect(api.planBatch).toHaveBeenLastCalledWith(expect.objectContaining({ memberIds: ['api', 'web', 'missing'], branchFilter: null })));
  });

  it('opens a repository with real status and preserves checks on a refreshed project prop', async () => {
    const onOpenRepository = vi.fn();
    const view = render(<GroupView project={project} onOpenRepository={onOpenRepository} />);
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select api' }));
    fireEvent.click(screen.getByRole('button', { name: 'api' }));
    expect(onOpenRepository).toHaveBeenCalledWith(expect.objectContaining({ repositoryId: 'api', displayPath: '/workspace/api' }));
    view.rerender(<GroupView project={{ ...project, members: [...project.members] }} onOpenRepository={onOpenRepository} />);
    expect((screen.getByRole('checkbox', { name: 'Select api' }) as HTMLInputElement).checked).toBe(true);
  });

  it('recovers independent activity and retries only chosen failures using a fresh plan', async () => {
    vi.mocked(api.getOperations).mockResolvedValue([result('api', 'succeeded'), result('web', 'failed'), result('missing', 'skipped')]);
    render(<GroupView project={project} onOpenRepository={vi.fn()} />);
    fireEvent.click(await screen.findByRole('checkbox', { name: 'Retry web' }));
    fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'api' } });
    const activity = screen.getByRole('region', { name: 'Batch activity' });
    expect(within(activity).getByText('Fast-forward required')).toBeTruthy();
    expect(within(activity).getByText('succeeded')).toBeTruthy();
    fireEvent.click(within(activity).getByRole('button', { name: 'Preview retry (1)' }));
    await waitFor(() => expect(api.planBatch).toHaveBeenCalledWith(expect.objectContaining({ memberIds: ['web'], branchFilter: null })));
    expect(api.runBatch).not.toHaveBeenCalled();
  });

  it('cancels a running batch and keeps its results visible', async () => {
    vi.mocked(api.getOperations).mockResolvedValue([result('api', 'running'), result('web', 'queued')]);
    render(<GroupView project={project} onOpenRepository={vi.fn()} />);
    fireEvent.click(await screen.findByRole('button', { name: 'Cancel batch' }));
    await waitFor(() => expect(api.cancelBatch).toHaveBeenCalledWith('batch-1'));
    expect(screen.getByRole('region', { name: 'Batch activity' }).textContent).toContain('running');
  });

  it('saves against the revision editing started from and retains the draft after a conflict', async () => {
    vi.mocked(api.saveProject).mockRejectedValue(new Error('Project changed externally'));
    const view = render(<GroupView project={project} onOpenRepository={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Edit project' }));
    fireEvent.change(screen.getByLabelText('Project name'), { target: { value: 'Renamed' } });
    view.rerender(<GroupView project={{ ...project, revision: 'revision-2' }} onOpenRepository={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Save project' }));
    await waitFor(() => expect(api.saveProject).toHaveBeenCalledWith('project', 'revision-1', expect.objectContaining({ name: 'Renamed' })));
    await screen.findByRole('button', { name: 'Reload project' });
    expect((screen.getByLabelText('Project name') as HTMLInputElement).value).toBe('Renamed');
    expect((screen.getByRole('button', { name: 'Pull all (3)' }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('retains the last dashboard and blocks new batches for malformed external edits', () => {
    render(<GroupView project={{ ...project, stale: true, error: { code: 'invalid_project', message: 'Malformed project JSON', retryable: false } }} onOpenRepository={vi.fn()} />);
    expect(screen.getByRole('alert').textContent).toContain('Malformed project JSON');
    expect(screen.getByRole('button', { name: 'api' })).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Pull all (3)' }) as HTMLButtonElement).disabled).toBe(true);
    expect(api.planBatch).not.toHaveBeenCalled();
  });

  it('adds repositories through the native folder picker and preserves order and labels on save', async () => {
    vi.mocked(open).mockResolvedValue(['/workspace/new checkout']);
    vi.mocked(api.saveProject).mockResolvedValue(project);
    render(<GroupView project={project} onOpenRepository={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Add repositories' }));
    await screen.findByLabelText('Label 4');
    fireEvent.change(screen.getByLabelText('Label 4'), { target: { value: 'New checkout' } });
    fireEvent.click(screen.getByRole('button', { name: 'Remove 2' }));
    fireEvent.click(screen.getByRole('button', { name: 'Save project' }));
    await waitFor(() => expect(api.saveProject).toHaveBeenCalledWith('project', 'revision-1', {
      version: 1, name: 'Work', repositories: [{ path: './api' }, { path: './missing' }, { path: '/workspace/new checkout', label: 'New checkout' }],
    }));
  });

  it('preserves repository locations when saving a project in a different directory', async () => {
    vi.mocked(save).mockResolvedValue('/elsewhere/copied.git-view.json');
    vi.mocked(api.createProject).mockResolvedValue({ ...project, projectId: 'copy', displayPath: '/elsewhere/copied.git-view.json' });
    const onProjectChange = vi.fn();
    render(<GroupView project={project} onOpenRepository={vi.fn()} onProjectChange={onProjectChange} />);
    fireEvent.click(screen.getByRole('button', { name: 'Edit project' }));
    fireEvent.click(screen.getByRole('button', { name: 'Save as' }));
    await waitFor(() => expect(api.createProject).toHaveBeenCalledWith('/elsewhere/copied.git-view.json', {
      version: 1, name: 'Work', repositories: [{ path: '/workspace/api' }, { path: '/workspace/web' }, { path: '/workspace/missing' }],
    }));
    expect(onProjectChange).toHaveBeenCalledWith(expect.objectContaining({ projectId: 'copy' }));
  });

  it('invalidates a pending preview after an external project revision change', async () => {
    const view = render(<GroupView project={project} onOpenRepository={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Pull all (3)' }));
    await screen.findByRole('region', { name: 'Batch preview' });
    view.rerender(<GroupView project={{ ...project, revision: 'new-revision' }} onOpenRepository={vi.fn()} />);
    await waitFor(() => expect(screen.queryByRole('region', { name: 'Batch preview' })).toBeNull());
    expect(api.runBatch).not.toHaveBeenCalled();
  });

  it('does not resurrect a late batch preview after the project changes', async () => {
    let resolvePlan!: (value: BatchPlan) => void;
    vi.mocked(api.planBatch).mockReturnValue(new Promise(resolve => { resolvePlan = resolve; }));
    const view = render(<GroupView project={project} onOpenRepository={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Pull all (3)' }));
    view.rerender(<GroupView project={{ ...project, revision: 'new-revision', stale: true }} onOpenRepository={vi.fn()} />);
    await act(async () => resolvePlan(plan));
    expect(screen.queryByRole('region', { name: 'Batch preview' })).toBeNull();
    expect(api.runBatch).not.toHaveBeenCalled();
  });

  it('uses draft-start paths for Save as even after externally changed membership', async () => {
    vi.mocked(save).mockResolvedValue('/elsewhere/copied.git-view.json');
    vi.mocked(api.createProject).mockResolvedValue({ ...project, projectId: 'copy' });
    const view = render(<GroupView project={project} onOpenRepository={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Edit project' }));
    view.rerender(<GroupView project={{ ...project, revision: 'new', content: { version: 1, name: 'Work', repositories: [] }, members: [] }} onOpenRepository={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Save as' }));
    await waitFor(() => expect(api.createProject).toHaveBeenCalledWith('/elsewhere/copied.git-view.json', expect.objectContaining({ repositories: [{ path: '/workspace/api' }, { path: '/workspace/web' }, { path: '/workspace/missing' }] })));
  });

  it('does not restore the old project when a refresh finishes after Save as', async () => {
    let resolveRefresh!: (value: Project) => void;
    vi.mocked(api.refreshProject).mockReturnValue(new Promise(resolve => { resolveRefresh = resolve; }));
    vi.mocked(save).mockResolvedValue('/elsewhere/copied.git-view.json');
    vi.mocked(api.createProject).mockResolvedValue({ ...project, projectId: 'copy', name: 'Copied project' });
    const onProjectChange = vi.fn();
    render(<GroupView project={project} onOpenRepository={vi.fn()} onProjectChange={onProjectChange} />);
    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    fireEvent.click(screen.getByRole('button', { name: 'Edit project' }));
    fireEvent.click(screen.getByRole('button', { name: 'Save as' }));
    await screen.findByRole('heading', { name: 'Copied project' });
    await act(async () => resolveRefresh(project));
    expect(screen.getByRole('heading', { name: 'Copied project' })).toBeTruthy();
    expect(onProjectChange).toHaveBeenCalledTimes(1);
  });
});
