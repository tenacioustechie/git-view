import type { ChangedFile, RepositoryAction, RepositoryStatus } from '../../api/types';
const conflictKinds: Record<string, string> = { AA: 'add/add conflict', UD: 'modify/delete conflict', DU: 'delete/modify conflict', UU: 'content conflict', DD: 'both deleted', AU: 'added on target', UA: 'added on source' };
const operationNames: Record<string, string> = { merge: 'Merge', squashMerge: 'Squash merge', cherryPick: 'Cherry-pick', rebase: 'Rebase', revert: 'Revert', other: 'Git operation' };

export function OperationContext({ status, files, active, onPlan, onReview }: {
  status: RepositoryStatus; files: ChangedFile[] | null; active: boolean;
  onPlan: (action: RepositoryAction) => void; onReview: () => void;
}) {
  if (status.gitState === 'idle') return status.counts.conflicted === 0 ? null : <section role="status" aria-label="Git operation" className="repository-notice operation-context">
    <div className="operation-heading"><strong>Unresolved index conflicts</strong><button onClick={onReview}>Review working changes</button></div>
    <p>Git reports conflicted index entries without an active merge or cherry-pick. This can follow a stash application or an external Git action; the source is not recorded here.</p>
    <p>Resolve each file in your editor, Refresh, then Stage resolution. Editing file content alone does not resolve the index. Review the result and make a normal commit when ready. There is no ordinary merge Continue or Abort for this state.</p>
    <ul>{files?.filter(file => file.conflicted).map(file => <li key={file.fileId}>{file.displayPath} · {conflictKinds[`${file.indexStatus ?? ''}${file.worktreeStatus ?? ''}`] ?? 'unresolved index conflict'}</li>)}</ul>
  </section>;
  const context = status.operationContext ?? null;
  const mismatchedCherry = context?.kind === 'cherryPick' && !['cherryPick', 'other'].includes(status.gitState);
  const cherry = status.gitState === 'cherryPick' || (status.gitState === 'other' && context?.kind === 'cherryPick');
  const interruptedCherry = cherry && status.gitState !== 'cherryPick';
  const merge = !mismatchedCherry && !cherry && status.gitState === 'merge';
  const squash = !mismatchedCherry && !cherry && status.gitState === 'squashMerge';
  const conflicts = files?.filter(file => file.conflicted) ?? [];
  const unresolved = status.counts.conflicted > 0 || conflicts.length > 0;
  const sourceOids = context?.sourceOids ?? (squash && status.squashMerge ? [status.squashMerge.sourceOid] : []);
  const sourceRef = context?.sourceRef ?? (squash ? status.squashMerge?.sourceRef : null);
  const originalHead = context?.originalHead ?? (squash ? status.squashMerge?.originalHead : null);
  const title = interruptedCherry ? 'Interrupted cherry-pick — review required' : cherry ? 'Cherry-pick in progress' : merge ? 'Merge in progress' : squash ? 'Squash merge review' : `${operationNames[status.gitState]} in progress`;
  const target = status.head.kind === 'detached' ? `Detached at ${status.head.oid}` : `${status.head.name}${status.head.kind === 'unborn' ? ' (no commits)' : ` at ${status.head.oid}`}`;
  const guidance = mismatchedCherry ? 'Review the actual Git operation and the recorded cherry-pick manually before proceeding. Cherry-pick recovery controls are unavailable for this different operation. Refresh after resolving the mismatch externally.'
    : unresolved ? 'Resolve each conflicted file in your editor, return here, Refresh, then Stage resolution for each file. Editing file content alone does not resolve Git’s index conflicts.'
    : cherry && context?.stopReason === 'empty' ? 'This cherry-pick has no remaining changes to commit. Review the result, then Skip empty cherry-pick to finish without creating a commit, or Abort cherry-pick.'
      : cherry && (!context || context.stopReason === 'unknown') ? 'Review the reported state and refresh after external Git changes. Use Git externally if a safe next action cannot be identified.'
      : cherry ? 'Review the staged result, then Continue cherry-pick to create the commit with the source message and author. Normal hooks and signing still apply.'
        : merge ? 'Review the staged merge result, then Continue merge to create the merge commit.'
          : squash ? 'Review the staged changes and enter a message below to create one normal commit.'
            : 'Inspect the current operation in Git, resolve it externally, and refresh. Incompatible actions remain blocked.';
  return <section role="status" aria-label="Git operation" className="repository-notice operation-context">
    <div className="operation-heading"><strong>{title}</strong><button onClick={onReview}>Review working changes</button></div>
    {interruptedCherry && <p>Git reports {status.gitState}; the recorded cherry-pick does not match an active cherry-pick marker. No completed pick or rollback is assumed.</p>}
    <p><strong>Current target:</strong> {target}</p>
    {mismatchedCherry && <p><strong>Recorded cherry-pick warning:</strong> A separate cherry-pick record does not match the active {status.gitState}. Its recorded context below does not describe that active operation.</p>}
    <p><strong>{mismatchedCherry ? 'Recorded cherry-pick target:' : 'Recorded target:'}</strong> {context?.targetBranch ?? 'Unknown — no reliable original branch context is available.'}</p>
    <p><strong>{mismatchedCherry ? 'Recorded cherry-pick source:' : 'Source:'}</strong> {sourceRef ? `${sourceRef} · ` : ''}{sourceOids.length ? sourceOids.join(', ') : 'Unknown — no reliable source context is available.'}</p>
    <p><strong>{mismatchedCherry ? 'Recorded cherry-pick original HEAD:' : 'Original HEAD:'}</strong> {originalHead ?? 'Unknown — no reliable original commit is available.'}</p>
    {context?.reason && <p>{context.reason}</p>}
    {context?.kind === 'cherryPick' && context.stopReason === 'unknown' && <p>Resolve the recorded cherry-pick context before staging files or starting another mutation. Fetch and inspection remain available.</p>}
    <p>{guidance}</p>
    {squash && <p>Squash merge does not create MERGE_HEAD. There is no ordinary merge to continue or abort; keep wanted changes and use Git externally for manual cleanup.</p>}
    {cherry && !context && <p>Refresh to identify this cherry-pick before continuing, skipping, or aborting. Its state is not yet known reliably.</p>}
    {files === null ? <p>Loading current file states…</p> : unresolved && <details open className="operation-conflicts"><summary>Unresolved files ({status.counts.conflicted})</summary><ul>{conflicts.map(file => <li key={file.fileId}>{file.displayPath} · {conflictKinds[`${file.indexStatus ?? ''}${file.worktreeStatus ?? ''}`] ?? 'unresolved index conflict'}</li>)}</ul></details>}
    {(merge || cherry) && <div className="operation-actions">
      <button disabled={active || unresolved || (cherry ? !context?.canContinue : context ? !context.canContinue : false)} onClick={() => onPlan({ kind: cherry ? 'continueCherryPick' : 'continueMerge', statusSnapshotId: status.snapshotId })}>{cherry ? 'Continue cherry-pick' : 'Continue merge'}</button>
      {cherry && context?.stopReason === 'empty' && <button disabled={active || !context.canSkip || unresolved} onClick={() => onPlan({ kind: 'skipCherryPick', statusSnapshotId: status.snapshotId })}>Skip empty cherry-pick</button>}
      <button disabled={active || (cherry ? !context?.canAbort : context ? !context.canAbort : false)} onClick={() => onPlan({ kind: cherry ? 'abortCherryPick' : 'abortMerge', statusSnapshotId: status.snapshotId })}>{interruptedCherry && status.gitState === 'other' ? 'Clear interrupted cherry-pick' : cherry ? 'Abort cherry-pick' : 'Abort merge'}</button>
    </div>}
  </section>;
}
