import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { api } from '../../api/client';
import type { CommitSummary, RewriteRecord } from '../../api/types';
import { fixtures } from '../../test/fixtures';
import { CombineCommits, suffixError } from './CombineCommits';

vi.mock('../../api/client', () => ({ api: { getHistory: vi.fn(), listRewrites: vi.fn() }, errorMessage: (cause: unknown) => String(cause) }));
const commit = (oid: string, parents: string[]): CommitSummary => ({ oid, parents, subject: `Subject ${oid}`, authorName: 'Test', authoredAt: '', refs: [], boundary: false });
const commits = [commit('tip', ['middle']), commit('middle', ['base']), commit('base', [])];
const status = { ...fixtures.clean, head: { kind: 'branch' as const, name: 'main', oid: 'tip' } };
const record: RewriteRecord = { recoveryId: 'recovery', phase: 'installed', branch: 'refs/heads/main', originalHead: 'tip', replacementHead: 'replacement', baseOid: 'base', treeOid: 'tree', backupRef: 'refs/git-view/backups/recovery', replacementRef: 'refs/git-view/replacements/recovery', commitOids: ['tip', 'middle'], message: 'Combined', createdAt: '2026-09-19T00:00:00Z', error: null, canRecover: true };
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.getHistory).mockResolvedValue({ snapshotId: 'head-history', commits, nextCursor: null });
  vi.mocked(api.listRewrites).mockResolvedValue([]);
});
afterEach(cleanup);

describe('combine selection and recovery', () => {
  it('rejects non-tip, root, merge, shallow and nonconsecutive sequences', () => {
    expect(suffixError(commits, 'tip', 2)).toBeNull();
    expect(suffixError(commits, 'tip', 1)).toContain('at least two');
    expect(suffixError(commits, 'other', 2)).toContain('current branch HEAD');
    expect(suffixError(commits, 'tip', 3)).toContain('root');
    expect(suffixError([commit('tip', ['middle', 'other']), ...commits.slice(1)], 'tip', 2)).toContain('merge');
    expect(suffixError([{ ...commits[0], boundary: true }, ...commits.slice(1)], 'tip', 2)).toContain('Shallow');
    expect(suffixError([commit('tip', ['other']), ...commits.slice(1)], 'tip', 2)).toContain('consecutive');
  });
  it('queries HEAD independently and submits the exact shown suffix and edited message', async () => {
    const onPlan = vi.fn();
    render(<CombineCommits repositoryId="combine-selection" status={status} active={false} onPlan={onPlan} onClose={vi.fn()} />);
    const count = await screen.findByLabelText('Newest commits to combine');
    expect(api.getHistory).toHaveBeenCalledWith('combine-selection', 'HEAD');
    expect(screen.getByLabelText('Commits to replace').textContent).toContain('Subject tip');
    fireEvent.change(count, { target: { value: '1' } });
    expect((screen.getByRole('button', { name: 'Preview combine commits' }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(count, { target: { value: '2' } });
    fireEvent.change(screen.getByLabelText('Combined commit message'), { target: { value: 'Chosen combined message' } });
    fireEvent.click(screen.getByRole('button', { name: 'Preview combine commits' }));
    expect(onPlan).toHaveBeenCalledWith({ kind: 'combineCommits', statusSnapshotId: status.snapshotId, commitOids: ['tip', 'middle'], message: 'Chosen combined message' });
  });
  it('shows durable backup records and allows fresh recovery even with later working edits', async () => {
    vi.mocked(api.listRewrites).mockResolvedValue([record]);
    const onPlan = vi.fn();
    const view = render(<CombineCommits repositoryId="recover-selection" status={{ ...status, dirty: true }} active={false} onPlan={onPlan} onClose={vi.fn()} />);
    const recover = await screen.findByRole('button', { name: 'Preview recovery' });
    expect(screen.getByText(record.backupRef)).toBeTruthy();
    expect((recover as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(recover);
    expect(onPlan).toHaveBeenCalledWith({ kind: 'recoverCombine', statusSnapshotId: status.snapshotId, recoveryId: 'recovery' });
    vi.mocked(api.listRewrites).mockResolvedValue([{ ...record, canRecover: false, error: 'Branch contains a later commit' }]);
    view.rerender(<CombineCommits repositoryId="recover-selection" status={{ ...status, snapshotId: 'new-status' }} active={false} onPlan={onPlan} onClose={vi.fn()} />);
    await screen.findByText('Branch contains a later commit');
    await waitFor(() => expect((screen.getByRole('button', { name: 'Preview recovery' }) as HTMLButtonElement).disabled).toBe(true));
  });
  it('refreshes a failed rewrite journal even when Git status did not change', async () => {
    const view = render(<CombineCommits repositoryId="failed-record" status={status} active={false} refreshVersion={0} onPlan={vi.fn()} onClose={vi.fn()} />);
    await screen.findByText('No combine operations recorded for this repository.');
    vi.mocked(api.listRewrites).mockResolvedValue([{ ...record, phase: 'failed', replacementHead: null, error: 'Candidate hook rejected commit' }]);
    view.rerender(<CombineCommits repositoryId="failed-record" status={status} active={false} refreshVersion={1} onPlan={vi.fn()} onClose={vi.fn()} />);
    expect(await screen.findByText('Candidate hook rejected commit')).toBeTruthy();
    expect(screen.getByText(record.backupRef)).toBeTruthy();
    expect(api.listRewrites).toHaveBeenCalledTimes(2);
  });
});
