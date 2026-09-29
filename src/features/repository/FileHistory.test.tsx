import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { api } from '../../api/client';
import type { CommitDetails, CommitSummary, DiffResult, FileHistoryPage, FileHistoryRequest } from '../../api/types';
import { FileHistory } from './FileHistory';

vi.mock('../../api/client', () => ({ errorMessage: (e: unknown) => e instanceof Error ? e.message : String(e), api: Object.fromEntries(['getFileHistory', 'getFileHistoryCommit', 'getDiff', 'planAction'].map(name => [name, vi.fn()])) }));
const commit = (oid: string): CommitSummary => ({ oid, parents: [], subject: `${oid} subject`, authorName: 'Author', authoredAt: '', refs: [], boundary: false });
const request: FileHistoryRequest = { repositoryId: 'repo', source: { snapshotId: 'source', fileId: 'opaque-file', path: 'current' } };
const page: FileHistoryPage = { displayPath: 'new.txt', startOid: 'tip', shallow: false, history: { snapshotId: 'history', commits: [commit('one'), commit('two')], nextCursor: null } };
const details = (oid: string): CommitDetails => ({ oid, snapshotId: `details-${oid}`, parents: ['parent1', 'parent2'], selectedParent: 'parent1', message: `${oid} actual message`, author: { name: 'Author', email: 'author@test', at: '' }, committer: { name: 'Author', email: 'author@test', at: '' }, files: [{ fileId: `file-${oid}`, displayPath: 'new.txt', oldDisplayPath: 'old.txt', changeKind: 'renamed', indexStatus: null, worktreeStatus: null, untracked: false, conflicted: false, submodule: false }] });
const props = { request, displayPath: 'new.txt', previousPath: 'old.txt', layout: 'split' as const, onLayoutChange: vi.fn(), onClose: vi.fn() };
beforeEach(() => {
  vi.resetAllMocks();
  HTMLDialogElement.prototype.showModal = function () { this.open = true; };
  HTMLDialogElement.prototype.close = function () { this.open = false; };
  vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(600);
  vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(600);
  vi.mocked(api.getFileHistory).mockResolvedValue(page);
  vi.mocked(api.getFileHistoryCommit).mockImplementation(async (_id, _snapshot, oid) => details(oid));
  vi.mocked(api.getDiff).mockResolvedValue({ kind: 'text', text: '@@ -1 +1 @@\n-old\n+<img src=x>\n', truncated: false });
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

it('uses opaque source IDs, opens exact read-only commit patches, supports parents and returns focus on close', async () => {
  const trigger = document.createElement('button'); document.body.append(trigger); trigger.focus();
  const view = render(<FileHistory {...props} />);
  fireEvent.click(await screen.findByRole('button', { name: /one subject/ }));
  expect(api.getFileHistory).toHaveBeenCalledWith(request);
  expect(api.getFileHistoryCommit).toHaveBeenCalledWith('repo', 'history', 'one', null);
  await screen.findByText('+<img src=x>');
  expect(api.getDiff).toHaveBeenCalledWith({ repositoryId: 'repo', snapshotId: 'details-one', fileId: 'file-one', context: 'commit' });
  expect(view.container.querySelector('img,svg')).toBeNull();
  expect(view.container.querySelector('.split-diff')).toBeTruthy();
  expect(screen.queryByRole('button', { name: 'Stage hunk' })).toBeNull();
  fireEvent.change(screen.getByLabelText('File history comparison parent'), { target: { value: 'parent2' } });
  await waitFor(() => expect(api.getFileHistoryCommit).toHaveBeenLastCalledWith('repo', 'history', 'one', 'parent2'));
  expect(api.planAction).not.toHaveBeenCalled();
  fireEvent(screen.getByRole('dialog', { name: 'File history' }), new Event('cancel', { bubbles: false, cancelable: true })); expect(props.onClose).toHaveBeenCalled();
  view.unmount(); expect(document.activeElement).toBe(trigger); trigger.remove();
});

it('ignores an initial response after changing to previous-path history', async () => {
  let finish!: (page: FileHistoryPage) => void;
  vi.mocked(api.getFileHistory).mockImplementationOnce(() => new Promise(resolve => { finish = resolve; })).mockResolvedValueOnce({ ...page, displayPath: 'old.txt', history: { ...page.history, commits: [commit('previous')] } });
  render(<FileHistory {...props} />);
  fireEvent.change(screen.getByLabelText('Path history'), { target: { value: 'previous' } });
  await screen.findByRole('button', { name: /previous subject/ });
  expect(api.getFileHistory).toHaveBeenLastCalledWith({ ...request, source: { ...request.source, path: 'previous' } });
  await act(async () => finish(page));
  expect(screen.queryByRole('button', { name: /one subject/ })).toBeNull();
});

it('ignores stale selected-commit details and stale diff responses', async () => {
  let finishDetails!: (details: CommitDetails) => void;
  let finishDiff!: (diff: DiffResult) => void;
  vi.mocked(api.getFileHistoryCommit).mockImplementationOnce(() => new Promise(resolve => { finishDetails = resolve; })).mockResolvedValueOnce(details('two')).mockResolvedValueOnce(details('one'));
  vi.mocked(api.getDiff).mockImplementationOnce(() => new Promise(resolve => { finishDiff = resolve; })).mockResolvedValueOnce({ kind: 'binary', description: 'Current binary change' });
  render(<FileHistory {...props} />);
  fireEvent.click(await screen.findByRole('button', { name: /one subject/ }));
  fireEvent.click(screen.getByRole('button', { name: /two subject/ }));
  await waitFor(() => expect(api.getDiff).toHaveBeenCalledTimes(1));
  await act(async () => finishDetails(details('stale')));
  expect(screen.queryByText('stale actual message')).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: /one subject/ }));
  await screen.findByText('Current binary change');
  await act(async () => finishDiff({ kind: 'text', text: 'stale patch', truncated: false }));
  expect(screen.queryByText('stale patch')).toBeNull();
});

