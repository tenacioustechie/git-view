import { useRef, useState } from 'react';
import type { GitRef, RepositoryAction, RepositoryStatus } from '../../api/types';

export function Rebase({ status, refs, active, onPlan }: {
  status: RepositoryStatus;
  refs: GitRef[];
  active: boolean;
  onPlan: (action: RepositoryAction, isCurrent: () => boolean) => void;
}) {
  const [onto, setOnto] = useState('');
  const latest = useRef('');
  const available = refs.filter(ref => (ref.kind === 'local' || ref.kind === 'remote')
    && !ref.fullName.endsWith('/HEAD')
    && (status.head.kind !== 'branch' || ref.fullName !== `refs/heads/${status.head.name}`));
  const selected = available.find(ref => ref.fullName === onto);
  const blocked = status.head.kind !== 'branch' || status.gitState !== 'idle' || status.dirty || status.submoduleDirty;
  return <details className="rebase-controls">
    <summary>Rebase branch</summary>
    <label>Onto branch<select aria-label="Rebase onto branch" value={selected ? onto : ''} onChange={event => { latest.current = event.target.value; setOnto(event.target.value); }}>
      <option value="">Choose local or fetched remote branch</option>
      {available.map(ref => <option key={ref.fullName} value={ref.fullName}>{ref.label} ({ref.kind})</option>)}
    </select></label>
    <small>Current branch: {status.head.kind === 'branch' ? `${status.head.name} at ${status.head.oid}` : 'Switch to a local branch first'}. Fetch separately if the target is stale. Only linear source history is supported.</small>
    {selected && <small>Chosen target: {selected.fullName} at {selected.oid}</small>}
    {blocked && <small>Start with a clean local branch and no Git operation in progress.</small>}
    <button disabled={active || blocked || !selected} onClick={() => {
      if (selected) onPlan({ kind: 'startRebase', statusSnapshotId: status.snapshotId, ontoRef: selected.fullName }, () => latest.current === selected.fullName);
    }}>Preview rebase</button>
  </details>;
}
