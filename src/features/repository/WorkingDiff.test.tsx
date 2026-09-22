import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { api } from '../../api/client';
import type { ChangedFile, DiffResult, RepositoryOpen } from '../../api/types';
import { fixtures } from '../../test/fixtures';
import { RepositoryView } from './RepositoryView';
import { WorkingDiff } from './WorkingDiff';

vi.mock('../../api/client', () => ({ errorMessage: (error: unknown) => error instanceof Error ? error.message : String(error), api: Object.fromEntries(['getStatus', 'getChanges', 'listRefs', 'listRemotes', 'getHistory', 'getOperations', 'getDiff', 'planAction', 'runAction'].map(name => [name, vi.fn()])) }));
const file: ChangedFile = { fileId: 'file-token', displayPath: 'Unicode β file.txt', oldDisplayPath: null, changeKind: 'modified', indexStatus: 'M', worktreeStatus: 'M', untracked: false, conflicted: false, submodule: false };
const patch = '@@ -2 +2 @@\n-old\n+<script>not executable</script>\n\\ No newline at end of file\n';
function diff(context: 'staged' | 'unstaged', token = 'diff-token'): DiffResult {
  return { kind: 'text', text: 'display fallback must not replace structured text', truncated: false, hunkActions: { kind: 'available', diffSnapshotId: token, context, preamble: 'diff --git a/file b/file\n', hunks: ['first-token', 'second-token'].map(hunkId => ({ hunkId, header: '@@ -2 +2 @@', oldStart: 2, oldLines: 1, newStart: 2, newLines: 1, text: patch })) } };
}
let repository: RepositoryOpen;
let sequence = 0;
beforeEach(() => {
  vi.resetAllMocks();
  repository = { repositoryId: `hunks-${sequence++}`, displayPath: '/fixture/hunks', status: { ...fixtures.mixed } };
  repository.status.repositoryId = repository.repositoryId;
  vi.mocked(api.getStatus).mockResolvedValue(repository.status);
  vi.mocked(api.getChanges).mockResolvedValue({ snapshotId: 'changes-token', files: [file] });
  vi.mocked(api.listRefs).mockResolvedValue([]); vi.mocked(api.listRemotes).mockResolvedValue([]);
  vi.mocked(api.getHistory).mockResolvedValue(fixtures.history); vi.mocked(api.getOperations).mockResolvedValue([]);
  vi.mocked(api.getDiff).mockImplementation(async request => diff(request.context === 'staged' ? 'staged' : 'unstaged'));
  vi.mocked(api.planAction).mockResolvedValue({ kind: 'eligible', planId: 'hunk-plan', expiresAt: '', summary: 'Stage exactly selected hunk', destination: null });
  vi.mocked(api.runAction).mockResolvedValue({ ...fixtures.failedAction, repositoryId: repository.repositoryId, action: 'stageHunk', state: 'succeeded', error: null });
});
afterEach(cleanup);
async function open(context: 'staged' | 'unstaged' = 'unstaged') {
  render(<RepositoryView repository={repository} />);
  const buttons = await screen.findAllByRole('button', { name: file.displayPath });
  fireEvent.click(buttons[context === 'staged' ? 0 : 1]);
  await screen.findByLabelText('Hunk 2');
}

