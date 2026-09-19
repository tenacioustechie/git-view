import { useEffect, useState } from 'react';
import { api, errorMessage } from '../../api/client';
import type { CommitSummary, RepositoryAction, RepositoryStatus, RewriteRecord } from '../../api/types';

const messages = new Map<string, string>();
export function suffixError(commits: CommitSummary[], headOid: string | null, count: number): string | null {
  if (!Number.isInteger(count) || count < 2) return 'Choose at least two commits.';
  if (count > commits.length) return 'The selection exceeds the loaded current-branch history.';
  if (!headOid || commits[0]?.oid !== headOid) return 'History no longer starts at the current branch HEAD. Refresh before combining.';
  const selection = commits.slice(0, count);
  if (selection.some(commit => commit.boundary)) return 'Shallow boundary commits cannot be combined.';
  if (selection.some(commit => commit.parents.length !== 1)) return 'Select a linear tip sequence without merge or root commits.';
  if (selection.some((commit, index) => index < selection.length - 1 && commit.parents[0] !== selection[index + 1].oid)) return 'Selected commits must form a consecutive sequence ending at HEAD.';
  return null;
}

export function CombineCommits({ repositoryId, status, active, refreshVersion = 0, onPlan, onClose }: {
  repositoryId: string; status: RepositoryStatus; active: boolean; refreshVersion?: number;
  onPlan: (action: RepositoryAction) => void; onClose: () => void;
}) {
  const [commits, setCommits] = useState<CommitSummary[]>([]);
  const [records, setRecords] = useState<RewriteRecord[]>([]);
  const [count, setCount] = useState(2);
  const [message, setMessage] = useState(messages.get(repositoryId) ?? '');
  const [edited, setEdited] = useState(messages.has(repositoryId));
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [more, setMore] = useState(false);
  const headOid = status.head.kind === 'branch' ? status.head.oid : null;
  useEffect(() => {
    let disposed = false;
    setLoading(true); setError(null);
    Promise.all([api.getHistory(repositoryId, 'HEAD'), api.listRewrites(repositoryId)]).then(([history, rewrites]) => {
      if (disposed) return;
      setCommits(history.commits); setRecords(rewrites); setMore(history.nextCursor !== null);
    }).catch(cause => { if (!disposed) setError(errorMessage(cause)); }).finally(() => { if (!disposed) setLoading(false); });
    return () => { disposed = true; };
  // A failed rewrite can create a journal without changing HEAD/index/worktree,
  // so refresh recovery records even when the Git status snapshot is identical.
  }, [repositoryId, status.snapshotId, refreshVersion]);
  useEffect(() => {
    if (!edited) setMessage(commits.slice(0, Number.isInteger(count) ? count : 0).reverse().map(commit => commit.subject).join('\n\n'));
  }, [commits, count, edited]);
  const selection = commits.slice(0, Number.isInteger(count) && count > 0 ? count : 0);
  const selectionError = suffixError(commits, headOid, count);
  const blocked = status.head.kind !== 'branch' ? 'Switch to an existing local branch before combining commits.'
    : status.gitState !== 'idle' ? 'Finish the current Git operation before combining commits.'
      : status.dirty ? 'Combining commits requires a clean index and working tree.' : selectionError;

  return <section className="combine-panel" aria-label="Combine commits">
    <div className="combine-heading"><h2>Combine commits</h2><button onClick={onClose}>Close combine</button></div>
    <p>Replace a consecutive sequence ending at the current branch HEAD with one commit. The final tree stays the same. A backup keeps the original history.</p>
    <p className="repository-notice">These commits may already be published. Remote-tracking information cannot prove otherwise. Rewriting changes commit IDs; no force push is performed.</p>
    {error && <p role="alert">{error}</p>}
    {loading ? <p role="status">Loading current branch history and recovery records…</p> : <>
      <p><strong>Current branch:</strong> {status.head.kind === 'branch' ? status.head.name : 'No attached branch'}<br /><strong>Original HEAD:</strong> <code>{headOid ?? 'Unavailable'}</code></p>
      <label>Newest commits to combine<input aria-label="Newest commits to combine" type="number" min={2} max={commits.length} value={Number.isNaN(count) ? '' : count} onChange={event => setCount(event.target.value === '' ? NaN : Number(event.target.value))} /></label>
      {more && <small>Choose from the first {commits.length} commits of the current branch. This list is independent of the history scope.</small>}
      {blocked && <p role="status" className="combine-blocked">{blocked}</p>}
      <ol className="combine-selection" aria-label="Commits to replace">{selection.map(commit => <li key={commit.oid}><strong>{commit.subject || '(no subject)'}</strong><code>{commit.oid}</code>{commit.parents.length !== 1 && <small>{commit.parents.length === 0 ? 'Root commit' : 'Merge commit'}</small>}</li>)}</ol>
      {!selectionError && selection.length > 0 && <p>Replacement parent: <code>{selection[selection.length - 1].parents[0]}</code></p>}
      <label className="combine-message">Combined commit message<textarea aria-label="Combined commit message" value={message} rows={5} onChange={event => { setMessage(event.target.value); setEdited(true); messages.set(repositoryId, event.target.value); }} /></label>
      <button disabled={active || Boolean(blocked) || !message.trim() || Boolean(error)} onClick={() => onPlan({ kind: 'combineCommits', statusSnapshotId: status.snapshotId, commitOids: selection.map(commit => commit.oid), message })}>Preview combine commits</button>
    </>}
    <section aria-label="Combine recovery" className="combine-recovery"><h3>Backups and recovery</h3>
      <p>Recovery restores the original branch tip while preserving later working-file edits. A later branch commit or changed backup requires manual review.</p>
      {!loading && !records.length && <p>No combine operations recorded for this repository.</p>}
      {records.map(record => <article key={record.recoveryId} className="rewrite-record">
        <h4>{record.branch} · {record.phase}</h4><small>{record.createdAt}</small>
        <dl><dt>Backup ref</dt><dd><code>{record.backupRef}</code></dd><dt>Original tip</dt><dd><code>{record.originalHead}</code></dd><dt>Replacement tip</dt><dd><code>{record.replacementHead ?? 'Not created'}</code></dd>{record.replacementRef && <><dt>Replacement backup</dt><dd><code>{record.replacementRef}</code></dd></>}</dl>
        {record.error && <p role="alert">{record.error}</p>}
        {!record.canRecover && <p>Automatic recovery is unavailable for this record. Inspect its phase and refs before any manual action.</p>}
        <button disabled={active || loading || !record.canRecover} onClick={() => onPlan({ kind: 'recoverCombine', statusSnapshotId: status.snapshotId, recoveryId: record.recoveryId })}>Preview recovery</button>
      </article>)}
    </section>
  </section>;
}
