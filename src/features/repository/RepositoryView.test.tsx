import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { api } from '../../api/client';
import { fixtures } from '../../test/fixtures';
import { RepositoryView } from './RepositoryView';
import type { ChangedFile, RepositoryOpen } from '../../api/types';

vi.mock('../../api/client', () => ({
  errorMessage: (error: unknown) => error instanceof Error ? error.message : String(error),
  api: Object.fromEntries(['getStatus', 'getChanges', 'listRefs', 'listRemotes', 'getHistory', 'getCommit', 'getDiff', 'getOperations', 'planAction', 'runAction'].map(name => [name, vi.fn()])),
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
});
