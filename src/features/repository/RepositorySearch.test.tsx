import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { api } from '../../api/client';
import { fixtures } from '../../test/fixtures';
import type { CommitSummary, HistoryPage, RepositoryOpen } from '../../api/types';
import { RepositoryView } from './RepositoryView';

vi.mock('../../api/client', () => ({ errorMessage: (error: unknown) => error instanceof Error ? error.message : String(error), api: Object.fromEntries(['getStatus', 'getChanges', 'listRefs', 'listRemotes', 'getHistory', 'searchHistory', 'getCommit', 'getDiff', 'getOperations', 'planAction', 'runAction', 'listRewrites'].map(name => [name, vi.fn()])) }));
const match = (oid: string, subject: string): CommitSummary => ({ oid, subject, parents: ['hidden-parent'], authorName: 'Author', authoredAt: '', refs: [], boundary: false });
const page: HistoryPage = { snapshotId: 'search-snapshot', commits: [match('a'.repeat(40), 'Matched newer'), match('b'.repeat(40), 'Matched older')], nextCursor: null };
let repository: RepositoryOpen;
let sequence = 0;
beforeEach(() => {
  vi.resetAllMocks();
  vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(600);
  vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(600);
  repository = { repositoryId: `search-${sequence++}`, displayPath: '/fixture/search', status: { ...fixtures.clean } };
  repository.status.repositoryId = repository.repositoryId;
  vi.mocked(api.getStatus).mockResolvedValue(repository.status);
  vi.mocked(api.getChanges).mockResolvedValue({ snapshotId: 'changes', files: [] });
  vi.mocked(api.listRefs).mockResolvedValue([{ fullName: 'refs/heads/main', label: 'main', oid: page.commits[0].oid, kind: 'local' }]);
  vi.mocked(api.listRemotes).mockResolvedValue([]);
  vi.mocked(api.getHistory).mockResolvedValue(fixtures.history);
  vi.mocked(api.searchHistory).mockResolvedValue(page);
  vi.mocked(api.getOperations).mockResolvedValue([]);
  vi.mocked(api.listRewrites).mockResolvedValue([]);
  vi.mocked(api.getCommit).mockResolvedValue({ ...fixtures.mergeCommit, message: 'Actual selected commit body', files: [{ fileId: 'historical-file', displayPath: 'patch.txt', oldDisplayPath: null, changeKind: 'modified', indexStatus: null, worktreeStatus: null, untracked: false, conflicted: false, submodule: false }] });
  vi.mocked(api.getDiff).mockResolvedValue({ kind: 'text', text: '+actual selected patch', truncated: false });
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
function search(text: string, field: 'message' | 'author' | 'oid' = 'message') {
  fireEvent.change(screen.getByLabelText('History search field'), { target: { value: field } });
  fireEvent.change(screen.getByLabelText('History search query'), { target: { value: text } });
  fireEvent.click(screen.getByRole('button', { name: 'Search history' }));
}

describe('repository history search (mocked IPC)', () => {
  it('submits literal field/query only on request, omits DAG edges and opens actual commit/file details', async () => {
    const view = render(<RepositoryView repository={repository} />);
    fireEvent.click(screen.getByRole("button", { name: "History" }));
    await screen.findByRole('button', { name: /Merge feature/ });
    fireEvent.change(screen.getByLabelText('History search query'), { target: { value: '[a.b]' } });
    expect(api.searchHistory).not.toHaveBeenCalled();
    search('[a.b]');
    fireEvent.click(await screen.findByRole('button', { name: /Matched newer/ }));
    expect(api.searchHistory).toHaveBeenCalledWith(repository.repositoryId, { field: 'message', text: '[a.b]' }, null);
    expect(view.container.querySelector('[aria-label="Search results"] svg')).toBeNull();
    await screen.findByText('Actual selected commit body');
    expect(api.getCommit).toHaveBeenCalledWith(repository.repositoryId, page.commits[0].oid, null);
    fireEvent.click(screen.getByRole('button', { name: /patch.txt/ }));
    expect(await screen.findByText('+actual selected patch')).toBeTruthy();
    expect(api.getDiff).toHaveBeenCalledWith(expect.objectContaining({ snapshotId: fixtures.mergeCommit.snapshotId, fileId: 'historical-file', context: 'commit' }));
  });
  it('pages only the active query snapshot and scope; clear restores ordinary history', async () => {
    vi.mocked(api.searchHistory).mockResolvedValueOnce({ ...page, commits: [page.commits[0]], nextCursor: 'older-results' }).mockResolvedValueOnce({ ...page, commits: [page.commits[1]] });
    const view = render(<RepositoryView repository={repository} />);
    fireEvent.click(screen.getByRole("button", { name: "History" }));
    fireEvent.click(await screen.findByRole('button', { name: 'main' }));
    search('Author@example.invalid', 'author');
    fireEvent.click(await screen.findByRole('button', { name: 'Load more results' }));
    await screen.findByRole('button', { name: /Matched older/ });
    expect(api.searchHistory).toHaveBeenLastCalledWith(repository.repositoryId, { field: 'author', text: 'Author@example.invalid' }, 'refs/heads/main', 'search-snapshot', 'older-results');
    expect(within(screen.getByLabelText('Search results')).getAllByRole('button')).toHaveLength(2);
    fireEvent.click(screen.getByRole('button', { name: 'Clear search' }));
    await screen.findByRole('button', { name: /Merge feature/ });
    expect(api.getHistory).toHaveBeenLastCalledWith(repository.repositoryId, 'refs/heads/main');
    expect(view.container.querySelector('[aria-label="Commit history"] svg')).toBeTruthy();
  });
  it('reissues search when branch scope changes and suppresses stale initial results after a new query', async () => {
    let completeOld!: (value: HistoryPage) => void;
    vi.mocked(api.searchHistory).mockImplementation((_id, query) => query.text === 'old' ? new Promise(resolve => { completeOld = resolve; }) : Promise.resolve({ ...page, commits: [match('c'.repeat(40), 'New result')] }));
    render(<RepositoryView repository={repository} />);
    fireEvent.click(screen.getByRole("button", { name: "History" }));
    await screen.findByRole('button', { name: 'main' });
    search('old');
    await waitFor(() => expect(api.searchHistory).toHaveBeenCalled());
    search('new');
    await screen.findByRole('button', { name: /New result/ });
    fireEvent.click(screen.getByRole('button', { name: 'main' }));
    await waitFor(() => expect(api.searchHistory).toHaveBeenLastCalledWith(repository.repositoryId, { field: 'message', text: 'new' }, 'refs/heads/main'));
    await act(async () => completeOld(page));
    expect(screen.queryByRole('button', { name: /Matched older/ })).toBeNull();
    expect(screen.getByRole('button', { name: /New result/ })).toBeTruthy();
  });
  it('ignores an old search page that completes after Clear search', async () => {
    let completePage!: (value: HistoryPage) => void;
    vi.mocked(api.searchHistory).mockResolvedValueOnce({ ...page, commits: [page.commits[0]], nextCursor: 'next' }).mockImplementationOnce(() => new Promise(resolve => { completePage = resolve; }));
    render(<RepositoryView repository={repository} />);
    fireEvent.click(screen.getByRole("button", { name: "History" }));
    search('find');
    fireEvent.click(await screen.findByRole('button', { name: 'Load more results' }));
    await waitFor(() => expect(api.searchHistory).toHaveBeenCalledTimes(2));
    fireEvent.click(screen.getByRole('button', { name: 'Clear search' }));
    await screen.findByRole('button', { name: /Merge feature/ });
    await act(async () => completePage(page));
    expect(screen.queryByLabelText('Search results')).toBeNull();
    expect(screen.queryByRole('button', { name: /Matched older/ })).toBeNull();
  });
  it('shows explicit empty results, backend errors and invalid OID guidance', async () => {
    vi.mocked(api.searchHistory).mockResolvedValueOnce({ ...page, commits: [] }).mockRejectedValueOnce(new Error('Query exceeds 512 UTF-8 bytes'));
    render(<RepositoryView repository={repository} />);
    fireEvent.click(screen.getByRole("button", { name: "History" }));
    search('abcdef', 'oid');
    expect(await screen.findByText('No commits match this search.')).toBeTruthy();
    expect(api.searchHistory).toHaveBeenLastCalledWith(repository.repositoryId, { field: 'oid', text: 'abcdef' }, null);
    search('oversized', 'message');
    expect(await screen.findByRole('alert')).toHaveProperty('textContent', expect.stringContaining('512 UTF-8 bytes'));
    fireEvent.click(screen.getByRole('button', { name: 'Dismiss error' }));
    expect(screen.queryByText('No commits match this search.')).toBeNull();
    search('ab', 'oid');
    expect(screen.getByRole('alert').textContent).toContain('at least four hexadecimal');
    expect(api.searchHistory).toHaveBeenCalledTimes(2);
  });
  it('focuses search with platform shortcut, clears with Escape and preserves the commit draft', async () => {
    render(<RepositoryView repository={repository} />);
    const draft = await screen.findByLabelText('Commit message');
    fireEvent.change(draft, { target: { value: 'Keep working draft' } });
    fireEvent.keyDown(window, { key: 'f', ctrlKey: true });
    const input = screen.getByLabelText('History search query');
    expect(document.activeElement).toBe(input);
    const dialog = document.createElement('dialog');
    dialog.open = true;
    const dialogInput = document.createElement('input');
    dialog.append(dialogInput); document.body.append(dialog); dialogInput.focus();
    fireEvent.keyDown(dialogInput, { key: 'f', metaKey: true });
    expect(document.activeElement).toBe(dialogInput);
    dialog.remove();
    search('literal');
    await screen.findByRole('button', { name: /Matched newer/ });
    fireEvent.keyDown(screen.getByRole('button', { name: 'History' }), { key: 'Escape' });
    expect(screen.getByLabelText('Search results')).toBeTruthy();
    fireEvent.keyDown(input, { key: 'Escape' });
    await screen.findByRole('button', { name: /Merge feature/ });
    fireEvent.click(screen.getByRole('button', { name: /^Changes / }));
    expect((screen.getByLabelText('Commit message') as HTMLTextAreaElement).value).toBe('Keep working draft');
  });
  it('clears unsubmitted query text without discarding normal history or leaving loading stuck', async () => {
    render(<RepositoryView repository={repository} />);
    fireEvent.click(screen.getByRole("button", { name: "History" }));
    await screen.findByRole('button', { name: /Merge feature/ });
    const input = screen.getByLabelText('History search query');
    fireEvent.change(input, { target: { value: 'unsubmitted query' } });
    fireEvent.keyDown(input, { key: 'Escape' });
    expect((input as HTMLInputElement).value).toBe('');
    expect(screen.getByRole('button', { name: /Merge feature/ })).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Refresh' }) as HTMLButtonElement).disabled).toBe(false);
    expect(api.getHistory).toHaveBeenCalledTimes(1);
    expect(api.searchHistory).not.toHaveBeenCalled();
  });
  it('selects real filtered commits with history navigation keys', async () => {
    render(<RepositoryView repository={repository} />);
    fireEvent.click(screen.getByRole("button", { name: "History" }));
    search('matched');
    await screen.findByRole('button', { name: /Matched newer/ });
    const results = screen.getByLabelText('Search results');
    for (const [key, oid] of [['ArrowDown', page.commits[0].oid], ['End', page.commits[1].oid], ['ArrowUp', page.commits[0].oid], ['Home', page.commits[0].oid]]) {
      fireEvent.keyDown(results, { key });
      await waitFor(() => expect(api.getCommit).toHaveBeenLastCalledWith(repository.repositoryId, oid, null));
      expect(results.querySelector<HTMLButtonElement>('[aria-pressed="true"]')?.title).toContain(oid);
    }
  });
});
