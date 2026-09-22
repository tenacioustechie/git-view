import { useEffect, useState } from 'react';
import { api, errorMessage } from '../../api/client';
import type { GitRef, RemoteCheckoutTarget, RepositoryAction, RepositoryStatus } from '../../api/types';

export function RemoteCheckout({ repositoryId, status, refs, active, refreshVersion, onPlan, onSwitch }: {
  repositoryId: string; status: RepositoryStatus; refs: GitRef[]; active: boolean; refreshVersion: number;
  onPlan: (action: RepositoryAction) => void; onSwitch: (localRef: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [branches, setBranches] = useState<RemoteCheckoutTarget[]>([]);
  const [issues, setIssues] = useState<{ sourceRef: string; reason: string }[]>([]);
  const [selected, setSelected] = useState('');
  const [name, setName] = useState('');
  const [edited, setEdited] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const key = (branch: RemoteCheckoutTarget) => JSON.stringify([branch.remoteId, branch.sourceRef]);
  useEffect(() => {
    if (!open) return;
    let disposed = false;
    setLoading(true); setError(null);
    api.listRemoteBranches(repositoryId).then(result => {
      if (disposed) return;
      setBranches(result.targets); setIssues(result.issues);
      // An externally removed source requires a new explicit selection. Keep
      // the local-name draft; never silently switch to another remote's tip.
      setSelected(previous => result.targets.some(branch => key(branch) === previous) ? previous : '');
    }).catch(cause => { if (!disposed) setError(errorMessage(cause)); })
      .finally(() => { if (!disposed) setLoading(false); });
    return () => { disposed = true; };
  }, [repositoryId, open, refreshVersion, status.snapshotId]);
  const source = branches.find(branch => key(branch) === selected);
  const collision = refs.find(ref => ref.kind === 'local' && ref.fullName === `refs/heads/${name}`);
  const operationName = status.gitState === 'squashMerge' ? 'squash merge review' : status.gitState === 'cherryPick' ? 'cherry-pick' : status.gitState;
  const blocked = status.gitState !== 'idle' ? `Finish the ${operationName} operation before checking out a remote branch.`
    : status.dirty || status.submoduleDirty ? 'Checkout requires a clean index and working tree, including untracked files and submodules.'
      : active || status.activeOperationId ? 'Wait for the current repository operation to finish.' : null;
  const choose = (value: string) => {
    setSelected(value);
    const next = branches.find(branch => key(branch) === value);
    if (next && !edited) setName(next.suggestedName);
  };
  return <details className="remote-checkout" onToggle={event => setOpen(event.currentTarget.open)}>
    <summary>Check out remote branch</summary>
    {open && <section aria-label="Remote branch checkout">
      <p>Choose a locally recorded remote branch. Use Fetch above to update this list; opening this form does not fetch.</p>
      <p>Current HEAD: {status.head.kind === 'detached' ? `detached at ${status.head.oid}` : `${status.head.name}${status.head.kind === 'unborn' ? ' (no commits)' : ` at ${status.head.oid}`}`}.</p>
      {loading && <p role="status">Loading remote branches…</p>}
      {error && <p role="alert">{error} Use Refresh to retry.</p>}
      {issues.map(issue => <p key={issue.sourceRef} role="status">{issue.sourceRef}: {issue.reason}</p>)}
      {!loading && !error && open && !branches.length && <p>No remote branches are available. Check your remotes and use Fetch.</p>}
      <label>Remote branch<select aria-label="Remote branch" disabled={active || loading} value={selected} onChange={event => choose(event.target.value)}>
        <option value="">Choose remote branch</option>
        {branches.map(branch => <option key={key(branch)} value={key(branch)}>{branch.remoteName} · {branch.sourceRef}</option>)}
      </select></label>
      <label>Local branch name<input aria-label="Local branch name" disabled={active} value={name} onChange={event => { setName(event.target.value); setEdited(true); }} /></label>
      {source && <dl><dt>Remote</dt><dd>{source.remoteName}</dd><dt>Source ref</dt><dd>{source.sourceRef}</dd><dt>Source commit</dt><dd>{source.sourceOid}</dd><dt>Intended upstream</dt><dd>{source.upstreamRef}</dd></dl>}
      {blocked && <p role="status">{blocked}</p>}
      {collision && <div role="status"><p>Local branch {collision.label} already exists at {collision.oid}. Choose another name, or separately review switching to the existing local branch. Its tip and upstream will be kept.</p><button disabled={active} onClick={() => onSwitch(collision.fullName)}>Review existing local branch</button></div>}
      <button disabled={active || loading || Boolean(error || blocked || collision) || !source || !name.trim()} onClick={() => {
        if (source) onPlan({ kind: 'checkoutRemoteBranch', statusSnapshotId: status.snapshotId, remoteId: source.remoteId, sourceRef: source.sourceRef, name });
      }}>Preview checkout</button>
    </section>}
  </details>;
}
