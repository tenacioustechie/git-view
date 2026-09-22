import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { api } from '../../api/client';
import { fixtures } from '../../test/fixtures';
import { RepositoryView } from './RepositoryView';
import type { ChangedFile, RepositoryOpen } from '../../api/types';

vi.mock('../../api/client', () => ({
  errorMessage: (error: unknown) => error instanceof Error ? error.message : String(error),
  api: Object.fromEntries(['getStatus', 'getChanges', 'listRefs', 'listRemotes', 'getHistory', 'getCommit', 'getDiff', 'getOperations', 'planAction', 'runAction', 'listRewrites'].map(name => [name, vi.fn()])),
}));
const file: ChangedFile = { fileId: 'opaque-file', displayPath: 'file with spaces.txt', oldDisplayPath: null, changeKind: 'modified', indexStatus: 'M', worktreeStatus: 'M', untracked: false, conflicted: false, submodule: false };
let repository: RepositoryOpen;
let sequence = 0;
beforeEach(() => {
  vi.resetAllMocks();
  vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(600);
  vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(600);
  const id = `test-repository-${sequence++}`;
  repository = { repositoryId: id, displayPath: '/disposable/repo', status: { ...fixtures.mixed, repositoryId: id } };
  vi.mocked(api.getStatus).mockResolvedValue(repository.status);
  vi.mocked(api.getChanges).mockResolvedValue({ snapshotId: 'changes-snapshot', files: [file] });
  vi.mocked(api.listRefs).mockResolvedValue([]);
  vi.mocked(api.listRemotes).mockResolvedValue([{ id: 'remote-token', name: 'origin', defaultFetch: true, defaultPush: true, destinationRef: 'refs/heads/main' }]);
  vi.mocked(api.getHistory).mockResolvedValue(fixtures.history);
  vi.mocked(api.getCommit).mockResolvedValue(fixtures.mergeCommit);
  vi.mocked(api.getDiff).mockResolvedValue({ kind: 'text', text: '+change', truncated: false });
  vi.mocked(api.getOperations).mockResolvedValue([]);
  vi.mocked(api.listRewrites).mockResolvedValue([]);
  vi.mocked(api.planAction).mockResolvedValue({ kind: 'eligible', planId: 'plan', expiresAt: '2030-01-01T00:00:00Z', summary: 'Fetch origin', destination: 'origin' });
  vi.mocked(api.runAction).mockResolvedValue({ ...fixtures.failedAction, repositoryId: id });
  HTMLDialogElement.prototype.showModal = function () { this.open = true; };
  HTMLDialogElement.prototype.close = function () { this.open = false; };
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

describe('repository workflows (mocked IPC)', () => {
  it('treats porcelain dot status as unchanged and keeps index/worktree lists separate', async () => {
    vi.mocked(api.getChanges).mockResolvedValue({ snapshotId: 'changes', files: [
      { ...file, fileId: 'staged-only', displayPath: 'staged.txt', worktreeStatus: '.' },
      { ...file, fileId: 'unstaged-only', displayPath: 'unstaged.txt', indexStatus: '.' },
    ] });
    render(<RepositoryView repository={repository} />);
    expect(await screen.findAllByRole('button', { name: 'staged.txt' })).toHaveLength(1);
    expect(await screen.findAllByRole('button', { name: 'unstaged.txt' })).toHaveLength(1);
    expect(screen.getAllByRole('button', { name: 'Stage' })).toHaveLength(1);
    expect(screen.getAllByRole('button', { name: 'Unstage' })).toHaveLength(1);
  });
  it('shows both halves of a partially staged file and stages only its opaque token', async () => {
    render(<RepositoryView repository={repository} />);
    await screen.findByRole('button', { name: 'Stage' });
    expect(screen.getAllByRole('button', { name: file.displayPath })).toHaveLength(2);
    fireEvent.click(screen.getByRole('button', { name: 'Stage' }));
    await waitFor(() => expect(api.planAction).toHaveBeenCalledWith(repository.repositoryId, { kind: 'stage', snapshotId: 'changes-snapshot', fileIds: ['opaque-file'] }));
    await waitFor(() => expect(api.runAction).toHaveBeenCalledWith('plan'));
  });
  it('retains the commit message after hook failure and leaving/reopening the repository', async () => {
    const view = render(<RepositoryView repository={repository} />);
    const input = await screen.findByLabelText('Commit message');
    fireEvent.change(input, { target: { value: 'Keep this draft after a hook failure' } });
    fireEvent.click(screen.getByRole('button', { name: 'Commit staged changes (1)' }));
    await waitFor(() => expect(api.runAction).toHaveBeenCalled());
    await screen.findAllByText(/Hook rejected commit/);
    expect((screen.getByLabelText('Commit message') as HTMLTextAreaElement).value).toBe('Keep this draft after a hook failure');
    view.unmount();
    render(<RepositoryView repository={repository} />);
    expect(((await screen.findByLabelText('Commit message')) as HTMLTextAreaElement).value).toBe('Keep this draft after a hook failure');
  });
  it('requires review of the resolved remote before a remote operation runs', async () => {
    render(<RepositoryView repository={repository} />);
    await waitFor(() => expect((screen.getByRole('button', { name: 'Fetch' }) as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(screen.getByRole('button', { name: 'Fetch' }));
    const preview = await screen.findByRole('dialog');
    expect(within(preview).getByText('origin')).toBeTruthy();
    expect(api.runAction).not.toHaveBeenCalled();
    fireEvent.click(within(preview).getByRole('button', { name: 'Run fetch' }));
    await waitFor(() => expect(api.runAction).toHaveBeenCalledWith('plan'));
  });
  it('uses the selected diff context for staged and unstaged versions', async () => {
    render(<RepositoryView repository={repository} />);
    const buttons = await screen.findAllByRole('button', { name: file.displayPath });
    fireEvent.click(buttons[0]);
    await waitFor(() => expect(api.getDiff).toHaveBeenLastCalledWith({ repositoryId: repository.repositoryId, snapshotId: 'changes-snapshot', fileId: file.fileId, context: 'staged' }));
    fireEvent.click(buttons[1]);
    await waitFor(() => expect(api.getDiff).toHaveBeenLastCalledWith({ repositoryId: repository.repositoryId, snapshotId: 'changes-snapshot', fileId: file.fileId, context: 'unstaged' }));
  });
  it('rejects a new page from another history snapshot', async () => {
    vi.mocked(api.getHistory).mockResolvedValueOnce(fixtures.history).mockResolvedValueOnce({ ...fixtures.historyNext, snapshotId: 'different' });
    render(<RepositoryView repository={repository} />);
    fireEvent.click(await screen.findByRole('button', { name: 'Load older commits' }));
    expect(await screen.findByRole('alert')).toHaveProperty('textContent', expect.stringContaining('History changed'));
    expect(api.getHistory).toHaveBeenLastCalledWith(repository.repositoryId, null, 'history', 'page-2');
  });
  it('inspects a merge against the explicitly selected parent', async () => {
    render(<RepositoryView repository={repository} />);
    fireEvent.click(await screen.findByRole('button', { name: /Merge feature/ }));
    const parents = await screen.findByLabelText('Compare with parent');
    fireEvent.change(parents, { target: { value: fixtures.mergeCommit.parents[1] } });
    await waitFor(() => expect(api.getCommit).toHaveBeenLastCalledWith(repository.repositoryId, fixtures.mergeCommit.oid, fixtures.mergeCommit.parents[1]));
  });
  it('previews separate ordinary merge and squash merge actions with source and target', async () => {
    vi.mocked(api.listRefs).mockResolvedValue([{ fullName: 'refs/heads/feature', label: 'feature', oid: 'feature-tip', kind: 'local' }]);
    vi.mocked(api.planAction).mockResolvedValue({ kind: 'eligible', planId: 'merge-plan', expiresAt: '2030-01-01T00:00:00Z', summary: 'Merge feature-tip into main-tip', destination: 'refs/heads/main' });
    render(<RepositoryView repository={repository} />);
    fireEvent.click(screen.getByText('Merge branches'));
    await within(screen.getByLabelText('Merge source branch')).findByRole('option', { name: 'feature' });
    fireEvent.change(screen.getByLabelText('Merge source branch'), { target: { value: 'refs/heads/feature' } });
    fireEvent.click(screen.getByRole('button', { name: 'Merge branch' }));
    const normalPreview = await screen.findByRole('dialog');
    expect(normalPreview.textContent).toContain('refs/heads/feature');
    expect(normalPreview.textContent).toContain('Current target: main');
    expect(api.planAction).toHaveBeenLastCalledWith(repository.repositoryId, { kind: 'merge', statusSnapshotId: repository.status.snapshotId, sourceRef: 'refs/heads/feature' });
    expect(api.runAction).not.toHaveBeenCalled();
    fireEvent.click(within(normalPreview).getByRole('button', { name: 'Cancel' }));
    fireEvent.click(screen.getByRole('button', { name: 'Squash merge branch' }));
    const squashPreview = await screen.findByRole('dialog');
    expect(squashPreview.textContent).toContain('result stays staged');
    expect(api.planAction).toHaveBeenLastCalledWith(repository.repositoryId, { kind: 'squashMerge', statusSnapshotId: repository.status.snapshotId, sourceRef: 'refs/heads/feature' });
    expect(api.runAction).not.toHaveBeenCalled();
  });
  it('exposes ordinary merge abort but prevents continuation while conflicts remain', async () => {
    repository.status = { ...repository.status, gitState: 'merge', counts: { staged: 0, unstaged: 0, untracked: 0, conflicted: 1 } };
    vi.mocked(api.getStatus).mockResolvedValue(repository.status);
    render(<RepositoryView repository={repository} />);
    expect((screen.getByRole('button', { name: 'Continue merge' }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: 'Abort merge' }));
    await screen.findByRole('dialog');
    expect(api.planAction).toHaveBeenLastCalledWith(repository.repositoryId, { kind: 'abortMerge', statusSnapshotId: repository.status.snapshotId });
    expect(api.runAction).not.toHaveBeenCalled();
  });
  it('continues a resolved ordinary merge through a fresh plan', async () => {
    repository.status = { ...repository.status, gitState: 'merge' };
    vi.mocked(api.getStatus).mockResolvedValue(repository.status);
    render(<RepositoryView repository={repository} />);
    fireEvent.click(screen.getByRole('button', { name: 'Continue merge' }));
    const preview = await screen.findByRole('dialog');
    fireEvent.click(within(preview).getByRole('button', { name: 'Run continueMerge' }));
    await waitFor(() => expect(api.runAction).toHaveBeenCalledWith('plan'));
    expect(api.planAction).toHaveBeenLastCalledWith(repository.repositoryId, { kind: 'continueMerge', statusSnapshotId: repository.status.snapshotId });
  });
  it('restores squash review from backend state and offers an explicit normal commit without ordinary abort', async () => {
    repository.status = { ...repository.status, gitState: 'squashMerge', squashMerge: { originalHead: 'original-tip', sourceOid: 'source-tip', sourceRef: 'refs/heads/feature' } };
    vi.mocked(api.getStatus).mockResolvedValue(repository.status);
    render(<RepositoryView repository={repository} />);
    expect(screen.getByText('Squash merge review')).toBeTruthy();
    expect(screen.getByRole('status').textContent).toContain('Original HEAD: original-tip');
    expect(screen.getByRole('status').textContent).toContain('does not create MERGE_HEAD');
    expect(screen.queryByRole('button', { name: 'Abort merge' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Continue merge' })).toBeNull();
    fireEvent.change(await screen.findByLabelText('Commit message'), { target: { value: 'Reviewed squash result' } });
    fireEvent.click(screen.getByRole('button', { name: 'Commit staged changes (1)' }));
    await waitFor(() => expect(api.planAction).toHaveBeenCalledWith(repository.repositoryId, { kind: 'commit', statusSnapshotId: repository.status.snapshotId, message: 'Reviewed squash result' }));
  });
  it('keeps combine message when backend rejects a stale preview and never executes it', async () => {
    repository.status = { ...repository.status, dirty: false };
    vi.mocked(api.getStatus).mockResolvedValue(repository.status);
    vi.mocked(api.getHistory).mockResolvedValue({ ...fixtures.history, commits: fixtures.history.commits.map(commit => ({ ...commit, parents: [commit.parents[0]] })) });
    vi.mocked(api.planAction).mockResolvedValue({ kind: 'blocked', reason: 'Branch changed after selection', error: null });
    render(<RepositoryView repository={repository} />);
    fireEvent.click(screen.getByRole('button', { name: 'Combine commits…' }));
    fireEvent.change(await screen.findByLabelText('Combined commit message'), { target: { value: 'Retain rejected combine message' } });
    fireEvent.click(await screen.findByRole('button', { name: 'Preview combine commits' }));
    expect(await screen.findByRole('alert')).toHaveProperty('textContent', expect.stringContaining('Branch changed'));
    expect(api.runAction).not.toHaveBeenCalled();
    expect((screen.getByLabelText('Combined commit message') as HTMLTextAreaElement).value).toBe('Retain rejected combine message');
  });
  it('retains combined message after execution failure and reopening the panel', async () => {
    repository.status = { ...repository.status, dirty: false };
    vi.mocked(api.getStatus).mockResolvedValue(repository.status);
    vi.mocked(api.getHistory).mockResolvedValue({ ...fixtures.history, commits: fixtures.history.commits.map(commit => ({ ...commit, parents: [commit.parents[0]] })) });
    vi.mocked(api.runAction).mockResolvedValue({ ...fixtures.failedAction, repositoryId: repository.repositoryId, action: 'combineCommits' });
    render(<RepositoryView repository={repository} />);
    fireEvent.click(screen.getByRole('button', { name: 'Combine commits…' }));
    fireEvent.change(await screen.findByLabelText('Combined commit message'), { target: { value: 'Retain failed combine message' } });
    fireEvent.click(await screen.findByRole('button', { name: 'Preview combine commits' }));
    const preview = await screen.findByRole('dialog');
    expect(preview.textContent).toContain('may already be published');
    fireEvent.click(within(preview).getByRole('button', { name: 'Run combineCommits' }));
    await screen.findAllByText(/Hook rejected commit/);
    fireEvent.click(screen.getByRole('button', { name: 'Close combine' }));
    fireEvent.click(screen.getByRole('button', { name: 'Combine commits…' }));
    expect(((await screen.findByLabelText('Combined commit message')) as HTMLTextAreaElement).value).toBe('Retain failed combine message');
  });
});
