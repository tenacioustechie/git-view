import { describe, expect, it } from 'vitest';
import { layoutPage } from './layout';
import type { CommitSummary } from '../api/types';
const commit = (oid: string, parents: string[]): CommitSummary => ({ oid, parents, subject: oid, authorName: 'Test', authoredAt: '', refs: [], boundary: false });
describe('history DAG', () => {
  const commits = [commit('merge', ['left', 'right', 'third']), commit('left', ['base']), commit('right', ['base']), commit('third', ['base']), commit('base', [])];
  it('draws every actual parent, including octopus parents, and retires all root lanes', () => {
    const result = layoutPage(commits);
    result.rows.forEach(row => expect(row.segments.filter(edge => edge.part === 'parent').map(edge => edge.parent)).toEqual(row.commit.parents));
    expect(result.state.pending).toEqual([]);
    expect(result.rows.every(row => row.segments.every(edge => edge.from >= 0 && edge.to >= 0))).toBe(true);
  });
  it('retains identical edge coordinates across page splits', () => {
    const first = layoutPage(commits.slice(0, 2));
    const second = layoutPage(commits.slice(2), first.state);
    expect([...first.rows, ...second.rows]).toEqual(layoutPage(commits).rows);
  });
  it('keeps missing and shallow parents as visible continuation endpoints', () => {
    const result = layoutPage([{ ...commit('shallow', ['missing']), boundary: true }]);
    expect(result.state.pending).toEqual(['missing']);
    expect(result.rows[0].segments[0]).toMatchObject({ part: 'parent', parent: 'missing' });
  });
  it('connects a merge parent already pending in a different lane without duplicating it', () => {
    const result = layoutPage([commit('a', ['base']), commit('b', ['other', 'base']), commit('other', ['base']), commit('base', [])]);
    expect(result.rows[1].pending).toEqual(['base', 'other']);
    expect(result.rows[1].segments.filter(edge => edge.part === 'parent').map(edge => edge.to)).toEqual([1, 0]);
    expect(result.state.pending).toEqual([]);
  });
});
