import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { api, errorMessage } from '../../api/client';
import type { Changes, CommitDetails, DiffRequest, DiffResult, GitRef, HistoryPage, OperationResult, Remote, RepositoryAction, RepositoryActionPlan, RepositoryOpen } from '../../api/types';
import { HistoryGraph } from '../../graph/HistoryGraph';
import { layoutPage } from '../../graph/layout';
import './repository.css';

const drafts = new Map<string, string>();
const terminal = (operation: OperationResult) => !['queued', 'running'].includes(operation.state);
type Planned = { plan: Extract<RepositoryActionPlan, { kind: 'eligible' }>; action: RepositoryAction };
export function RepositoryView(props: { repository: RepositoryOpen; onBack?: () => void }) {
  return <RepositoryScreen key={props.repository.repositoryId} {...props} />;
}

function RepositoryScreen({ repository, onBack }: { repository: RepositoryOpen; onBack?: () => void }) {
  const id = repository.repositoryId;
  const [status, setStatus] = useState(repository.status);
  const [changes, setChanges] = useState<Changes | null>(null);
  const [refs, setRefs] = useState<GitRef[]>([]);
  const [remotes, setRemotes] = useState<Remote[]>([]);
  const [scope, setScope] = useState<string | null>(null);
  const [history, setHistory] = useState<HistoryPage | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [details, setDetails] = useState<CommitDetails | null>(null);
  const [diff, setDiff] = useState<DiffResult | null>(null);
  const [diffRequest, setDiffRequest] = useState<DiffRequest | null>(null);
  const [draft, setDraft] = useState(drafts.get(id) ?? '');
  const [planned, setPlanned] = useState<Planned | null>(null);
  const [busy, setBusy] = useState(false);
  const [operations, setOperations] = useState<OperationResult[]>([]);
  const [remoteId, setRemoteId] = useState('');
  const [destination, setDestination] = useState('');
  const [setUpstream, setSetUpstream] = useState(false);
  const [branchName, setBranchName] = useState('');
  const [switchNew, setSwitchNew] = useState(true);
  const [branchTarget, setBranchTarget] = useState('');
  const generation = useRef(0);
  const detailGeneration = useRef(0);
  const submittedDrafts = useRef(new Map<string, string>());
  const seenOperations = useRef(new Map<string, string>());
  const dialog = useRef<HTMLDialogElement>(null);
  const active = busy || operations.some(operation => !terminal(operation));
  const graph = useMemo(() => layoutPage(history?.commits ?? []).rows, [history]);

  const refresh = useCallback(async () => {
    const epoch = ++generation.current;
    setLoading(true);
    setError(null);
    setDiffRequest(null);
    try {
      const [nextStatus, nextChanges, nextRefs, nextRemotes, nextHistory] = await Promise.all([
        api.getStatus(id), api.getChanges(id), api.listRefs(id), api.listRemotes(id), api.getHistory(id, scope),
      ]);
      if (epoch !== generation.current) return;
      setStatus(nextStatus); setChanges(nextChanges); setRefs(nextRefs); setRemotes(nextRemotes); setHistory(nextHistory);
      setRemoteId(previous => nextRemotes.some(remote => remote.id === previous) ? previous : (nextRemotes.find(remote => remote.defaultFetch)?.id ?? nextRemotes[0]?.id ?? ''));
    } catch (cause) { if (epoch === generation.current) setError(errorMessage(cause)); }
    finally { if (epoch === generation.current) setLoading(false); }
  }, [id, scope]);

  useEffect(() => { void refresh(); return () => { generation.current++; }; }, [refresh]);
  useEffect(() => {
    const onFocus = () => { void refresh(); };
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, [refresh]);
  useEffect(() => {
    const remote = remotes.find(item => item.id === remoteId);
    setDestination(remote?.destinationRef ?? (status.head.kind === 'branch' ? `refs/heads/${status.head.name}` : ''));
  }, [remoteId, remotes, status.head]);

  useEffect(() => {
    let disposed = false;
    let polling = false;
    const poll = async () => {
      if (polling) return;
      polling = true;
      try {
        const all = await api.getOperations();
        if (disposed) return;
        const own = all.filter(operation => operation.repositoryId === id);
        setOperations(own);
        let changed = false;
        let failure: string | null = null;
        for (const operation of own) {
          const old = seenOperations.current.get(operation.operationId);
          if (terminal(operation) && old !== undefined && old !== operation.state) {
            changed = true;
            if (operation.state === 'failed') failure = operation.error?.message ?? operation.reason ?? 'Operation failed. Inspect activity for details.';
          }
          seenOperations.current.set(operation.operationId, operation.state);
          if (terminal(operation) && submittedDrafts.current.has(operation.operationId)) {
            const submitted = submittedDrafts.current.get(operation.operationId);
            submittedDrafts.current.delete(operation.operationId);
            if (operation.state === 'succeeded' && drafts.get(id) === submitted) { drafts.delete(id); setDraft(''); }
            changed = true;
          }
        }
        if (changed) await refresh();
        if (failure && !disposed) setError(failure);
      } catch (cause) { if (!disposed) setError(`Cannot refresh operations: ${errorMessage(cause)}`); }
      finally { polling = false; }
    };
    void poll();
    const timer = window.setInterval(() => { void poll(); }, 1200);
    return () => { disposed = true; window.clearInterval(timer); };
  }, [id, refresh]);

  useEffect(() => {
    if (planned && !dialog.current?.open) dialog.current?.showModal();
    if (!planned && dialog.current?.open) dialog.current.close();
  }, [planned]);

  const inspect = async (oid: string, parent: string | null = null) => {
    const epoch = ++detailGeneration.current;
    setSelected(oid); setDetails(null); setDiffRequest(null); setError(null);
    try { const result = await api.getCommit(id, oid, parent); if (epoch === detailGeneration.current) setDetails(result); }
    catch (cause) { if (epoch === detailGeneration.current) setError(errorMessage(cause)); }
  };
  useEffect(() => {
    let disposed = false;
    setDiff(null);
    if (diffRequest) api.getDiff(diffRequest).then(result => { if (!disposed) setDiff(result); }).catch(cause => { if (!disposed) setDiff({ kind: 'unavailable', reason: errorMessage(cause) }); });
    return () => { disposed = true; };
  }, [diffRequest]);

  const execute = async ({ plan, action }: Planned) => {
    setBusy(true); setPlanned(null); setError(null);
    try {
      const operation = await api.runAction(plan.planId);
      seenOperations.current.set(operation.operationId, operation.state);
      setOperations(old => [...old.filter(item => item.operationId !== operation.operationId), operation]);
      if (action.kind === 'commit') {
        if (operation.state === 'succeeded' && drafts.get(id) === action.message) { drafts.delete(id); setDraft(''); }
        else if (!terminal(operation)) submittedDrafts.current.set(operation.operationId, action.message);
      }
      if (terminal(operation)) await refresh();
      if (operation.state === 'failed') setError(operation.error?.message ?? operation.reason ?? 'Operation failed. Inspect activity for details.');
    } catch (cause) { setError(errorMessage(cause)); }
    finally { setBusy(false); }
  };
  const act = async (action: RepositoryAction, preview = false) => {
    setBusy(true); setError(null);
    try {
      const plan = await api.planAction(id, action);
      if (plan.kind === 'blocked') setError(plan.reason);
      else if (preview) setPlanned({ plan, action });
      else await execute({ plan, action });
    } catch (cause) { setError(errorMessage(cause)); }
    finally { setBusy(false); }
  };
  const loadMore = async () => {
    if (!history?.nextCursor || loading) return;
    const epoch = generation.current;
    setLoading(true);
    try {
      const next = await api.getHistory(id, scope, history.snapshotId, history.nextCursor);
      if (epoch !== generation.current) return;
      if (next.snapshotId !== history.snapshotId) throw new Error('History changed. Refresh to start a new history snapshot.');
      setHistory({ ...next, commits: [...history.commits, ...next.commits] });
    } catch (cause) { if (epoch === generation.current) setError(errorMessage(cause)); }
    finally { if (epoch === generation.current) setLoading(false); }
  };
  const working = () => { detailGeneration.current++; setSelected(null); setDetails(null); setDiffRequest(null); };
  const headLabel = status.head.kind === 'detached' ? `Detached at ${status.head.oid.slice(0, 8)}` : `${status.head.name}${status.head.kind === 'unborn' ? ' (no commits)' : ''}`;

  return <section className="repository-screen" aria-label="Repository">
    <header className="repository-toolbar">
      {onBack && <button onClick={onBack}>← Back</button>}
      <div className="repository-title"><strong title={repository.displayPath}>{repository.displayPath}</strong><small>{headLabel} · {status.dirty ? 'Working changes' : 'Clean'}{status.upstream ? ` · ↑${status.upstream.ahead ?? '?'} ↓${status.upstream.behind ?? '?'} ${status.upstream.ref}` : ' · No upstream'}</small></div>
      <button disabled={loading} onClick={() => { void refresh(); }}>{loading ? 'Refreshing…' : 'Refresh'}</button>
      <label>Remote <select aria-label="Remote" value={remoteId} onChange={event => setRemoteId(event.target.value)}><option value="">None</option>{remotes.map(remote => <option key={remote.id} value={remote.id}>{remote.name}</option>)}</select></label>
      <button disabled={active || !remoteId} onClick={() => { void act({ kind: 'fetch', remoteId }, true); }}>Fetch</button>
      <button disabled={active} onClick={() => { void act({ kind: 'pull', statusSnapshotId: status.snapshotId }, true); }}>Pull</button>
      <details className="push-controls"><summary>Push…</summary><div>
        <label>Destination ref<input value={destination} onChange={event => setDestination(event.target.value)} placeholder="refs/heads/main" /></label>
        <label><input type="checkbox" checked={setUpstream} onChange={event => setSetUpstream(event.target.checked)} /> Set upstream</label>
        <button disabled={active || !remoteId || !destination} onClick={() => { void act({ kind: 'push', statusSnapshotId: status.snapshotId, remoteId, destinationRef: destination, setUpstream }, true); }}>Preview push</button>
      </div></details>
    </header>
    {error && <div role="alert" className="repository-error">{error}<button onClick={() => setError(null)} aria-label="Dismiss error">×</button></div>}
    {status.gitState !== 'idle' && <div role="status" className="repository-notice">{status.gitState} in progress. Resolve conflicts in your editor, then refresh. Incompatible actions will be blocked.</div>}
    {status.submoduleDirty && <div className="repository-notice">A submodule has working changes. Open the submodule separately to review it.</div>}
    <div className="repository-panels">
      <aside className="repository-refs">
        <h2>History scope</h2><button className={scope === null ? 'selected' : ''} onClick={() => setScope(null)}>All branches and tags</button>
        {(['local', 'remote', 'tag'] as const).map(kind => <div key={kind}><h3>{kind === 'local' ? 'Local branches' : kind === 'remote' ? 'Remote branches' : 'Tags'}</h3>{refs.filter(ref => ref.kind === kind).map(ref => <button title={ref.fullName} key={ref.fullName} className={scope === ref.fullName ? 'selected' : ''} onClick={() => setScope(ref.fullName)}>{ref.label}</button>)}</div>)}
        <details><summary>Switch branch</summary><label>Local branch<select value={branchTarget} onChange={event => setBranchTarget(event.target.value)}><option value="">Choose branch</option>{refs.filter(ref => ref.kind === 'local').map(ref => <option key={ref.fullName} value={ref.fullName}>{ref.label}</option>)}</select></label><button disabled={active || !branchTarget} onClick={() => { void act({ kind: 'switchBranch', statusSnapshotId: status.snapshotId, localRef: branchTarget }, true); }}>Preview switch</button></details>
        <details><summary>Create branch</summary><label>Name<input value={branchName} onChange={event => setBranchName(event.target.value)} /></label><label><input type="checkbox" checked={switchNew} onChange={event => setSwitchNew(event.target.checked)} /> Switch after creating</label><button disabled={active || !branchName || status.head.kind === 'unborn'} onClick={() => { if (status.head.kind !== 'unborn') void act({ kind: 'createBranch', statusSnapshotId: status.snapshotId, name: branchName, startOid: selected ?? status.head.oid, switchAfterCreate: switchNew }, true); }}>Preview create</button><small>Starts at {selected ? selected.slice(0, 8) : 'HEAD'}.</small></details>
      </aside>
      <section className="repository-history" aria-label="History">
        <button className={`working-entry ${selected === null ? 'selected' : ''}`} onClick={working}>Working changes <small>{status.counts.staged} staged · {status.counts.unstaged} unstaged · {status.counts.untracked} untracked · {status.counts.conflicted} conflicts</small></button>
        <HistoryGraph rows={graph} selected={selected} onSelect={oid => { void inspect(oid); }} loading={loading} more={Boolean(history?.nextCursor)} onMore={() => { void loadMore(); }} />
      </section>
      <section className="repository-details" aria-label={selected ? 'Commit details' : 'Working changes'}>
        {selected ? <>
          <h2>Commit {selected.slice(0, 8)}</h2>
          {!details ? <p>Loading commit…</p> : <>
            <pre className="commit-message">{details.message}</pre>
            <dl className="commit-metadata"><dt>Author</dt><dd>{details.author.name} &lt;{details.author.email}&gt; · {details.author.at}</dd><dt>Committer</dt><dd>{details.committer.name} &lt;{details.committer.email}&gt; · {details.committer.at}</dd><dt>Commit</dt><dd>{details.oid}</dd></dl>
            {details.parents.length > 0 ? <label>Compare with parent<select aria-label="Compare with parent" value={details.selectedParent ?? ''} onChange={event => { void inspect(details.oid, event.target.value); }}>{details.parents.map((parent, index) => <option key={parent} value={parent}>Parent {index + 1}: {parent}</option>)}</select></label> : <p>Root commit · compared with empty tree</p>}
            <h3>Changed files ({details.files.length})</h3><ul className="file-list">{details.files.map(file => <li key={file.fileId}><button title={file.displayPath} onClick={() => setDiffRequest({ repositoryId: id, snapshotId: details.snapshotId, fileId: file.fileId, context: 'commit' })}>{file.changeKind} · {file.oldDisplayPath ? `${file.oldDisplayPath} → ` : ''}{file.displayPath}</button></li>)}</ul>
          </>}
        </> : <>
          <h2>Working changes</h2>
          {!changes ? <p>Loading changes…</p> : <>
            {(['staged', 'unstaged', 'untracked', 'conflicted'] as const).map(section => {
              const files = changes.files.filter(file => section === 'conflicted' ? file.conflicted : !file.conflicted && (section === 'untracked' ? file.untracked : section === 'staged' ? Boolean(file.indexStatus?.trim() && file.indexStatus !== '?') : Boolean(file.worktreeStatus?.trim() && file.worktreeStatus !== '?' && !file.untracked)));
              return <div key={section}><h3>{section[0].toUpperCase() + section.slice(1)} ({files.length})</h3><ul className="file-list">{files.map(file => <li key={file.fileId}>
                <button title={file.displayPath} onClick={() => setDiffRequest({ repositoryId: id, snapshotId: changes.snapshotId, fileId: file.fileId, context: section === 'staged' ? 'staged' : 'unstaged' })}>{file.oldDisplayPath ? `${file.oldDisplayPath} → ` : ''}{file.displayPath}{file.submodule ? ' (submodule)' : ''}</button>
                <button disabled={active} onClick={() => { void act({ kind: section === 'staged' ? 'unstage' : 'stage', snapshotId: changes.snapshotId, fileIds: [file.fileId] }); }}>{section === 'staged' ? 'Unstage' : file.conflicted ? 'Stage resolution' : 'Stage'}</button>
              </li>)}</ul></div>;
            })}
            {!changes.files.length && <p className="empty">Working tree clean.</p>}
            <form className="commit-form" onSubmit={event => { event.preventDefault(); void act({ kind: 'commit', statusSnapshotId: status.snapshotId, message: draft }); }}>
              <label htmlFor="commit-message">Commit message</label><textarea id="commit-message" value={draft} placeholder="Describe the staged changes" onChange={event => { setDraft(event.target.value); drafts.set(id, event.target.value); }} rows={4} />
              <small>Only staged content is committed. Drafts stay with this repository while the app is open.</small>
              <button type="submit" disabled={active || status.counts.staged === 0 || status.counts.conflicted > 0 || !draft.trim()}>Commit staged changes ({status.counts.staged})</button>
            </form>
          </>}
        </>}
        {diffRequest && <section className="diff-panel" aria-label="File diff"><h3>Diff · {diffRequest.context}</h3>{!diff ? <p>Loading diff…</p> : diff.kind === 'text' ? <>{diff.truncated && <p role="status">Preview truncated: this is not the full patch.</p>}<pre className="unified-diff">{diff.text.split('\n').map((line, index) => <span key={index} className={line.startsWith('+') ? 'diff-add' : line.startsWith('-') ? 'diff-remove' : line.startsWith('@@') ? 'diff-hunk' : ''}>{line}{'\n'}</span>)}</pre>{!diff.text && <p>No text changes.</p>}</> : <p>{diff.kind === 'binary' ? diff.description : diff.reason}</p>}</section>}
      </section>
    </div>
    <details className="repository-activity"><summary>Activity · {operations.length ? `${operations[operations.length - 1].action}: ${operations[operations.length - 1].state}` : 'No operations this session'}</summary><ol>{operations.slice(-30).reverse().map(operation => <li key={operation.operationId}><strong>{operation.action}: {operation.state}</strong>{operation.reason && ` · ${operation.reason}`}{operation.error && <span role="alert"> · {operation.error.message}</span>}{operation.outcomeUncertain && ' · Outcome uncertain. Refresh and inspect the repository before retrying.'}{operation.outputTruncated && ' · Operation output truncated.'}</li>)}</ol></details>
    <dialog ref={dialog} onCancel={() => setPlanned(null)} aria-labelledby="action-preview-title">
      {planned && <><h2 id="action-preview-title">Review {planned.action.kind}</h2><p>{planned.plan.summary}</p>{planned.plan.destination && <p><strong>Destination:</strong> {planned.plan.destination}</p>}<p>Preconditions are checked again before execution.</p><div className="dialog-actions"><button onClick={() => setPlanned(null)}>Cancel</button><button disabled={busy} onClick={() => { void execute(planned); }}>Run {planned.action.kind}</button></div></>}
    </dialog>
  </section>;
}
