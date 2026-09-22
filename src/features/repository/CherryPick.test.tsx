import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { api } from '../../api/client';
import type { ChangedFile, OperationContext, RepositoryOpen } from '../../api/types';
import { fixtures } from '../../test/fixtures';
import { RepositoryView } from './RepositoryView';

vi.mock('../../api/client', () => ({ errorMessage: (error: unknown) => error instanceof Error ? error.message : String(error), api: Object.fromEntries(['getStatus', 'getChanges', 'listRefs', 'listRemotes', 'getHistory', 'getCommit', 'getDiff', 'getOperations', 'planAction', 'runAction', 'listRewrites'].map(name => [name, vi.fn()])) }));
const sourceOid = 'a'.repeat(40);
const context: OperationContext = { kind: 'cherryPick', sourceOids: [sourceOid], sourceRef: null, originalHead: 'original-tip', targetBranch: 'main', stopReason: 'conflicts', canContinue: false, canAbort: true, canSkip: false, reason: 'Resolve the index conflicts before continuing.' };
const file: ChangedFile = { fileId: 'conflict-file', displayPath: 'conflict.txt', oldDisplayPath: null, changeKind: 'unmerged', indexStatus: 'U', worktreeStatus: 'D', untracked: false, conflicted: true, submodule: false };
let repository: RepositoryOpen;
let sequence = 0;
beforeEach(() => {
  vi.resetAllMocks();
  vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(600);
  vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(600);
  repository = { repositoryId: `cherry-${sequence++}`, displayPath: '/fixture/cherry', status: { ...fixtures.clean } };
  repository.status.repositoryId = repository.repositoryId;
  vi.mocked(api.getStatus).mockResolvedValue(repository.status);
  vi.mocked(api.getChanges).mockResolvedValue({ snapshotId: 'changes', files: [] });
  vi.mocked(api.listRefs).mockResolvedValue([{ fullName: 'refs/heads/source', label: 'source', oid: sourceOid, kind: 'local' }]);
  vi.mocked(api.listRemotes).mockResolvedValue([]);
  vi.mocked(api.getHistory).mockResolvedValue({ snapshotId: 'history', nextCursor: null, commits: [{ ...fixtures.history.commits[0], oid: sourceOid, subject: 'Cherry source', parents: ['source-parent'] }, { ...fixtures.history.commits[0], oid: 'another', subject: 'Another source', parents: ['source-parent'] }] });
  vi.mocked(api.getCommit).mockImplementation(async (_id, oid) => ({ ...fixtures.mergeCommit, oid, parents: ['source-parent'], message: 'Preserve original source message', files: [] }));
  vi.mocked(api.getOperations).mockResolvedValue([]);
  vi.mocked(api.listRewrites).mockResolvedValue([]);
  vi.mocked(api.planAction).mockResolvedValue({ kind: 'eligible', planId: 'cherry-plan', expiresAt: '', summary: `Cherry-pick ${sourceOid} onto main at target-tip; Preserve original source message`, destination: 'refs/heads/main' });
  vi.mocked(api.runAction).mockResolvedValue({ ...fixtures.failedAction, repositoryId: repository.repositoryId, action: 'cherryPick', state: 'succeeded', error: null });
  HTMLDialogElement.prototype.showModal = function () { this.open = true; };
  HTMLDialogElement.prototype.close = function () { this.open = false; };
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
async function selectSource() {
  fireEvent.click(await screen.findByRole('button', { name: /Cherry source/ }));
  return screen.findByRole('button', { name: 'Cherry-pick commit' });
}
function stopped(stopReason: OperationContext['stopReason'], files = [file]) {
  const conflicted = files.filter(file => file.conflicted).length;
  repository.status = { ...repository.status, gitState: 'cherryPick', dirty: files.length > 0, counts: { staged: conflicted ? 0 : files.length, unstaged: 0, untracked: 0, conflicted }, operationContext: { ...context, stopReason, canContinue: stopReason === 'ready', canSkip: stopReason === 'empty' } };
  vi.mocked(api.getStatus).mockResolvedValue(repository.status);
  vi.mocked(api.getChanges).mockResolvedValue({ snapshotId: 'resolved-changes', files });
}

describe('cherry-pick and operation guidance (mocked IPC)', () => {
  it('previews the selected source against actual target despite a different read-only scope', async () => {
    render(<RepositoryView repository={repository} />);
    fireEvent.click(await screen.findByRole('button', { name: 'source' }));
    expect(api.planAction).not.toHaveBeenCalled();
    fireEvent.click(await selectSource());
    const dialog = await screen.findByRole('dialog');
    expect(dialog.textContent).toContain(sourceOid);
    expect(dialog.textContent).toContain('onto main at target-tip');
    expect(api.planAction).toHaveBeenCalledWith(repository.repositoryId, { kind: 'cherryPick', statusSnapshotId: repository.status.snapshotId, sourceOid });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(api.runAction).not.toHaveBeenCalled();
  });
  it.each(['merge', 'dirty', 'detached', 'active'] as const)('blocks incompatible %s source or target with a reason', async mode => {
    if (mode === 'merge') vi.mocked(api.getCommit).mockResolvedValue(fixtures.mergeCommit);
    if (mode === 'dirty') repository.status.dirty = true;
    if (mode === 'active') repository.status.activeOperationId = 'running-operation';
    if (mode === 'detached') repository.status.head = { kind: 'detached', oid: 'detached-tip' };
    render(<RepositoryView repository={repository} />);
    expect((await selectSource() as HTMLButtonElement).disabled).toBe(true);
    const panel = screen.getByLabelText('Cherry-pick selected commit');
    expect(panel.textContent).toContain(mode === 'merge' ? 'Merge commits are not supported' : mode === 'dirty' ? 'clean index and working tree' : mode === 'active' ? 'Wait for the current repository operation' : 'existing local branch');
  });
  it('rejects an old source preview after a different commit is selected', async () => {
    let finish!: (value: Awaited<ReturnType<typeof api.planAction>>) => void;
    vi.mocked(api.planAction).mockImplementation(() => new Promise(resolve => { finish = resolve; }));
    render(<RepositoryView repository={repository} />);
    fireEvent.click(await selectSource());
    fireEvent.click(screen.getByRole('button', { name: /Another source/ }));
    await act(async () => finish({ kind: 'eligible', planId: 'old-source', expiresAt: '', summary: 'Old source', destination: null }));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.getByRole('alert').textContent).toContain('create a new preview');
  });
  it('restores cherry-pick context on remount and keeps continue blocked after external content edits until index resolution', async () => {
    stopped('conflicts');
    const view = render(<RepositoryView repository={repository} />);
    const notice = await screen.findByLabelText('Git operation');
    await within(notice).findByText(/conflict.txt · modify\/delete conflict/);
    expect(notice.textContent).toContain(sourceOid);
    expect(notice.textContent).toContain('Editing file content alone does not resolve');
    expect((screen.getByRole('button', { name: 'Continue cherry-pick' }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    await waitFor(() => expect(api.getStatus).toHaveBeenCalledTimes(2));
    expect((screen.getByRole('button', { name: 'Continue cherry-pick' }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.queryByRole('button', { name: 'Continue merge' })).toBeNull();
    view.unmount(); render(<RepositoryView repository={repository} />);
    expect(screen.getByLabelText('Git operation').textContent).toContain('Cherry-pick in progress');
    stopped('ready', [{ ...file, conflicted: false, indexStatus: 'M', worktreeStatus: '.' }]);
    fireEvent.click(await screen.findByRole('button', { name: 'Refresh' }));
    await waitFor(() => expect((screen.getByRole('button', { name: 'Continue cherry-pick' }) as HTMLButtonElement).disabled).toBe(false));
    fireEvent.change(screen.getByLabelText('Commit message'), { target: { value: 'Separate normal draft' } });
    expect((screen.getByRole('button', { name: 'Commit staged changes (1)' }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: 'Continue cherry-pick' }));
    await screen.findByRole('dialog');
    expect(api.planAction).toHaveBeenLastCalledWith(repository.repositoryId, { kind: 'continueCherryPick', statusSnapshotId: repository.status.snapshotId });
    expect((screen.getByLabelText('Commit message') as HTMLTextAreaElement).value).toBe('Separate normal draft');
  });
  it('offers Skip only for a backend-confirmed empty result and previews abort consequences separately', async () => {
    stopped('empty', []);
    render(<RepositoryView repository={repository} />);
    fireEvent.click(await screen.findByRole('button', { name: 'Refresh' }));
    await waitFor(() => expect(api.getStatus).toHaveBeenCalledTimes(2));
    expect((screen.getByRole('button', { name: 'Continue cherry-pick' }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: 'Skip empty cherry-pick' }));
    const preview = await screen.findByRole('dialog');
    expect(preview.textContent).toContain('without creating a commit');
    expect(api.planAction).toHaveBeenLastCalledWith(repository.repositoryId, { kind: 'skipCherryPick', statusSnapshotId: repository.status.snapshotId });
    fireEvent.click(within(preview).getByRole('button', { name: 'Cancel' }));
    fireEvent.click(screen.getByRole('button', { name: 'Abort cherry-pick' }));
    expect((await screen.findByRole('dialog')).textContent).toContain('discard edits made while resolving');
    expect(api.planAction).toHaveBeenLastCalledWith(repository.repositoryId, { kind: 'abortCherryPick', statusSnapshotId: repository.status.snapshotId });
  });
  it('reports unknown external source honestly and never guesses empty from zero staged files', async () => {
    stopped('unknown', []);
    repository.status.operationContext = { ...context, sourceOids: [], originalHead: null, targetBranch: null, stopReason: 'unknown', canContinue: false, canAbort: false, canSkip: false, reason: 'External operation needs manual inspection.' };
    render(<RepositoryView repository={repository} />);
    expect(screen.getByLabelText('Git operation').textContent).toContain('Unknown — no reliable source context');
    expect(screen.getByLabelText('Git operation').textContent).toContain('Original HEAD: Unknown');
    expect(screen.queryByRole('button', { name: 'Skip empty cherry-pick' })).toBeNull();
    expect((screen.getByRole('button', { name: 'Continue cherry-pick' }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole('button', { name: 'Abort cherry-pick' }) as HTMLButtonElement).disabled).toBe(true);
  });
  it('refreshes actual stopped state after hook failure and retains the unrelated normal draft', async () => {
    vi.mocked(api.runAction).mockResolvedValue({ ...fixtures.failedAction, repositoryId: repository.repositoryId, action: 'cherryPick', error: { code: 'hook', message: 'Cherry-pick hook rejected commit', retryable: true } });
    render(<RepositoryView repository={repository} />);
    fireEvent.change(await screen.findByLabelText('Commit message'), { target: { value: 'Keep separate normal draft' } });
    fireEvent.click(await selectSource());
    const preview = await screen.findByRole('dialog');
    stopped('ready', [{ ...file, conflicted: false, indexStatus: 'M', worktreeStatus: '.' }]);
    fireEvent.click(within(preview).getByRole('button', { name: 'Run cherry-pick' }));
    await waitFor(() => expect(api.getStatus).toHaveBeenCalledTimes(2));
    expect(screen.getByLabelText('Git operation').textContent).toContain('Cherry-pick in progress');
    expect(screen.getAllByRole('alert').some(node => node.textContent?.includes('hook rejected'))).toBe(true);
    expect((screen.getByLabelText('Commit message') as HTMLTextAreaElement).value).toBe('Keep separate normal draft');
    expect((screen.getByRole('button', { name: 'Continue cherry-pick' }) as HTMLButtonElement).disabled).toBe(false);
    expect((screen.getByRole('button', { name: 'Commit staged changes (1)' }) as HTMLButtonElement).disabled).toBe(true);
  });
  it('shows known ordinary-merge sources with shared file guidance while preserving merge controls', async () => {
    repository.status = { ...repository.status, gitState: 'merge', counts: { staged: 0, unstaged: 0, untracked: 0, conflicted: 1 }, operationContext: { ...context, kind: 'merge', sourceOids: [sourceOid, 'second-merge-source'] } };
    vi.mocked(api.getStatus).mockResolvedValue(repository.status);
    vi.mocked(api.getChanges).mockResolvedValue({ snapshotId: 'merge-files', files: [{ ...file, indexStatus: 'A', worktreeStatus: 'A' }] });
    render(<RepositoryView repository={repository} />);
    const panel = screen.getByLabelText('Git operation');
    await within(panel).findByText(/add\/add conflict/);
    expect(panel.textContent).toContain('second-merge-source');
    expect(panel.textContent).toContain('Current target: main');
    expect((screen.getByRole('button', { name: 'Continue merge' }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByRole('button', { name: 'Abort merge' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Skip empty cherry-pick' })).toBeNull();
  });
  it('offers reviewed app-marker cleanup for a safe interrupted journal even when Git state is other', async () => {
    repository.status = { ...repository.status, gitState: 'other', operationContext: { ...context, stopReason: 'unknown', canContinue: false, canAbort: true, canSkip: false, reason: 'Git marker absent; clean original target allows clearing only the app marker.' } };
    vi.mocked(api.getStatus).mockResolvedValue(repository.status);
    render(<RepositoryView repository={repository} />);
    await screen.findByRole('button', { name: 'Refresh' });
    expect(screen.getByLabelText('Git operation').textContent).toContain(sourceOid);
    expect(screen.getByLabelText('Git operation').textContent).toContain('Git reports other');
    fireEvent.click(screen.getByRole('button', { name: 'Clear interrupted cherry-pick' }));
    const preview = await screen.findByRole('dialog');
    expect(preview.textContent).toContain('Clear only the recorded app cherry-pick marker');
    expect(preview.textContent).not.toContain('restore files');
    expect(within(preview).getByRole('button', { name: 'Clear interrupted cherry-pick' })).toBeTruthy();
    expect(api.planAction).toHaveBeenLastCalledWith(repository.repositoryId, { kind: 'abortCherryPick', statusSnapshotId: repository.status.snapshotId });
  });
  it.each(['rebase', 'revert', 'merge'] as const)('keeps actual %s labeled correctly and separates a mismatched recorded cherry-pick', async gitState => {
    repository.status = { ...repository.status, gitState, operationContext: { ...context, stopReason: 'unknown', canContinue: false, canAbort: false, canSkip: false, reason: 'Recorded cherry-pick mismatches active Git markers; manual review required.' } };
    vi.mocked(api.getStatus).mockResolvedValue(repository.status);
    render(<RepositoryView repository={repository} />);
    const panel = screen.getByLabelText('Git operation');
    expect(within(panel).getByText(`${gitState[0].toUpperCase()}${gitState.slice(1)} in progress`)).toBeTruthy();
    expect(panel.textContent).toContain(`does not match the active ${gitState}`);
    expect(panel.textContent).toContain(`Recorded cherry-pick source: ${sourceOid}`);
    expect(panel.textContent).toContain('manual review required');
    expect(panel.textContent).not.toContain('Cherry-pick in progress');
    expect(within(panel).getAllByRole('button').map(button => button.textContent)).toEqual(['Review working changes']);
    expect(api.planAction).not.toHaveBeenCalled();
  });
  it('blocks staging and normal commits while an unknown recorded cherry-pick mismatches squash state', async () => {
    repository.status = { ...repository.status, gitState: 'squashMerge', counts: { staged: 1, unstaged: 1, untracked: 0, conflicted: 0 }, operationContext: { ...context, stopReason: 'unknown', canContinue: false, canAbort: false, canSkip: false, reason: 'Review inconsistent operation markers.' } };
    vi.mocked(api.getStatus).mockResolvedValue(repository.status);
    vi.mocked(api.getChanges).mockResolvedValue({ snapshotId: 'mixed', files: [{ ...file, conflicted: false, indexStatus: 'M', worktreeStatus: 'M' }] });
    render(<RepositoryView repository={repository} />);
    expect((await screen.findByRole('button', { name: 'Stage' }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole('button', { name: 'Unstage' }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(screen.getByLabelText('Commit message'), { target: { value: 'Do not commit mismatched state' } });
    expect((screen.getByRole('button', { name: 'Commit staged changes (1)' }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByLabelText('Git operation').textContent).toContain('before staging files');
    expect(within(screen.getByLabelText('Git operation')).getByText('Squash merge in progress')).toBeTruthy();
  });
});
