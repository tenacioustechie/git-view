import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { api } from '../../api/client';
import { fixtures } from '../../test/fixtures';
import { RepositoryView } from './RepositoryView';
import { DEFAULT_VIEW, VIEW_STORAGE_KEY } from './useViewPreferences';
import type { ChangedFile, RepositoryOpen } from '../../api/types';

vi.mock('../../api/client', () => ({
  errorMessage: (error: unknown) => error instanceof Error ? error.message : String(error),
  api: Object.fromEntries(['getStatus', 'getChanges', 'listRefs', 'listRemotes', 'getHistory', 'getCommit', 'getDiff', 'getOperations', 'planAction', 'runAction', 'listRewrites'].map(name => [name, vi.fn()])),
}));
const file: ChangedFile = { fileId: 'opaque-file', displayPath: 'file with spaces.txt', oldDisplayPath: null, changeKind: 'modified', indexStatus: 'M', worktreeStatus: 'M', untracked: false, conflicted: false, submodule: false };
let repository: RepositoryOpen;
let sequence = 0;
beforeEach(() => {
  localStorage.clear();
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
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

function mockCompactWindow(initial = true) {
  let matches = initial;
  const listeners = new Set<() => void>();
  const media = { get matches() { return matches; }, addEventListener: (_: string, listener: () => void) => listeners.add(listener), removeEventListener: (_: string, listener: () => void) => listeners.delete(listener) };
  vi.stubGlobal('matchMedia', () => media);
  return (compact: boolean) => act(() => { matches = compact; listeners.forEach(listener => listener()); });
}

describe('repository workflows (mocked IPC)', () => {
  it('moves focus from the removed branch divider to the compact scope button', async () => {
    const resize = mockCompactWindow(false);
    render(<RepositoryView repository={repository} />);
    await screen.findByLabelText('Commit message');
    fireEvent.click(screen.getByRole('button', { name: 'History' }));
    act(() => screen.getByRole('separator', { name: 'Resize branches panel' }).focus());
    resize(true);
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'History scope and branches' }));
    expect(api.planAction).not.toHaveBeenCalled();
  });
  it('collapses compact history scope, closes with Escape and preserves saved widths', async () => {
    mockCompactWindow();
    localStorage.setItem(VIEW_STORAGE_KEY, JSON.stringify({ version: 1, preferences: { ...DEFAULT_VIEW, openingView: 'history', branchesWidth: 260 } }));
    const saved = localStorage.getItem(VIEW_STORAGE_KEY);
    render(<RepositoryView repository={repository} />);
    await screen.findByRole('button', { name: /Merge feature/ });
    const trigger = screen.getByRole('button', { name: 'History scope and branches' });
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    expect(screen.queryByRole('complementary', { name: 'History scope and branches' })).toBeNull();
    expect(screen.queryByRole('separator', { name: 'Resize branches panel' })).toBeNull();
    fireEvent.click(trigger);
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Close history scope' }));
    fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
    expect(document.activeElement).toBe(trigger);
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    expect(localStorage.getItem(VIEW_STORAGE_KEY)).toBe(saved);
    expect(api.planAction).not.toHaveBeenCalled();
  });
  it('opens branch actions in compact layout and retains form inputs across closing and resizing', async () => {
    const resize = mockCompactWindow();
    render(<RepositoryView repository={repository} />);
    await screen.findByLabelText('Commit message');
    fireEvent.click(screen.getByRole('button', { name: 'Branch actions' }));
    fireEvent.click(screen.getByRole('button', { name: 'Create branch…' }));
    const name = screen.getByLabelText('Name');
    expect(document.activeElement).toBe(name.closest('details')!.querySelector('summary'));
    fireEvent.change(name, { target: { value: 'retain-branch-name' } });
    fireEvent.click(screen.getByRole('button', { name: 'Close history scope' }));
    expect(screen.queryByRole('textbox', { name: 'Name' })).toBeNull();
    resize(false);
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'All branches and tags' }));
    expect(screen.getByLabelText('Name')).toBe(name);
    expect((name as HTMLInputElement).value).toBe('retain-branch-name');
    expect(screen.getByRole('separator', { name: 'Resize branches panel' })).toBeTruthy();
    act(() => { name.focus(); });
    resize(true);
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'History scope and branches' }));
    fireEvent.click(screen.getByRole('button', { name: 'History scope and branches' }));
    expect(screen.getByLabelText('Name')).toBe(name);
    expect(api.planAction).not.toHaveBeenCalled();
  });
  it('shows the chosen compact scope without checkout and dismisses on outside focus', async () => {
    mockCompactWindow();
    vi.mocked(api.listRefs).mockResolvedValue([{ kind: 'local', fullName: 'refs/heads/topic', label: 'topic', oid: fixtures.history.commits[0].oid }]);
    render(<RepositoryView repository={repository} />);
    await screen.findByLabelText('Commit message');
    fireEvent.click(screen.getByRole('button', { name: 'History' }));
    const trigger = screen.getByRole('button', { name: 'History scope and branches' });
    fireEvent.click(trigger);
    fireEvent.click(screen.getByRole('button', { name: 'topic' }));
    await waitFor(() => expect(api.getHistory).toHaveBeenLastCalledWith(repository.repositoryId, 'refs/heads/topic'));
    expect(trigger.textContent).toContain('refs/heads/topic');
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    fireEvent.click(trigger);
    act(() => { screen.getByLabelText('History search query').focus(); });
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    expect(document.activeElement).toBe(screen.getByLabelText('History search query'));
    expect(api.planAction).not.toHaveBeenCalled();
    expect(api.runAction).not.toHaveBeenCalled();
  });
  it('keeps compact scope open for confirmation dialogs and restores usable focus at wide size', async () => {
    const resize = mockCompactWindow();
    render(<RepositoryView repository={repository} />);
    await screen.findByLabelText('Commit message');
    fireEvent.click(screen.getByRole('button', { name: 'History' }));
    const trigger = screen.getByRole('button', { name: 'History scope and branches' });
    fireEvent.click(trigger);
    const modal = document.createElement('dialog');
    modal.open = true; document.body.append(modal);
    fireEvent.keyDown(modal, { key: 'Escape' });
    expect(trigger.getAttribute('aria-expanded')).toBe('true');
    modal.remove();
    resize(false);
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'All branches and tags' }));
    expect(screen.queryByRole('button', { name: 'History scope and branches' })).toBeNull();
  });
  it('restores opening view, details visibility and widths from saved preferences', async () => {
    localStorage.setItem(VIEW_STORAGE_KEY, JSON.stringify({ version: 1, preferences: { ...DEFAULT_VIEW, openingView: 'history', showCommitDetails: false, branchesWidth: 250 } }));
    render(<RepositoryView repository={repository} />);
    expect(screen.getByRole('button', { name: 'History' }).getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByRole('region', { name: 'Repository' }).style.getPropertyValue('--branches-width')).toBe('250px');
    fireEvent.click(await screen.findByRole('button', { name: /Merge feature/ }));
    await screen.findByRole('button', { name: 'Show details' });
    expect(screen.queryByRole('button', { name: 'Cherry-pick commit' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Show details' }));
    expect(JSON.parse(localStorage.getItem(VIEW_STORAGE_KEY)!).preferences.showCommitDetails).toBe(true);
    expect(api.planAction).not.toHaveBeenCalled();
  });
  it('saves View controls for future repositories without navigating or losing the draft; reset preserves Recent', async () => {
    localStorage.setItem('git-view.recent-workspaces.v1', 'keep');
    const view = render(<RepositoryView repository={repository} />);
    const editor = await screen.findByLabelText('Commit message');
    fireEvent.change(editor, { target: { value: 'Keep my draft' } });
    fireEvent.click(screen.getByRole('button', { name: 'View preferences' }));
    fireEvent.change(screen.getByLabelText('Opening view'), { target: { value: 'history' } });
    fireEvent.change(screen.getByRole('slider', { name: 'Changed files width' }), { target: { value: '420' } });
    expect(screen.getByLabelText('Commit message')).toBe(editor);
    expect((editor as HTMLTextAreaElement).value).toBe('Keep my draft');
    expect(screen.getByRole('region', { name: 'Repository' }).style.getPropertyValue('--changes-width')).toBe('420px');
    fireEvent.keyDown(screen.getByLabelText('Opening view'), { key: 'Escape' });
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'View preferences' }));
    view.unmount();
    render(<RepositoryView repository={repository} />);
    expect(screen.getByRole('button', { name: 'History' }).getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(screen.getByRole('button', { name: 'View preferences' }));
    fireEvent.click(screen.getByRole('button', { name: 'Reset view preferences' }));
    expect(localStorage.getItem(VIEW_STORAGE_KEY)).toBeNull();
    expect(localStorage.getItem('git-view.recent-workspaces.v1')).toBe('keep');
    expect(screen.getByRole('button', { name: 'History' }).getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(screen.getByRole('button', { name: /^Changes / }));
    expect((screen.getByLabelText('Commit message') as HTMLTextAreaElement).value).toBe('Keep my draft');
    expect(api.planAction).not.toHaveBeenCalled();
  });
  it('saves divider keyboard sizes and exposes storage failure inside View', async () => {
    render(<RepositoryView repository={repository} />);
    await screen.findByLabelText('Commit message');
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('Quota'); });
    fireEvent.keyDown(screen.getByRole('separator', { name: 'Resize changed files panel' }), { key: 'ArrowRight' });
    expect(screen.getByRole('region', { name: 'Repository' }).style.getPropertyValue('--changes-width')).toBe('360px');
    fireEvent.click(screen.getByRole('button', { name: 'View preferences' }));
    expect(within(screen.getByRole('region', { name: 'View preferences' })).getByText(/View preferences could not be saved/).getAttribute('role')).toBe('status');
    expect(api.planAction).not.toHaveBeenCalled();
  });
  it('opens the requested branch form from Changes without planning a Git mutation', async () => {
    render(<RepositoryView repository={repository} />);
    await screen.findByRole('button', { name: 'Stage' });
    fireEvent.click(screen.getByRole('button', { name: 'Branch actions' }));
    fireEvent.click(screen.getByRole('button', { name: 'Create branch…' }));
    const control = screen.getByText('Create branch').closest('details')!;
    expect(control.open).toBe(true);
    expect(document.activeElement).toBe(control.querySelector('summary'));
    expect(screen.getByRole('button', { name: 'History' }).getAttribute('aria-pressed')).toBe('true');
    expect(screen.queryByRole('region', { name: 'Branch actions' })).toBeNull();
    expect(api.planAction).not.toHaveBeenCalled();
  });
  it('keeps push destination drafts across dismissal and still requires confirmation', async () => {
    render(<RepositoryView repository={repository} />);
    await waitFor(() => expect((screen.getByRole('button', { name: 'Fetch' }) as HTMLButtonElement).disabled).toBe(false));
    const trigger = screen.getByRole('button', { name: 'Push…' });
    fireEvent.click(trigger);
    const input = within(screen.getByRole('region', { name: 'Push…' })).getByLabelText('Destination ref');
    fireEvent.change(input, { target: { value: 'refs/heads/reviewed-topic' } });
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(document.activeElement).toBe(trigger);
    fireEvent.click(trigger);
    expect((screen.getByLabelText('Destination ref') as HTMLInputElement).value).toBe('refs/heads/reviewed-topic');
    fireEvent.click(screen.getByRole('button', { name: 'Preview push' }));
    const dialog = await screen.findByRole('dialog');
    expect(api.planAction).toHaveBeenCalledWith(repository.repositoryId, { kind: 'push', statusSnapshotId: repository.status.snapshotId, remoteId: 'remote-token', destinationRef: 'refs/heads/reviewed-topic', setUpstream: false });
    expect(screen.queryByRole('region', { name: 'Push…' })).toBeNull();
    expect(api.runAction).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(api.runAction).not.toHaveBeenCalled();
  });
  it('opens in Changes, selects only the inspected context, and retains the draft across workspaces', async () => {
    render(<RepositoryView repository={repository} />);
    const rows = await screen.findAllByRole('button', { name: file.displayPath });
    expect(screen.queryByRole('region', { name: 'History' })).toBeNull();
    fireEvent.click(rows[1]);
    expect(rows[1].getAttribute('aria-pressed')).toBe('true');
    expect(rows[0].getAttribute('aria-pressed')).toBe('false');
    expect(api.planAction).not.toHaveBeenCalled();
    expect(within(screen.getByLabelText('Change preview')).getByLabelText('File diff')).toBeTruthy();
    fireEvent.change(screen.getByLabelText('Commit message'), { target: { value: 'Keep across views' } });
    fireEvent.click(screen.getByRole('button', { name: 'History' }));
    expect(screen.queryByLabelText('Filter changed files')).toBeNull();
    expect(screen.getByRole('region', { name: 'History' })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: /^Changes / }));
    expect((screen.getByLabelText('Commit message') as HTMLTextAreaElement).value).toBe('Keep across views');
    expect(screen.queryByLabelText('File diff')).toBeNull();
  });
  it('stages only filtered files using the current snapshot and opaque IDs', async () => {
    vi.mocked(api.getChanges).mockResolvedValue({ snapshotId: 'filtered-snapshot', files: [
      { ...file, fileId: 'a', displayPath: 'src/a.ts', indexStatus: '.' },
      { ...file, fileId: 'b', displayPath: 'src/b.ts', indexStatus: '.' },
      { ...file, fileId: 'c', displayPath: 'docs/readme.md', indexStatus: '.' },
    ] });
    render(<RepositoryView repository={repository} />);
    await screen.findByRole('button', { name: 'Stage all (3)' });
    fireEvent.change(screen.getByLabelText('Filter changed files'), { target: { value: 'src/' } });
    expect(screen.queryByRole('button', { name: 'docs/readme.md' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Stage visible (2)' }));
    await waitFor(() => expect(api.planAction).toHaveBeenCalledWith(repository.repositoryId, { kind: 'stage', snapshotId: 'filtered-snapshot', fileIds: ['a', 'b'] }));
  });
  it('navigates file previews with the keyboard without staging and skips collapsed groups', async () => {
    render(<RepositoryView repository={repository} />);
    const rows = await screen.findAllByRole('button', { name: file.displayPath });
    rows[0].focus();
    fireEvent.keyDown(rows[0], { key: 'ArrowDown' });
    expect(document.activeElement).toBe(rows[1]);
    expect(rows[1].getAttribute('aria-pressed')).toBe('true');
    rows[0].closest('details')!.open = false;
    fireEvent.keyDown(rows[1], { key: 'Home' });
    expect(document.activeElement).toBe(rows[1]);
    expect(api.planAction).not.toHaveBeenCalled();
  });
  it('keeps conflict resolution explicit and prevents group staging during unknown operations', async () => {
    repository.status = { ...repository.status, operationContext: { kind: 'cherryPick', stopReason: 'unknown', sourceOids: [], sourceRef: null, originalHead: null, targetBranch: null, reason: 'Review interrupted operation.', canContinue: false, canAbort: false, canSkip: false } };
    vi.mocked(api.getStatus).mockResolvedValue(repository.status);
    vi.mocked(api.getChanges).mockResolvedValue({ snapshotId: 'guarded', files: [file, { ...file, fileId: 'conflict', displayPath: 'conflict.txt', conflicted: true }] });
    render(<RepositoryView repository={repository} />);
    const stage = await screen.findByRole('button', { name: 'Stage all (1)' });
    expect((stage as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole('button', { name: 'Stage resolution' }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(stage);
    expect(api.planAction).not.toHaveBeenCalled();
  });
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
    fireEvent.click(screen.getByRole('button', { name: 'History' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Load older commits' }));
    expect(await screen.findByRole('alert')).toHaveProperty('textContent', expect.stringContaining('History changed'));
    expect(api.getHistory).toHaveBeenLastCalledWith(repository.repositoryId, null, 'history', 'page-2');
  });
  it('inspects a merge against the explicitly selected parent', async () => {
    render(<RepositoryView repository={repository} />);
    fireEvent.click(screen.getByRole('button', { name: 'History' }));
    fireEvent.click(await screen.findByRole('button', { name: /Merge feature/ }));
    const parents = await screen.findByLabelText('Compare with parent');
    fireEvent.change(parents, { target: { value: fixtures.mergeCommit.parents[1] } });
    await waitFor(() => expect(api.getCommit).toHaveBeenLastCalledWith(repository.repositoryId, fixtures.mergeCommit.oid, fixtures.mergeCommit.parents[1]));
  });
  it('uses exact historical file tokens and clears the file filter and preview on parent changes', async () => {
    const historical = { ...file, fileId: 'history-file', displayPath: 'src/first.ts', indexStatus: null, worktreeStatus: null };
    vi.mocked(api.getCommit).mockResolvedValueOnce({ ...fixtures.mergeCommit, snapshotId: 'parent-one', files: [historical] })
      .mockResolvedValueOnce({ ...fixtures.mergeCommit, snapshotId: 'parent-two', selectedParent: fixtures.mergeCommit.parents[1], files: [{ ...historical, fileId: 'second-file', displayPath: 'src/second.ts' }] });
    render(<RepositoryView repository={repository} />);
    fireEvent.click(screen.getByRole('button', { name: 'History' }));
    fireEvent.click(await screen.findByRole('button', { name: /Merge feature/ }));
    fireEvent.click(await screen.findByRole('button', { name: historical.displayPath }));
    await waitFor(() => expect(api.getDiff).toHaveBeenLastCalledWith({ repositoryId: repository.repositoryId, snapshotId: 'parent-one', fileId: 'history-file', context: 'commit' }));
    expect(screen.getByRole('button', { name: historical.displayPath }).getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(screen.getByRole('button', { name: 'Hide details' }));
    expect(screen.queryByRole('button', { name: 'Cherry-pick commit' })).toBeNull();
    expect(screen.getByLabelText('File diff')).toBeTruthy();
    expect(screen.getByLabelText('Compare with parent')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Show details' }));
    expect(screen.getByRole('button', { name: 'Cherry-pick commit' })).toBeTruthy();
    fireEvent.change(screen.getByLabelText('Filter commit files'), { target: { value: 'first' } });
    fireEvent.change(screen.getByLabelText('Compare with parent'), { target: { value: fixtures.mergeCommit.parents[1] } });
    await screen.findByRole('button', { name: 'src/second.ts' });
    expect(screen.queryByLabelText('File diff')).toBeNull();
    expect((screen.getByLabelText('Filter commit files') as HTMLInputElement).value).toBe('');
    fireEvent.click(screen.getByRole('button', { name: 'src/second.ts' }));
    await waitFor(() => expect(api.getDiff).toHaveBeenLastCalledWith({ repositoryId: repository.repositoryId, snapshotId: 'parent-two', fileId: 'second-file', context: 'commit' }));
    expect(api.planAction).not.toHaveBeenCalled();
  });
  it('previews separate ordinary merge and squash merge actions with source and target', async () => {
    vi.mocked(api.listRefs).mockResolvedValue([{ fullName: 'refs/heads/feature', label: 'feature', oid: 'feature-tip', kind: 'local' }]);
    vi.mocked(api.planAction).mockResolvedValue({ kind: 'eligible', planId: 'merge-plan', expiresAt: '2030-01-01T00:00:00Z', summary: 'Merge feature-tip into main-tip', destination: 'refs/heads/main' });
    render(<RepositoryView repository={repository} />);
    fireEvent.click(screen.getByRole('button', { name: 'History' }));
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
    fireEvent.click(screen.getByRole('button', { name: 'More repository actions' }));
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
    fireEvent.click(screen.getByRole('button', { name: 'More repository actions' }));
    fireEvent.click(screen.getByRole('button', { name: 'Combine commits…' }));
    fireEvent.change(await screen.findByLabelText('Combined commit message'), { target: { value: 'Retain failed combine message' } });
    fireEvent.click(await screen.findByRole('button', { name: 'Preview combine commits' }));
    const preview = await screen.findByRole('dialog');
    expect(preview.textContent).toContain('may already be published');
    fireEvent.click(within(preview).getByRole('button', { name: 'Run combineCommits' }));
    await screen.findAllByText(/Hook rejected commit/);
    fireEvent.click(screen.getByRole('button', { name: 'Close combine' }));
    fireEvent.click(screen.getByRole('button', { name: 'More repository actions' }));
    fireEvent.click(screen.getByRole('button', { name: 'Combine commits…' }));
    expect(((await screen.findByLabelText('Combined commit message')) as HTMLTextAreaElement).value).toBe('Retain failed combine message');
  });
});