describe('working hunk actions (mocked IPC)', () => {
  it('uses the exact opaque second hunk token for duplicate-looking hunks, without sending patch text', async () => {
    await open();
    const panel = screen.getByLabelText('File diff');
    expect(panel.textContent).toContain(file.displayPath);
    expect(panel.textContent).toContain('Diff · unstaged');
    expect(panel.querySelector('script')).toBeNull();
    expect(screen.getByLabelText('Hunk 2').querySelector('pre')?.textContent).toBe(patch);
    fireEvent.click(within(screen.getByLabelText('Hunk 2')).getByRole('button', { name: 'Stage hunk' }));
    await waitFor(() => expect(api.runAction).toHaveBeenCalledWith('hunk-plan'));
    expect(api.planAction).toHaveBeenCalledWith(repository.repositoryId, { kind: 'stageHunk', diffSnapshotId: 'diff-token', fileId: 'file-token', hunkId: 'second-token' });
    expect(screen.getByRole('button', { name: 'Stage' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Unstage' })).toBeTruthy();
  });
  it('unstages from staged context and reloads the same file with fresh file/diff tokens while retaining the draft', async () => {
    await open('staged');
    fireEvent.change(screen.getByLabelText('Commit message'), { target: { value: 'Retain partial commit draft' } });
    vi.mocked(api.getChanges).mockResolvedValue({ snapshotId: 'fresh-changes', files: [{ ...file, fileId: 'fresh-file' }] });
    vi.mocked(api.getDiff).mockResolvedValue(diff('staged', 'fresh-diff'));
    fireEvent.click(within(screen.getByLabelText('Hunk 1')).getByRole('button', { name: 'Unstage hunk' }));
    await waitFor(() => expect(api.getDiff).toHaveBeenLastCalledWith({ repositoryId: repository.repositoryId, snapshotId: 'fresh-changes', fileId: 'fresh-file', context: 'staged' }));
    await screen.findByLabelText('Hunk 1');
    expect((screen.getByLabelText('Commit message') as HTMLTextAreaElement).value).toBe('Retain partial commit draft');
    expect(api.planAction).toHaveBeenCalledWith(repository.repositoryId, { kind: 'unstageHunk', diffSnapshotId: 'diff-token', fileId: 'file-token', hunkId: 'first-token' });
    fireEvent.click(within(screen.getByLabelText('Hunk 1')).getByRole('button', { name: 'Unstage hunk' }));
    await waitFor(() => expect(api.planAction).toHaveBeenLastCalledWith(repository.repositoryId, { kind: 'unstageHunk', diffSnapshotId: 'fresh-diff', fileId: 'fresh-file', hunkId: 'first-token' }));
  });
  it('suppresses an old diff read after switching staged/unstaged context', async () => {
    let finish!: (value: DiffResult) => void;
    vi.mocked(api.getDiff).mockImplementation(request => request.context === 'staged' ? new Promise(resolve => { finish = resolve; }) : Promise.resolve(diff('unstaged')));
    render(<RepositoryView repository={repository} />);
    const buttons = await screen.findAllByRole('button', { name: file.displayPath });
    fireEvent.click(buttons[0]);
    await waitFor(() => expect(api.getDiff).toHaveBeenCalledTimes(1));
    fireEvent.click(buttons[1]); await screen.findByLabelText('Hunk 1');
    await act(async () => finish(diff('staged', 'obsolete')));
    expect(screen.queryAllByRole('button', { name: 'Unstage hunk' })).toHaveLength(0);
    expect(screen.getAllByRole('button', { name: 'Stage hunk' })).toHaveLength(2);
  });
  it.each(['context', 'refresh', 'close'] as const)('rejects an eligible delayed plan after %s changes the inspected snapshot', async change => {
    let finish!: (value: Awaited<ReturnType<typeof api.planAction>>) => void;
    vi.mocked(api.planAction).mockImplementation(() => new Promise(resolve => { finish = resolve; }));
    await open();
    fireEvent.click(within(screen.getByLabelText('Hunk 1')).getByRole('button', { name: 'Stage hunk' }));
    if (change === 'context') fireEvent.click(screen.getAllByRole('button', { name: file.displayPath })[0]);
    else if (change === 'refresh') fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    else fireEvent.click(screen.getByRole('button', { name: /Working changes .*staged/ }));
    await act(async () => finish({ kind: 'eligible', planId: 'obsolete', expiresAt: '', summary: 'Old hunk', destination: null }));
    await screen.findByText(/Repository view changed while preparing hunk action/);
    expect(api.runAction).not.toHaveBeenCalled();
    if (change === 'close') expect(screen.queryByLabelText('File diff')).toBeNull();
    if (change === 'context') expect(screen.getByLabelText('File diff').textContent).toContain('Diff · staged');
  });
  it('refreshes actual file state after backend stale-plan rejection and preserves entered commit text', async () => {
    await open();
    fireEvent.change(screen.getByLabelText('Commit message'), { target: { value: 'Keep after external edit' } });
    vi.mocked(api.planAction).mockResolvedValue({ kind: 'blocked', reason: 'Working file changed; review again', error: null });
    vi.mocked(api.getDiff).mockResolvedValue(diff('unstaged', 'after-external-edit'));
    fireEvent.click(within(screen.getByLabelText('Hunk 1')).getByRole('button', { name: 'Stage hunk' }));
    await screen.findByText('Working file changed; review again');
    expect(api.getStatus).toHaveBeenCalledTimes(2); expect(api.getDiff).toHaveBeenCalledTimes(2);
    expect(api.runAction).not.toHaveBeenCalled();
    expect((screen.getByLabelText('Commit message') as HTMLTextAreaElement).value).toBe('Keep after external edit');
  });
  it('surfaces skipped mutation reason and refreshes actual state instead of leaving old tokens actionable', async () => {
    await open();
    vi.mocked(api.runAction).mockResolvedValue({ ...fixtures.failedAction, repositoryId: repository.repositoryId, action: 'stageHunk', state: 'skipped', error: null, reason: 'Index changed after review' });
    vi.mocked(api.getChanges).mockResolvedValue({ snapshotId: 'now-clean', files: [] });
    fireEvent.click(within(screen.getByLabelText('Hunk 1')).getByRole('button', { name: 'Stage hunk' }));
    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('Index changed after review'));
    expect(screen.queryByLabelText('File diff')).toBeNull();
    expect(api.getStatus).toHaveBeenCalledTimes(2);
  });
  it.each([
    [{ kind: 'text', text: patch, truncated: true, hunkActions: (diff('unstaged') as Extract<DiffResult, {kind:'text'}>).hunkActions }, 'complete diff'],
    [{ kind: 'text', text: patch, truncated: false, hunkActions: { kind: 'unavailable', reason: 'Renamed files require whole-file staging.' } }, 'Renamed files'],
    [{ kind: 'binary', description: 'Binary file' }, 'Binary changes'],
    [{ kind: 'unavailable', reason: 'Diff exceeds preview limit.' }, 'preview limit'],
  ] as [DiffResult, string][])('disables unsupported or incomplete preview hunk mutations: %s', (value, reason) => {
    const onAction = vi.fn();
    render(<WorkingDiff request={{ repositoryId: 'repo', snapshotId: 'changes', fileId: 'file', context: 'unstaged' }} displayPath={file.displayPath} diff={value} active={false} blockedReason={null} onAction={onAction} />);
    expect(screen.getByLabelText('File diff').textContent).toContain(reason);
    const button = screen.getByRole('button', { name: 'Stage hunk' });
    expect((button as HTMLButtonElement).disabled).toBe(true); fireEvent.click(button); expect(onAction).not.toHaveBeenCalled();
  });
  it('never offers hunk mutations for historical/stash commit diffs even if capability data is present', () => {
    render(<WorkingDiff request={{ repositoryId: 'repo', snapshotId: 'history', fileId: 'file', context: 'commit' }} displayPath={file.displayPath} diff={diff('unstaged')} active={false} blockedReason={null} onAction={vi.fn()} />);
    expect(screen.queryByRole('button')).toBeNull();
  });
});
