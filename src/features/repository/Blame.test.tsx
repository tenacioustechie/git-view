import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { api } from '../../api/client';
import type { BlameResult, CommitDetails, DiffResult } from '../../api/types';
import { Blame } from './Blame';

vi.mock('../../api/client', () => ({ errorMessage: (e: unknown) => e instanceof Error ? e.message : String(e), api: Object.fromEntries(['getBlame', 'getBlameCommit', 'getDiff', 'planAction'].map(name => [name, vi.fn()])) }));
const identity = { name: 'Author', email: 'author@test', at: '2026-09-30' };
const result: BlameResult = { snapshotId: 'blame-token', revisionOid: 'revision', displayPath: 'new.txt', working: false, shallow: false, unavailableReason: null, lines: [1, 2].map(line => ({ line, originalLine: line + 5, oid: `commit${line}`, author: identity, subject: 'Subject', originalPath: 'old.txt', boundary: false, content: `<script>line ${line}</script>` })) };
const details = (oid: string): CommitDetails => ({ oid, snapshotId: `details-${oid}`, parents: ['parent1', 'parent2'], selectedParent: 'parent1', message: `${oid} message`, author: identity, committer: identity, files: [{ fileId: `file-${oid}`, displayPath: 'old.txt', oldDisplayPath: null, changeKind: 'modified', indexStatus: null, worktreeStatus: null, untracked: false, conflicted: false, submodule: false }] });
const props = { request: { repositoryId: 'repo', snapshotId: 'source', fileId: 'opaque-file', context: 'commit' as const }, displayPath: 'new.txt', previousPath: 'old.txt', layout: 'split' as const, onLayoutChange: vi.fn(), onClose: vi.fn() };
beforeEach(() => {
  vi.resetAllMocks();
  HTMLDialogElement.prototype.showModal = function () { this.open = true; };
  HTMLDialogElement.prototype.close = function () { this.open = false; };
  vi.mocked(api.getBlame).mockResolvedValue(result);
  vi.mocked(api.getBlameCommit).mockImplementation(async (_id, _snapshot, oid) => details(oid));
  vi.mocked(api.getDiff).mockResolvedValue({ kind: 'text', text: '@@ -1 +1 @@\n-old\n+new\n', truncated: false });
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

it('uses backend tokens and opens a line’s real commit with parent-specific read-only diffs', async () => {
  const trigger = document.createElement('button'); document.body.append(trigger); trigger.focus();
  const view = render(<Blame {...props} />);
  fireEvent.click(await screen.findByRole('button', { name: /Inspect line 1:/ }));
  expect(api.getBlame).toHaveBeenCalledWith({ repositoryId: 'repo', snapshotId: 'source', fileId: 'opaque-file', side: 'current' });
  expect(api.getBlameCommit).toHaveBeenCalledWith('repo', 'blame-token', 'commit1', null);
  await screen.findByText('+new');
  expect(api.getDiff).toHaveBeenCalledWith({ repositoryId: 'repo', snapshotId: 'details-commit1', fileId: 'file-commit1', context: 'commit' });
  expect(view.container.querySelector('script')).toBeNull();
  expect(screen.getByText('Line 1 ← old.txt:6')).toBeTruthy();
  expect(screen.queryByRole('button', { name: 'Stage hunk' })).toBeNull();
  fireEvent.change(screen.getByLabelText('Blame comparison parent'), { target: { value: 'parent2' } });
  await waitFor(() => expect(api.getBlameCommit).toHaveBeenLastCalledWith('repo', 'blame-token', 'commit1', 'parent2'));
  expect(api.planAction).not.toHaveBeenCalled();
  fireEvent(screen.getByRole('dialog', { name: 'Blame' }), new Event('cancel', { cancelable: true }));
  expect(props.onClose).toHaveBeenCalled(); view.unmount(); expect(document.activeElement).toBe(trigger); trigger.remove();
});

it('rejects stale blame after switching revision and labels working HEAD separately from edits', async () => {
  let finish!: (value: BlameResult) => void;
  vi.mocked(api.getBlame).mockImplementationOnce(() => new Promise(resolve => { finish = resolve; })).mockResolvedValueOnce({ ...result, displayPath: 'old.txt', lines: [] });
  render(<Blame {...props} request={{ ...props.request, context: 'staged' }} />);
  expect(screen.getByText(/HEAD content only/)).toBeTruthy();
  fireEvent.change(screen.getByLabelText('Blame version'), { target: { value: 'previous' } });
  await screen.findByText('Empty committed file — no lines to attribute.');
  expect(api.getBlame).toHaveBeenLastCalledWith({ repositoryId: 'repo', snapshotId: 'source', fileId: 'opaque-file', side: 'previous' });
  await act(async () => finish(result));
  expect(screen.queryByRole('button', { name: /Inspect line/ })).toBeNull();
});

it('ignores stale commit and diff responses during line selection', async () => {
  let finishDetails!: (value: CommitDetails) => void; let finishDiff!: (value: DiffResult) => void;
  vi.mocked(api.getBlameCommit).mockImplementationOnce(() => new Promise(resolve => { finishDetails = resolve; })).mockResolvedValueOnce(details('commit2')).mockResolvedValueOnce(details('commit1'));
  vi.mocked(api.getDiff).mockImplementationOnce(() => new Promise(resolve => { finishDiff = resolve; })).mockResolvedValueOnce({ kind: 'binary', description: 'Current binary' });
  render(<Blame {...props} />);
  fireEvent.click(await screen.findByRole('button', { name: /Inspect line 1:/ }));
  fireEvent.click(screen.getByRole('button', { name: /Inspect line 2:/ }));
  await waitFor(() => expect(api.getDiff).toHaveBeenCalledTimes(1));
  await act(async () => finishDetails(details('stale'))); expect(screen.queryByText('stale message')).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: /Inspect line 1:/ })); await screen.findByText('Current binary');
  await act(async () => finishDiff({ kind: 'text', text: 'stale patch', truncated: false })); expect(screen.queryByText('stale patch')).toBeNull();
});

