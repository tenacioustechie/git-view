import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { api } from '../../api/client';
import { fixtures } from '../../test/fixtures';
import { RepositoryView } from './RepositoryView';
import { VIEW_STORAGE_KEY } from './useViewPreferences';
import type { PullPreview, RepositoryAction, RepositoryActionPlan, RepositoryOpen } from '../../api/types';

vi.mock('../../api/client', () => ({
  errorMessage: (error: unknown) => error instanceof Error ? error.message : String(error),
  api: Object.fromEntries(['getStatus', 'getChanges', 'listRefs', 'listRemotes', 'getHistory', 'getCommit', 'getDiff', 'getOperations', 'planAction', 'runAction', 'listRewrites'].map(name => [name, vi.fn()])),
}));

let repository: RepositoryOpen;
let sequence = 0;
const preview = (mode: PullPreview['mode'], strategy: PullPreview['strategy']): PullPreview => ({
  mode, strategy, branch: 'main', headOid: 'a'.repeat(40), upstreamRef: 'refs/remotes/origin/main', upstreamOid: 'b'.repeat(40),
  remote: 'origin', incoming: 3, local: strategy === 'fastForward' ? 0 : 2, fetchedAt: '2026-11-12T10:00:00Z',
});
const eligible = (pullPreview: PullPreview, extra: Partial<Extract<RepositoryActionPlan, { kind: 'eligible' }>> = {}): RepositoryActionPlan => ({
  kind: 'eligible', planId: `plan-${pullPreview.mode}`, expiresAt: '2030-01-01T00:00:00Z', summary: 'Pull summary', destination: 'origin/main → main', pullPreview, ...extra,
});
const plannedModes = () => vi.mocked(api.planAction).mock.calls.map(([, action]) => (action as Extract<RepositoryAction, { kind: 'pull' }>).mode);
const dialog = () => screen.getByRole('dialog', { hidden: true });