it('pages using snapshot cursors and rejects an incompatible page without losing prior results', async () => {
  vi.mocked(api.getFileHistory).mockResolvedValueOnce({ ...page, history: { ...page.history, nextCursor: 'cursor' } }).mockResolvedValueOnce({ ...page, history: { ...page.history, snapshotId: 'different' } });
  render(<FileHistory {...props} />);
  fireEvent.click(await screen.findByRole('button', { name: 'Load more results' }));
  await screen.findByRole('alert');
  expect(api.getFileHistory).toHaveBeenLastCalledWith({ ...request, historySnapshotId: 'history', cursor: 'cursor' });
  expect(screen.getByRole('button', { name: /one subject/ })).toBeTruthy();
  expect(screen.getByRole('alert').textContent).toContain('File history changed');
});

it('distinguishes errors from successful empty and shallow results and supports retry', async () => {
  vi.mocked(api.getFileHistory).mockRejectedValueOnce(new Error('Snapshot expired')).mockResolvedValueOnce({ ...page, startOid: null, shallow: true, history: { ...page.history, commits: [] } });
  render(<FileHistory {...props} />);
  await screen.findByRole('alert');
  expect(screen.queryByText('No committed changes for this path at the starting revision.')).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'Retry file history' }));
  await screen.findByText('No committed changes for this path at the starting revision.');
  expect(screen.getByText(/Shallow repository/)).toBeTruthy();
  expect(screen.getByText(/Unborn HEAD/)).toBeTruthy();
});

it('navigates result rows with the keyboard without drawing a false graph or staging controls', async () => {
  render(<FileHistory {...props} />);
  await screen.findByRole('button', { name: /one subject/ });
  const list = screen.getByLabelText('File history results');
  fireEvent.keyDown(list, { key: 'End' });
  await waitFor(() => expect(api.getFileHistoryCommit).toHaveBeenCalledWith('repo', 'history', 'two', null));
  expect(document.activeElement).toBe(list);
  expect(list.querySelector('svg')).toBeNull();
});