it('distinguishes failed reads, unavailable files and empty files, with explicit shallow attribution', async () => {
  vi.mocked(api.getBlame).mockRejectedValueOnce(new Error('Expired')).mockResolvedValueOnce({ ...result, shallow: true, lines: [], unavailableReason: 'Binary file unavailable' });
  render(<Blame {...props} />); await screen.findByRole('alert');
  expect(screen.queryByText('Empty committed file — no lines to attribute.')).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'Retry blame' })); await screen.findByText('Binary file unavailable');
  expect(screen.getByText(/Shallow history/)).toBeTruthy(); expect(screen.queryByText('Empty committed file — no lines to attribute.')).toBeNull();
});

it('bounds the rendered lines, keeps paging focus visible, and supports arrow navigation without automatic commit loads', async () => {
  vi.mocked(api.getBlame).mockResolvedValue({ ...result, lines: Array.from({ length: 205 }, (_, index) => ({ ...result.lines[0], line: index + 1 })) });
  render(<Blame {...props} />);
  const first = await screen.findByRole('button', { name: /Inspect line 1:/ });
  expect(screen.getAllByRole('button', { name: /Inspect line/ })).toHaveLength(200);
  fireEvent.keyDown(first, { key: 'End' }); expect(document.activeElement?.getAttribute('aria-label')).toMatch(/Inspect line 200:/);
  expect(api.getBlameCommit).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Next lines' }));
  expect(screen.getAllByRole('button', { name: /Inspect line/ })).toHaveLength(5);
  expect(document.activeElement).toBe(screen.getByLabelText('Blame lines'));
  expect(screen.getByText('Lines 201–205 of 205')).toBeTruthy(); expect(api.getBlame).toHaveBeenCalledTimes(1);
});

it('offers explicit boundary attribution and returns keyboard focus from commit inspection', async () => {
  vi.mocked(api.getBlame).mockResolvedValue({ ...result, shallow: true, lines: [{ ...result.lines[0], boundary: true }] });
  render(<Blame {...props} />); fireEvent.click(await screen.findByRole('button', { name: /shallow boundary/ }));
  await screen.findByText(/earlier origin unknown/);
  fireEvent.click(screen.getByRole('button', { name: 'Back to lines' }));
  expect(document.activeElement).toBe(screen.getByLabelText('Blame lines')); expect(screen.queryByLabelText('Blame commit')).toBeNull();
});
