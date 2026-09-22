import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { api } from '../../api/client';
import type { ChangedFile, CommitDetails, DiffResult, RepositoryOpen, StashList } from '../../api/types';
import { fixtures } from '../../test/fixtures';
import { RepositoryView } from './RepositoryView';

vi.mock('../../api/client', () => ({ errorMessage: (error: unknown) => error instanceof Error ? error.message : String(error), api: Object.fromEntries(['getStatus', 'getChanges', 'listRefs', 'listRemotes', 'getHistory', 'getOperations', 'planAction', 'runAction', 'listRewrites', 'listStashes', 'getStashDetails', 'getDiff'].map(name => [name, vi.fn()])) }));
const file: ChangedFile = { fileId: 'opaque-stash-file', displayPath: 'saved.txt', oldDisplayPath: null, changeKind: 'modified', indexStatus: null, worktreeStatus: null, untracked: false, conflicted: false, submodule: false };
const list: StashList = { snapshotId: 'stash-list-1', entries: [
  { entryId: 'opaque-first', oid: 'same-object-id', message: 'Newer saved changes', createdAt: '2026-09-20', branch: 'main', hasUntracked: true, unavailableReason: null },
  { entryId: 'opaque-second', oid: 'same-object-id', message: 'Older saved changes', createdAt: '2026-09-19', branch: null, hasUntracked: false, unavailableReason: null },
], deleteUnavailableReason: null, recovery: null };
let repository: RepositoryOpen;
let sequence = 0;
beforeEach(() => {
  vi.resetAllMocks();
  repository = { repositoryId: `stash-${sequence++}`, displayPath: '/fixture/stash', status: { ...fixtures.clean } };
  repository.status.repositoryId = repository.repositoryId;
  vi.mocked(api.getStatus).mockResolvedValue(repository.status);
  vi.mocked(api.getChanges).mockResolvedValue({ snapshotId: 'changes', files: [] });
  vi.mocked(api.listRefs).mockResolvedValue([]);
  vi.mocked(api.listRemotes).mockResolvedValue([]);
  vi.mocked(api.getHistory).mockResolvedValue(fixtures.history);
  vi.mocked(api.getOperations).mockResolvedValue([]);
  vi.mocked(api.listRewrites).mockResolvedValue([]);
  vi.mocked(api.listStashes).mockResolvedValue(list);
  vi.mocked(api.getStashDetails).mockResolvedValue({ ...fixtures.mergeCommit, snapshotId: 'saved-worktree', files: [file] });
  vi.mocked(api.getDiff).mockResolvedValue({ kind: 'text', text: '+saved patch', truncated: false });
  vi.mocked(api.planAction).mockResolvedValue({ kind: 'eligible', planId: 'stash-plan', expiresAt: '', summary: 'Reviewed exact stash entry and current branch main', destination: null });
  vi.mocked(api.runAction).mockResolvedValue({ ...fixtures.failedAction, repositoryId: repository.repositoryId, action: 'applyStash', state: 'succeeded', error: null });
  HTMLDialogElement.prototype.showModal = function () { this.open = true; };
  HTMLDialogElement.prototype.close = function () { this.open = false; };
});
afterEach(cleanup);
async function open() {
  render(<RepositoryView repository={repository} />);
  await screen.findByRole('button', { name: 'Refresh' });
  fireEvent.click(screen.getByRole('button', { name: 'Stashes' }));
  await screen.findByRole('button', { name: /Newer saved changes/ });
}
async function select(older = false) {
  fireEvent.click(screen.getByRole('button', { name: older ? /Older saved changes/ : /Newer saved changes/ }));
  return screen.findByRole('button', { name: /modified · saved.txt/ });
}

