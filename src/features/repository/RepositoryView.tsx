import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { api, errorMessage } from '../../api/client';
import type { Changes, CommitDetails, DiffRequest, DiffResult, GitRef, HistoryPage, HistorySearch, OperationResult, Remote, RepositoryAction, RepositoryActionPlan, RepositoryOpen } from '../../api/types';
import { HistoryGraph } from '../../graph/HistoryGraph';
import { layoutPage } from '../../graph/layout';
import { CombineCommits } from './CombineCommits';
import './repository.css';

const drafts = new Map<string, string>();
const terminal = (operation: OperationResult) => !['queued', 'running'].includes(operation.state);
const hasChange = (status: string | null) => Boolean(status?.trim() && status !== '.' && status !== '?');
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
  const [searchField, setSearchField] = useState<HistorySearch['field']>('message');
  const [searchText, setSearchText] = useState('');
  const [search, setSearch] = useState<HistorySearch | null>(null);
  const searchInput = useRef<HTMLInputElement>(null);
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
  const [mergeSource, setMergeSource] = useState('');
  const [showCombine, setShowCombine] = useState(false);
  const [refreshVersion, setRefreshVersion] = useState(0);
  const generation = useRef(0);
  const loadingPage = useRef<number | null>(null);
  const detailGeneration = useRef(0);
  const submittedDrafts = useRef(new Map<string, string>());
  const seenOperations = useRef(new Map<string, string>());
  const dialog = useRef<HTMLDialogElement>(null);
  const active = busy || operations.some(operation => !terminal(operation));
  const graph = useMemo(() => search
    ? (history?.commits ?? []).map(commit => ({ commit, lane: 0, width: 0, segments: [], pending: [] }))
    : layoutPage(history?.commits ?? []).rows, [history, search]);
  const clearSearch = useCallback(() => {
    if (!search) { setSearchText(''); return; }
    generation.current++; setSearch(null); setSearchText(''); setHistory(null); setLoading(true); setError(null);
  }, [search]);
  const changeScope = (next: string | null) => {
    if (next === scope) return;
    generation.current++; setHistory(null); setLoading(true); setScope(next);
  };
  const submitSearch = () => {
    if (!searchText.trim()) { setError('Enter a history search query.'); return; }
    if (/[\r\n]/.test(searchText)) { setError('History search must be a single line.'); return; }
    if (searchField === 'oid' && !/^[a-f0-9]{4,64}$/i.test(searchText)) { setError('Enter a commit OID or at least four hexadecimal characters.'); return; }
    generation.current++; setHistory(null); setLoading(true); setError(null); setSearch({ field: searchField, text: searchText });
  };

  const refresh = useCallback(async () => {
    const epoch = ++generation.current;
    setLoading(true);
    setError(null);
    setDiffRequest(null);
    try {
      const [nextStatus, nextChanges, nextRefs, nextRemotes, nextHistory] = await Promise.all([
        api.getStatus(id), api.getChanges(id), api.listRefs(id), api.listRemotes(id), search ? api.searchHistory(id, search, scope) : api.getHistory(id, scope),
      ]);
      if (epoch !== generation.current) return;
      setStatus(nextStatus); setChanges(nextChanges); setRefs(nextRefs); setRemotes(nextRemotes); setHistory(nextHistory);
      setRefreshVersion(version => version + 1);
      setRemoteId(previous => nextRemotes.some(remote => remote.id === previous) ? previous : (nextRemotes.find(remote => remote.defaultFetch)?.id ?? nextRemotes[0]?.id ?? ''));
    } catch (cause) { if (epoch === generation.current) setError(errorMessage(cause)); }
    finally { if (epoch === generation.current) setLoading(false); }
  }, [id, scope, search]);

  useEffect(() => { void refresh(); return () => { generation.current++; }; }, [refresh]);
  useEffect(() => {
    const onFocus = () => { void refresh(); };
    const onKey = (event: KeyboardEvent) => {
      if (document.querySelector('dialog[open]')) return;
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'f') {
        event.preventDefault(); searchInput.current?.focus(); searchInput.current?.select();
      } else if (event.key === 'Escape' && (search || searchText) && (event.target === searchInput.current || (event.target instanceof HTMLElement && event.target.closest('.history-list')))) {
        event.preventDefault(); clearSearch();
      } else if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'r') {
        event.preventDefault(); void refresh();
      }
    };
    window.addEventListener('focus', onFocus);
    window.addEventListener('keydown', onKey);
    return () => { window.removeEventListener('focus', onFocus); window.removeEventListener('keydown', onKey); };
  }, [refresh, search, searchText, clearSearch]);
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
    setShowCombine(false);
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
      if (action.kind === 'squashMerge' || action.kind === 'merge') {
        detailGeneration.current++; setSelected(null); setDetails(null); setDiffRequest(null);
      }
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
    if (!history?.nextCursor || loading || loadingPage.current === generation.current) return;
    const epoch = generation.current;
    loadingPage.current = epoch;
    setLoading(true);
    try {
      const next = search ? await api.searchHistory(id, search, scope, history.snapshotId, history.nextCursor) : await api.getHistory(id, scope, history.snapshotId, history.nextCursor);
      if (epoch !== generation.current) return;
      if (next.snapshotId !== history.snapshotId) throw new Error('History changed. Refresh to start a new history snapshot.');
      setHistory({ ...next, commits: [...history.commits, ...next.commits] });
    } catch (cause) { if (epoch === generation.current) setError(errorMessage(cause)); }
    finally { if (loadingPage.current === epoch) loadingPage.current = null; if (epoch === generation.current) setLoading(false); }
  };
  const working = () => { detailGeneration.current++; setSelected(null); setDetails(null); setDiffRequest(null); setShowCombine(false); };
  const headLabel = status.head.kind === 'detached' ? `Detached at ${status.head.oid.slice(0, 8)}` : `${status.head.name}${status.head.kind === 'unborn' ? ' (no commits)' : ''}`;

  return <section className="repository-screen" aria-label="Repository">
    <header className="repository-toolbar">
      {onBack && <button onClick={onBack}>← Back</button>}
      <div className="repository-title"><strong title={repository.displayPath}>{repository.displayPath}</strong><small>{headLabel} · {status.dirty ? 'Working changes' : 'Clean'}{status.upstream ? ` · ↑${status.upstream.ahead ?? '?'} ↓${status.upstream.behind ?? '?'} ${status.upstream.ref}` : ' · No upstream'}</small></div>
      <button title="Refresh (⌘R / Ctrl+R)" disabled={loading} onClick={() => { void refresh(); }}>{loading ? 'Refreshing…' : 'Refresh'}</button>
      <label>Remote <select aria-label="Remote" value={remoteId} onChange={event => setRemoteId(event.target.value)}><option value="">None</option>{remotes.map(remote => <option key={remote.id} value={remote.id}>{remote.name}</option>)}</select></label>
      <button disabled={active || !remoteId} onClick={() => { void act({ kind: 'fetch', remoteId }, true); }}>Fetch</button>
      <button disabled={active} onClick={() => { void act({ kind: 'pull', statusSnapshotId: status.snapshotId }, true); }}>Pull</button>
      <button onClick={() => { setShowCombine(true); setDiffRequest(null); }}>Combine commits…</button>
      <details className="push-controls"><summary>Push…</summary><div>
        <label>Destination ref<input value={destination} onChange={event => setDestination(event.target.value)} placeholder="refs/heads/main" /></label>
        <label><input type="checkbox" checked={setUpstream} onChange={event => setSetUpstream(event.target.checked)} /> Set upstream</label>
        <button disabled={active || !remoteId || !destination} onClick={() => { void act({ kind: 'push', statusSnapshotId: status.snapshotId, remoteId, destinationRef: destination, setUpstream }, true); }}>Preview push</button>
      </div></details>
    </header>
    {error && <div role="alert" className="repository-error">{error}<button onClick={() => setError(null)} aria-label="Dismiss error">×</button></div>}
    {status.gitState === 'merge' ? <div role="status" className="repository-notice merge-notice">
      <div><strong>Merge in progress</strong><p>{status.counts.conflicted > 0 ? 'Resolve each conflicted file in your editor, refresh, then stage its resolution.' : 'Review the staged merge result, then continue to create the merge commit.'}</p></div>
      <div className="merge-notice-actions"><button onClick={working}>Review working changes</button><button disabled={active || status.counts.conflicted > 0} onClick={() => { void act({ kind: 'continueMerge', statusSnapshotId: status.snapshotId }, true); }}>Continue merge</button><button disabled={active} onClick={() => { void act({ kind: 'abortMerge', statusSnapshotId: status.snapshotId }, true); }}>Abort merge</button></div>
    </div> : status.gitState === 'squashMerge' ? <div role="status" className="repository-notice merge-notice">
      <div><strong>Squash merge review</strong><p>{status.counts.conflicted > 0 ? 'Resolve conflicts in your editor, refresh, and stage each resolution. Then review and commit the result below.' : 'Review the staged changes and enter a message below to create one normal commit.'} Squash merge does not create MERGE_HEAD. There is no ordinary merge to continue or abort; keep any wanted changes and use Git externally for manual cleanup.</p>
        {status.squashMerge && <small>Source: {status.squashMerge.sourceRef} ({status.squashMerge.sourceOid.slice(0, 8)}) · Original HEAD: {status.squashMerge.originalHead}</small>}
      </div><button onClick={working}>Review working changes</button>
    </div> : status.gitState !== 'idle' && <div role="status" className="repository-notice">{status.gitState} in progress. Resolve conflicts in your editor, then refresh. Incompatible actions will be blocked.</div>}
    {status.submoduleDirty && <div className="repository-notice">A submodule has working changes. Open the submodule separately to review it.</div>}
    <div className="repository-panels">
      <aside className="repository-refs">
        <h2>History scope</h2><button className={scope === null ? 'selected' : ''} onClick={() => changeScope(null)}>All branches and tags</button>
        {(['local', 'remote', 'tag'] as const).map(kind => <div key={kind}><h3>{kind === 'local' ? 'Local branches' : kind === 'remote' ? 'Remote branches' : 'Tags'}</h3>{refs.filter(ref => ref.kind === kind).map(ref => <button title={ref.fullName} key={ref.fullName} className={scope === ref.fullName ? 'selected' : ''} onClick={() => changeScope(ref.fullName)}>{ref.label}</button>)}</div>)}
        <details><summary>Switch branch</summary><label>Local branch<select value={branchTarget} onChange={event => setBranchTarget(event.target.value)}><option value="">Choose branch</option>{refs.filter(ref => ref.kind === 'local').map(ref => <option key={ref.fullName} value={ref.fullName}>{ref.label}</option>)}</select></label><button disabled={active || !branchTarget} onClick={() => { void act({ kind: 'switchBranch', statusSnapshotId: status.snapshotId, localRef: branchTarget }, true); }}>Preview switch</button></details>
        <details><summary>Create branch</summary><label>Name<input value={branchName} onChange={event => setBranchName(event.target.value)} /></label><label><input type="checkbox" checked={switchNew} onChange={event => setSwitchNew(event.target.checked)} /> Switch after creating</label><button disabled={active || !branchName || status.head.kind === 'unborn'} onClick={() => { if (status.head.kind !== 'unborn') void act({ kind: 'createBranch', statusSnapshotId: status.snapshotId, name: branchName, startOid: selected ?? status.head.oid, switchAfterCreate: switchNew }, true); }}>Preview create</button><small>Starts at {selected ? selected.slice(0, 8) : 'HEAD'}.</small></details>
        <details className="merge-controls"><summary>Merge branches</summary>
          <label>Source branch<select aria-label="Merge source branch" value={mergeSource} onChange={event => setMergeSource(event.target.value)}><option value="">Choose source</option>{refs.filter(ref => (ref.kind === 'local' || ref.kind === 'remote') && (status.head.kind !== 'branch' || ref.fullName !== `refs/heads/${status.head.name}`)).map(ref => <option key={ref.fullName} value={ref.fullName}>{ref.label}</option>)}</select></label>
          <small>Target: {headLabel}. Start with a clean working tree.</small>
          <button disabled={active || !mergeSource || status.gitState !== 'idle' || status.head.kind !== 'branch'} onClick={() => { void act({ kind: 'merge', statusSnapshotId: status.snapshotId, sourceRef: mergeSource }, true); }}>Merge branch</button>
          <button disabled={active || !mergeSource || status.gitState !== 'idle' || status.head.kind !== 'branch'} onClick={() => { void act({ kind: 'squashMerge', statusSnapshotId: status.snapshotId, sourceRef: mergeSource }, true); }}>Squash merge branch</button>
          <small>Squash merge stages the branch changes for review and an explicit commit.</small>
        </details>
      </aside>
      <section className="repository-history" aria-label="History">
        <form className="history-search" aria-label="Search history" onSubmit={event => { event.preventDefault(); submitSearch(); }}>
          <label>Search by<select aria-label="History search field" value={searchField} onChange={event => setSearchField(event.target.value as HistorySearch['field'])}><option value="message">Message</option><option value="author">Author</option><option value="oid">Commit OID</option></select></label>
          <input ref={searchInput} aria-label="History search query" type="search" maxLength={512} value={searchText} onChange={event => setSearchText(event.target.value)} placeholder={searchField === 'oid' ? 'At least four hex characters' : 'Literal text, case-insensitive'} title="Search history (⌘F / Ctrl+F); Escape clears search" />
          <div><button type="submit" disabled={!searchText.trim()}>Search history</button>{search && <button type="button" onClick={clearSearch}>Clear search</button>}</div>
          {search && <small role="status">{loading ? 'Searching…' : history ? `${history.commits.length} matches loaded` : 'Search unavailable. Retry or clear the search.'} · {search.field}: {search.text} · {scope ?? 'All branches and tags'}. Results omit graph edges.</small>}
        </form>
        <button className={`working-entry ${selected === null ? 'selected' : ''}`} onClick={working}>Working changes <small>{status.counts.staged} staged · {status.counts.unstaged} unstaged · {status.counts.untracked} untracked · {status.counts.conflicted} conflicts</small></button>
        <HistoryGraph key={`${scope ?? ''}:${search ? `${search.field}:${search.text}` : 'history'}`} rows={graph} selected={selected} onSelect={oid => { void inspect(oid); }} loading={loading} failed={!history} searchResults={search !== null} more={Boolean(history?.nextCursor)} onMore={() => { void loadMore(); }} />
      </section>
      <section className="repository-details" aria-label={showCombine ? 'History rewrite' : selected ? 'Commit details' : 'Working changes'}>
        {showCombine ? <CombineCommits repositoryId={id} status={status} active={active} refreshVersion={refreshVersion} onPlan={action => { void act(action, true); }} onClose={working} /> : selected ? <>
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
              const files = changes.files.filter(file => section === 'conflicted' ? file.conflicted : !file.conflicted && (section === 'untracked' ? file.untracked : section === 'staged' ? hasChange(file.indexStatus) : hasChange(file.worktreeStatus) && !file.untracked));
              return <div key={section}><h3>{section[0].toUpperCase() + section.slice(1)} ({files.length})</h3><ul className="file-list">{files.map(file => <li key={file.fileId}>
                <button title={file.displayPath} onClick={() => setDiffRequest({ repositoryId: id, snapshotId: changes.snapshotId, fileId: file.fileId, context: section === 'staged' ? 'staged' : 'unstaged' })}>{file.oldDisplayPath ? `${file.oldDisplayPath} → ` : ''}{file.displayPath}{file.submodule ? ' (submodule)' : ''}</button>
                <button disabled={active} onClick={() => { void act({ kind: section === 'staged' ? 'unstage' : 'stage', snapshotId: changes.snapshotId, fileIds: [file.fileId] }); }}>{section === 'staged' ? 'Unstage' : file.conflicted ? 'Stage resolution' : 'Stage'}</button>
              </li>)}</ul></div>;
            })}
            {!changes.files.length && <p className="empty">Working tree clean.</p>}
            <form className="commit-form" onSubmit={event => { event.preventDefault(); void act({ kind: 'commit', statusSnapshotId: status.snapshotId, message: draft }); }}>
              <label htmlFor="commit-message">Commit message</label><textarea id="commit-message" value={draft} placeholder="Describe the staged changes" onChange={event => { setDraft(event.target.value); drafts.set(id, event.target.value); }} rows={4} />
              <small>Only staged content is committed. Drafts stay with this repository while the app is open.</small>
              {status.gitState === 'merge' && <small>Use Continue merge above to finish this merge.</small>}
              <button type="submit" disabled={active || status.gitState === 'merge' || status.counts.staged === 0 || status.counts.conflicted > 0 || !draft.trim()}>Commit staged changes ({status.counts.staged})</button>
            </form>
          </>}
        </>}
        {diffRequest && <section className="diff-panel" aria-label="File diff"><h3>Diff · {diffRequest.context}</h3>{!diff ? <p>Loading diff…</p> : diff.kind === 'text' ? <>{diff.truncated && <p role="status">Preview truncated: this is not the full patch.</p>}<pre className="unified-diff">{diff.text.split('\n').map((line, index) => <span key={index} className={line.startsWith('+') ? 'diff-add' : line.startsWith('-') ? 'diff-remove' : line.startsWith('@@') ? 'diff-hunk' : ''}>{line}{'\n'}</span>)}</pre>{!diff.text && <p>No text changes.</p>}</> : <p>{diff.kind === 'binary' ? diff.description : diff.reason}</p>}</section>}
      </section>
    </div>
    <details className="repository-activity"><summary>Activity · {operations.length ? `${operations[operations.length - 1].action}: ${operations[operations.length - 1].state}` : 'No operations this session'}</summary><ol>{operations.slice(-30).reverse().map(operation => <li key={operation.operationId}><strong>{operation.action}: {operation.state}</strong>{operation.reason && ` · ${operation.reason}`}{operation.error && <span role="alert"> · {operation.error.message}</span>}{operation.outcomeUncertain && ' · Outcome uncertain. Refresh and inspect the repository before retrying.'}{operation.outputTruncated && ' · Operation output truncated.'}</li>)}</ol></details>
    <dialog ref={dialog} onCancel={() => setPlanned(null)} aria-labelledby="action-preview-title">
      {planned && <><h2 id="action-preview-title">Review {planned.action.kind === 'squashMerge' ? 'squash merge' : planned.action.kind}</h2><p>{planned.plan.summary}</p>{(planned.action.kind === 'merge' || planned.action.kind === 'squashMerge') && <p><strong>Source:</strong> {planned.action.sourceRef}<br /><strong>Current target:</strong> {headLabel}</p>}{planned.plan.destination && <p><strong>Destination:</strong> {planned.plan.destination}</p>}{planned.action.kind === 'squashMerge' && <p>The result stays staged for review. You must explicitly create the normal commit afterward. Squash conflicts have no ordinary merge abort.</p>}{planned.action.kind === 'abortMerge' && <p>Abort the current ordinary merge. Git will attempt to restore the state before the merge; inspect any changes you made during conflict resolution before proceeding.</p>}{planned.action.kind === 'combineCommits' && <><p>This rewrites history and creates a backup. The selected commits may already be published.</p><ol>{planned.action.commitOids.map(oid => <li key={oid}><code>{oid}</code></li>)}</ol><pre className="commit-message">{planned.action.message}</pre></>}{planned.action.kind === 'recoverCombine' && <p>Recovery rechecks the original branch and backup, and preserves working-file edits. Later branch commits require manual review.</p>}<p>Preconditions are checked again before execution.</p><div className="dialog-actions"><button onClick={() => setPlanned(null)}>Cancel</button><button disabled={busy} onClick={() => { void execute(planned); }}>Run {planned.action.kind}</button></div></>}
    </dialog>
  </section>;
}
