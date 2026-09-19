import type { CommitSummary } from '../api/types';

export interface GraphSegment { from: number; to: number; part: 'through' | 'incoming' | 'parent'; parent: string }
export interface GraphRow { commit: CommitSummary; lane: number; width: number; segments: GraphSegment[]; pending: string[] }
export interface GraphState { pending: string[] }

// Pending parent identities, rather than row positions, survive page boundaries.
export function layoutPage(commits: CommitSummary[], initial: GraphState = { pending: [] }): { rows: GraphRow[]; state: GraphState } {
  let pending = [...initial.pending];
  const rows = commits.map(commit => {
    const before = [...pending];
    const incoming = before.indexOf(commit.oid);
    const lane = incoming < 0 ? before.length : incoming;
    const after = before.filter(oid => oid !== commit.oid);
    commit.parents.forEach((parent, index) => {
      if (!after.includes(parent)) after.splice(index === 0 ? Math.min(lane, after.length) : after.length, 0, parent);
    });
    const segments: GraphSegment[] = before.flatMap<GraphSegment>((oid, index) => oid === commit.oid
      ? [{ from: index, to: lane, part: 'incoming' as const, parent: oid }]
      : [{ from: index, to: after.indexOf(oid), part: 'through' as const, parent: oid }]);
    commit.parents.forEach(parent => segments.push({ from: lane, to: after.indexOf(parent), part: 'parent', parent }));
    pending = after;
    return { commit, lane, width: Math.max(before.length, after.length, lane + 1), segments, pending: [...pending] };
  });
  return { rows, state: { pending } };
}