beforeEach(() => {
  localStorage.clear();
  vi.resetAllMocks();
  vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(600);
  vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(600);
  const id = `pull-repository-${sequence++}`;
  repository = { repositoryId: id, displayPath: '/disposable/repo', status: { ...fixtures.clean, repositoryId: id } };
  vi.mocked(api.getStatus).mockResolvedValue(repository.status);
  vi.mocked(api.getChanges).mockResolvedValue({ snapshotId: 'changes-snapshot', files: [] });
  vi.mocked(api.listRefs).mockResolvedValue([]);
  vi.mocked(api.listRemotes).mockResolvedValue([{ id: 'remote-token', name: 'origin', defaultFetch: true, defaultPush: true, destinationRef: 'refs/heads/main' }]);
  vi.mocked(api.getHistory).mockResolvedValue(fixtures.history);
  vi.mocked(api.getOperations).mockResolvedValue([]);
  vi.mocked(api.listRewrites).mockResolvedValue([]);
  vi.mocked(api.runAction).mockResolvedValue({ ...fixtures.failedAction, repositoryId: id, action: 'pull', state: 'succeeded', error: null, reason: null });
  HTMLDialogElement.prototype.showModal = function () { this.open = true; };
  HTMLDialogElement.prototype.close = function () { this.open = false; };
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

it('pulls with rebase by default, refreshes after the fetching preview, and runs the reviewed plan', async () => {
  vi.mocked(api.planAction).mockResolvedValue(eligible(preview('rebase', 'rebase'), {
    rebasePreview: { branch: 'refs/heads/main', originalHead: 'a'.repeat(40), ontoRef: 'refs/remotes/origin/main', ontoOid: 'b'.repeat(40), baseOid: 'c'.repeat(40), commits: [{ oid: 'd'.repeat(40), subject: 'Local work' }] },
  }));
  render(<RepositoryView repository={repository} />);
  const pull = await screen.findByRole('button', { name: 'Pull with rebase' });
  const statusReads = vi.mocked(api.getStatus).mock.calls.length;
  await act(async () => { fireEvent.click(pull); });
  expect(api.planAction).toHaveBeenCalledWith(repository.repositoryId, { kind: 'pull', statusSnapshotId: repository.status.snapshotId, mode: 'rebase' });
  expect(vi.mocked(api.getStatus).mock.calls.length).toBeGreaterThan(statusReads);
  const review = dialog();
  expect(within(review).getByRole('heading').textContent).toBe('Review pull with rebase');
  expect(review.textContent).toContain('origin/main has 3 new commit(s); main has 2 local commit(s).');
  expect(review.textContent).toContain('replayed on top of origin/main with new IDs');
  expect(review.textContent).toContain('Local work');
  await act(async () => { fireEvent.click(within(review).getByRole('button', { name: 'Rebase onto upstream' })); });
  expect(api.runAction).toHaveBeenCalledWith('plan-rebase');
});

it('choosing a mode from Pull options plans it and makes it the saved default', async () => {
  vi.mocked(api.planAction).mockResolvedValue(eligible(preview('merge', 'merge')));
  render(<RepositoryView repository={repository} />);
  await screen.findByRole('button', { name: 'Pull with rebase' });
  fireEvent.click(screen.getByRole('button', { name: 'Pull options' }));
  const options = screen.getByRole('region', { name: 'Pull options' });
  expect(within(options).getByRole('button', { name: /Pull with rebase/ }).textContent).toContain('Default');
  await act(async () => { fireEvent.click(within(options).getByRole('button', { name: 'Pull with merge' })); });
  expect(plannedModes()).toEqual(['merge']);
  expect(JSON.parse(localStorage.getItem(VIEW_STORAGE_KEY)!).preferences.pullMode).toBe('merge');
  expect(dialog().textContent).toContain('A merge commit will join origin/main into main');
  expect(within(dialog()).getByRole('button', { name: 'Merge upstream' })).toBeTruthy();
  expect(screen.getByRole('button', { name: 'Pull with merge' })).toBeTruthy();
});

it('changing the mode inside the review plans again; a behind-only branch shows it will fast-forward', async () => {
  vi.mocked(api.planAction)
    .mockResolvedValueOnce(eligible(preview('rebase', 'rebase')))
    .mockResolvedValueOnce(eligible(preview('fastForward', 'fastForward')));
  render(<RepositoryView repository={repository} />);
  await act(async () => { fireEvent.click(await screen.findByRole('button', { name: 'Pull with rebase' })); });
  await act(async () => { fireEvent.change(within(dialog()).getByRole('combobox', { name: 'Pull mode' }), { target: { value: 'fastForward' } }); });
  expect(plannedModes()).toEqual(['rebase', 'fastForward']);
  expect(dialog().textContent).toContain('so it fast-forwards to bbbbbbbb. No commits are rewritten.');
  expect(within(dialog()).getByRole('button', { name: 'Fast-forward' })).toBeTruthy();
});

it('shows why a pull cannot run, such as nothing to pull or a diverged fast-forward-only pull', async () => {
  vi.mocked(api.planAction).mockResolvedValue({ kind: 'blocked', reason: 'main has 2 local commit(s) not on origin/main, which has 3 new commit(s). Choose Pull with rebase or Pull with merge to combine them.', error: { code: 'diverged', message: '', retryable: false } });
  localStorage.setItem(VIEW_STORAGE_KEY, JSON.stringify({ version: 1, preferences: { pullMode: 'fastForward' } }));
  render(<RepositoryView repository={repository} />);
  await act(async () => { fireEvent.click(await screen.findByRole('button', { name: 'Pull fast-forward only' })); });
  expect(plannedModes()).toEqual(['fastForward']);
  expect(screen.getByRole('alert').textContent).toContain('Choose Pull with rebase or Pull with merge');
  expect(api.runAction).not.toHaveBeenCalled();
});