describe('stash workspace (mocked IPC)', () => {
  it('defaults untracked inclusion off and retains the message after rejected creation while refreshing state', async () => {
    repository.status = { ...repository.status, dirty: true, counts: { staged: 1, unstaged: 1, untracked: 1, conflicted: 0 } };
    vi.mocked(api.getStatus).mockResolvedValue(repository.status);
    vi.mocked(api.planAction).mockResolvedValue({ kind: 'blocked', reason: 'Working files changed; review again', error: null });
    await open();
    expect((screen.getByLabelText('Include untracked files') as HTMLInputElement).checked).toBe(false);
    expect(screen.getByLabelText('Create stash').textContent).toContain('Untracked files remain in place');
    fireEvent.change(screen.getByLabelText('Stash message'), { target: { value: 'Keep create message' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create stash' }));
    await screen.findByText('Working files changed; review again');
    expect(api.planAction).toHaveBeenCalledWith(repository.repositoryId, { kind: 'createStash', statusSnapshotId: repository.status.snapshotId, message: 'Keep create message', includeUntracked: false });
    expect(api.getStatus).toHaveBeenCalledTimes(2);
    expect((screen.getByLabelText('Stash message') as HTMLInputElement).value).toBe('Keep create message');
    expect(api.runAction).not.toHaveBeenCalled();
  });
  it('requires explicit opt-in before an untracked-only stash can be created', async () => {
    repository.status = { ...repository.status, dirty: true, counts: { staged: 0, unstaged: 0, untracked: 1, conflicted: 0 } };
    vi.mocked(api.getStatus).mockResolvedValue(repository.status);
    await open();
    fireEvent.change(screen.getByLabelText('Stash message'), { target: { value: 'Untracked choice' } });
    expect((screen.getByRole('button', { name: 'Create stash' }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByLabelText('Include untracked files'));
    fireEvent.click(screen.getByRole('button', { name: 'Create stash' }));
    await screen.findByRole('dialog');
    expect(api.planAction).toHaveBeenCalledWith(repository.repositoryId, { kind: 'createStash', statusSnapshotId: repository.status.snapshotId, message: 'Untracked choice', includeUntracked: true });
  });
  it('inspects tracked/index/untracked sections using exact list and opaque file snapshots', async () => {
    await open();
    fireEvent.click(await select());
    expect(await screen.findByText('+saved patch')).toBeTruthy();
    expect(api.getStashDetails).toHaveBeenLastCalledWith(repository.repositoryId, 'stash-list-1', 'opaque-first', 'worktree');
    expect(api.getDiff).toHaveBeenCalledWith({ repositoryId: repository.repositoryId, snapshotId: 'saved-worktree', fileId: file.fileId, context: 'commit' });
    for (const section of ['index', 'untracked']) {
      fireEvent.change(screen.getByLabelText('Stash section'), { target: { value: section } });
      await waitFor(() => expect(api.getStashDetails).toHaveBeenLastCalledWith(repository.repositoryId, 'stash-list-1', 'opaque-first', section));
    }
  });
  it('disables absent untracked inspection without blocking Apply or Delete', async () => {
    vi.mocked(api.getStashDetails).mockImplementation(async (_id, _snapshot, _entry, section) => {
      if (section === 'untracked') throw new Error('no saved untracked files');
      return { ...fixtures.mergeCommit, files: [file] };
    });
    await open(); await select(true);
    expect((screen.getByRole('option', { name: 'Saved untracked files' }) as HTMLOptionElement).disabled).toBe(true);
    expect(screen.getByText('This stash contains no saved untracked files.')).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Apply stash' }) as HTMLButtonElement).disabled).toBe(false);
    expect((screen.getByRole('button', { name: 'Delete stash' }) as HTMLButtonElement).disabled).toBe(false);
    expect(api.getStashDetails).toHaveBeenCalledTimes(1);
  });
  it('applies while retaining the entry with staged restoration explicitly off by default', async () => {
    await open(); await select();
    expect((screen.getByLabelText('Restore staged state') as HTMLInputElement).checked).toBe(false);
    expect(screen.getByLabelText('Selected stash').textContent).toContain('some newly added files may be staged');
    fireEvent.click(screen.getByRole('button', { name: 'Apply stash' }));
    const preview = await screen.findByRole('dialog');
    expect(preview.textContent).toContain('Apply retains the saved stash');
    expect(api.planAction).toHaveBeenLastCalledWith(repository.repositoryId, { kind: 'applyStash', statusSnapshotId: repository.status.snapshotId, stashSnapshotId: 'stash-list-1', entryId: 'opaque-first', restoreIndex: false });
    vi.mocked(api.getStatus).mockResolvedValue({ ...repository.status, dirty: true });
    fireEvent.click(within(preview).getByRole('button', { name: 'Run apply stash' }));
    await waitFor(() => expect(api.getStatus).toHaveBeenCalledTimes(2));
    expect(screen.getByRole('button', { name: /Newer saved changes/ })).toBeTruthy();
    expect(screen.getByLabelText('Selected stash').textContent).toContain('clean destination');
  });
  it('uses explicit Restore staged state and exact non-top token even when object IDs repeat; cancel deletes nothing', async () => {
    await open(); await select(true);
    fireEvent.click(screen.getByLabelText('Restore staged state'));
    fireEvent.click(screen.getByRole('button', { name: 'Apply stash' }));
    const apply = await screen.findByRole('dialog');
    expect(api.planAction).toHaveBeenLastCalledWith(repository.repositoryId, expect.objectContaining({ entryId: 'opaque-second', restoreIndex: true }));
    fireEvent.click(within(apply).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(api.listStashes).toHaveBeenCalledTimes(2));
    await select(true);
    fireEvent.click(screen.getByRole('button', { name: 'Delete stash' }));
    const preview = await screen.findByRole('dialog');
    expect(preview.textContent).toContain('recovery backup is retained');
    expect(api.planAction).toHaveBeenLastCalledWith(repository.repositoryId, { kind: 'deleteStash', statusSnapshotId: repository.status.snapshotId, stashSnapshotId: 'stash-list-1', entryId: 'opaque-second' });
    fireEvent.click(within(preview).getByRole('button', { name: 'Cancel' }));
    expect(api.runAction).not.toHaveBeenCalled();
  });
  it('rejects a late mutation preview after selecting a different saved entry', async () => {
    let finish!: (value: Awaited<ReturnType<typeof api.planAction>>) => void;
    vi.mocked(api.planAction).mockImplementation(() => new Promise(resolve => { finish = resolve; }));
    await open(); await select(true);
    fireEvent.click(screen.getByRole('button', { name: 'Delete stash' }));
    await select();
    await act(async () => finish({ kind: 'eligible', planId: 'old-selection', expiresAt: '', summary: 'Old entry', destination: null }));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.getByRole('alert').textContent).toContain('create a new preview');
    expect(api.runAction).not.toHaveBeenCalled();
  });
  it('suppresses stale inspection results after selecting another entry', async () => {
    let finish!: (value: CommitDetails) => void;
    vi.mocked(api.getStashDetails).mockImplementation((_id, _snapshot, entryId) => entryId === 'opaque-first' ? new Promise(resolve => { finish = resolve; }) : Promise.resolve({ ...fixtures.mergeCommit, files: [file] }));
    await open();
    fireEvent.click(screen.getByRole('button', { name: /Newer saved changes/ }));
    await waitFor(() => expect(api.getStashDetails).toHaveBeenCalled());
    await select(true);
    await act(async () => finish({ ...fixtures.mergeCommit, files: [{ ...file, displayPath: 'stale.txt' }] }));
    expect(screen.queryByRole('button', { name: /stale.txt/ })).toBeNull();
    expect(screen.getByRole('button', { name: /saved.txt/ })).toBeTruthy();
  });
  it('retains exact selected identity and inspected section across an unchanged-list refresh', async () => {
    await open(); await select(true);
    fireEvent.change(screen.getByLabelText('Stash section'), { target: { value: 'index' } });
    await waitFor(() => expect(api.getStashDetails).toHaveBeenLastCalledWith(repository.repositoryId, 'stash-list-1', 'opaque-second', 'index'));
    vi.mocked(api.listStashes).mockResolvedValue({ ...list, snapshotId: 'stash-list-refreshed' });
    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    await waitFor(() => expect(api.getStashDetails).toHaveBeenLastCalledWith(repository.repositoryId, 'stash-list-refreshed', 'opaque-second', 'index'));
    expect(screen.getByLabelText('Selected stash').textContent).toContain('Older saved changes');
    expect((screen.getByLabelText('Stash section') as HTMLSelectElement).value).toBe('index');
    fireEvent.click(await screen.findByRole('button', { name: /modified · saved.txt/ }));
    expect(await screen.findByText('+saved patch')).toBeTruthy();
  });
  it('clears selection when reflog identity changes even with identical OIDs and messages', async () => {
    await open(); await select(true);
    vi.mocked(api.listStashes).mockResolvedValue({ ...list, snapshotId: 'changed-pair', entries: list.entries.map(entry => ({ ...entry, entryId: `${entry.entryId}-new-pair` })) });
    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    await waitFor(() => expect(screen.queryByLabelText('Selected stash')).toBeNull());
    expect(screen.queryByRole('button', { name: 'Delete stash' })).toBeNull();
    expect(screen.getByRole('button', { name: /Older saved changes/ }).getAttribute('aria-pressed')).toBe('false');
    expect(api.getStashDetails).toHaveBeenCalledTimes(1);
  });
  it('blocks old visible entry actions when a refreshed list fails', async () => {
    await open(); await select();
    vi.mocked(api.listStashes).mockRejectedValue(new Error('Cannot validate stash reflog'));
    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    await screen.findByText(/Cannot validate stash reflog/);
    expect((screen.getByRole('button', { name: 'Apply stash' }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole('button', { name: 'Delete stash' }) as HTMLButtonElement).disabled).toBe(true);
    expect(api.planAction).not.toHaveBeenCalled();
  });
  it('suppresses a late saved-file diff after switching inspected sections', async () => {
    let finish!: (value: DiffResult) => void;
    vi.mocked(api.getDiff).mockImplementation(() => new Promise(resolve => { finish = resolve; }));
    await open(); fireEvent.click(await select());
    await waitFor(() => expect(api.getDiff).toHaveBeenCalled());
    fireEvent.change(screen.getByLabelText('Stash section'), { target: { value: 'index' } });
    await waitFor(() => expect(api.getStashDetails).toHaveBeenLastCalledWith(repository.repositoryId, 'stash-list-1', 'opaque-first', 'index'));
    await act(async () => finish({ kind: 'text', text: '+obsolete worktree diff', truncated: false }));
    expect(screen.queryByText('+obsolete worktree diff')).toBeNull();
    expect(screen.queryByLabelText('Stash file diff')).toBeNull();
  });
  it('refreshes actual failed Apply conflicts and retains the saved entry', async () => {
    await open(); await select();
    fireEvent.click(screen.getByRole('button', { name: 'Apply stash' }));
    const preview = await screen.findByRole('dialog');
    vi.mocked(api.runAction).mockResolvedValue({ ...fixtures.failedAction, repositoryId: repository.repositoryId, action: 'applyStash' });
    vi.mocked(api.getStatus).mockResolvedValue({ ...repository.status, dirty: true, counts: { staged: 0, unstaged: 0, untracked: 0, conflicted: 1 } });
    vi.mocked(api.getChanges).mockResolvedValue({ snapshotId: 'conflict', files: [{ ...file, conflicted: true, indexStatus: 'U', worktreeStatus: 'U' }] });
    fireEvent.click(within(preview).getByRole('button', { name: 'Run apply stash' }));
    await screen.findByText('Unresolved index conflicts');
    expect(screen.getByRole('button', { name: /Newer saved changes/ })).toBeTruthy();
    expect(screen.getByLabelText('Git operation').textContent).toContain('saved.txt');
    expect(screen.queryByRole('button', { name: /Continue merge|Abort merge/ })).toBeNull();
    expect((screen.getByRole('button', { name: 'Apply stash' }) as HTMLButtonElement).disabled).toBe(true);
    expect(api.listStashes).toHaveBeenCalledTimes(2);
  });
  it('requires explicit reviewed finalization and exposes manual recovery when automatic completion is unavailable', async () => {
    vi.mocked(api.listStashes).mockResolvedValue({ ...list, recovery: { recoveryId: 'recovery-token', message: 'Review interrupted deletion', canFinalize: false, backupRef: 'refs/git-view/stash-backup/id' } });
    await open();
    expect((screen.getByRole('button', { name: 'Review deletion recovery' }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByLabelText('Stash deletion recovery').textContent).toContain('may complete the previously approved removal');
    vi.mocked(api.listStashes).mockResolvedValue({ ...list, recovery: { recoveryId: 'recovery-token', message: 'Safe to complete deletion', canFinalize: true, backupRef: 'refs/git-view/stash-backup/id' } });
    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    await screen.findByText('Safe to complete deletion');
    fireEvent.click(screen.getByRole('button', { name: 'Review deletion recovery' }));
    const preview = await screen.findByRole('dialog');
    expect(preview.textContent).toContain('Finalize may complete');
    expect(api.planAction).toHaveBeenLastCalledWith(repository.repositoryId, { kind: 'finalizeStashDelete', statusSnapshotId: repository.status.snapshotId, recoveryId: 'recovery-token' });
  });
  it('shows generic idle-index conflicts with staging guidance and no false merge/cherry-pick recovery controls', async () => {
    repository.status = { ...repository.status, dirty: true, gitState: 'idle', counts: { staged: 0, unstaged: 0, untracked: 0, conflicted: 1 } };
    vi.mocked(api.getStatus).mockResolvedValue(repository.status);
    vi.mocked(api.getChanges).mockResolvedValue({ snapshotId: 'conflict', files: [{ ...file, conflicted: true, indexStatus: 'U', worktreeStatus: 'U' }] });
    await open(); await select();
    const notice = screen.getByLabelText('Git operation');
    expect(notice.textContent).toContain('Unresolved index conflicts');
    expect(notice.textContent).toContain('source is not recorded');
    expect(screen.queryByRole('button', { name: /Continue merge|Abort merge|Continue cherry-pick|Abort cherry-pick/ })).toBeNull();
    expect((screen.getByRole('button', { name: 'Apply stash' }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: 'Close stashes' }));
    expect((screen.getByRole('button', { name: 'Stage resolution' }) as HTMLButtonElement).disabled).toBe(false);
  });
});
