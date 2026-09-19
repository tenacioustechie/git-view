import type { RepositoryRow } from '../../api/types';

export interface GroupFilters { search: string; branch: string; state: 'all' | 'dirty' | 'behind' | 'attention' }
export function branchName(row: RepositoryRow): string {
  if (row.kind !== 'ready') return 'Unavailable';
  const head = row.status.head;
  return head.kind === 'detached' ? `Detached ${head.oid.slice(0, 8)}` : head.name;
}
export function matchesFilters(row: RepositoryRow, filters: GroupFilters): boolean {
  const search = filters.search.trim().toLocaleLowerCase();
  if (search && !`${row.label} ${row.displayPath}`.toLocaleLowerCase().includes(search)) return false;
  if (filters.branch && (row.kind !== 'ready' || row.status.head.kind === 'detached' || row.status.head.name !== filters.branch)) return false;
  if (filters.state === 'all') return true;
  if (row.kind !== 'ready') return filters.state === 'attention';
  const status = row.status;
  if (filters.state === 'dirty') return status.dirty;
  if (filters.state === 'behind') return (status.upstream?.behind ?? 0) > 0;
  return status.dirty || status.gitState !== 'idle' || status.head.kind !== 'branch' || !status.upstream || (status.upstream.behind ?? 0) > 0;
}
export function workingState(row: RepositoryRow): string {
  if (row.kind === 'loading') return 'Loading';
  if (row.kind === 'unavailable') return row.error.message;
  const { status } = row;
  const parts = [status.dirty ? 'Dirty' : 'Clean'];
  if (status.counts.staged) parts.push(`${status.counts.staged} staged`);
  if (status.counts.unstaged) parts.push(`${status.counts.unstaged} unstaged`);
  if (status.counts.untracked) parts.push(`${status.counts.untracked} untracked`);
  if (status.counts.conflicted) parts.push(`${status.counts.conflicted} conflicted`);
  if (status.submoduleDirty) parts.push('submodule changes');
  if (status.gitState !== 'idle') parts.push(`${status.gitState} in progress`);
  if (status.activeOperationId) parts.push('operation running');
  return parts.join(' · ');
}
