import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { api, errorMessage } from '../../api/client';
import type { Changes, CommitDetails, DiffRequest, DiffResult, GitRef, HistoryPage, HistorySearch, OperationResult, Remote, RepositoryAction, RepositoryActionPlan, RepositoryOpen } from '../../api/types';
import { HistoryGraph } from '../../graph/HistoryGraph';
import { layoutPage } from '../../graph/layout';
import { CombineCommits } from './CombineCommits';
import { RemoteCheckout } from './RemoteCheckout';
import { Rebase } from './Rebase';
import { OperationContext } from './OperationContext';
import { Stashes } from './Stashes';
import { WorkingDiff } from './WorkingDiff';
import { ChangesWorkspace } from './ChangesWorkspace';
import { CommitFiles } from './CommitFiles';
import { ToolbarPopover } from '../../app/ToolbarPopover';
import { Icon } from '../../app/Icon';
import { useViewPreferences, PANEL_LIMITS } from './useViewPreferences';
import { ViewPreferencesMenu } from './ViewPreferencesMenu';
import { PanelDivider } from './PanelDivider';
import { useHistorySidebar } from './useHistorySidebar';
import { ActivityDrawer } from './ActivityDrawer';
import { FileHistory } from './FileHistory';
import { primaryShortcut, shortcutAria, shortcutLabel, shortcutsBlocked } from '../../app/shortcuts';
import './repository.css';

const drafts = new Map<string, string>();
const terminal = (operation: OperationResult) => !['queued', 'running'].includes(operation.state);
const isCherryAction = (kind: string) => ['cherryPick', 'continueCherryPick', 'abortCherryPick', 'skipCherryPick'].includes(kind);
const isRebaseAction = (kind: string) => ['startRebase', 'continueRebase', 'abortRebase', 'skipRebase'].includes(kind);
const isHunkAction = (kind: string) => kind === 'stageHunk' || kind === 'unstageHunk';
const isStashAction = (kind: string) => ['createStash', 'applyStash', 'deleteStash', 'finalizeStashDelete'].includes(kind);
const actionLabels: Record<string, string> = { createStash: 'create stash', applyStash: 'apply stash', deleteStash: 'delete stash', finalizeStashDelete: 'finalize stash deletion', cherryPick: 'cherry-pick', continueCherryPick: 'continue cherry-pick', abortCherryPick: 'abort cherry-pick', skipCherryPick: 'skip empty cherry-pick', startRebase: 'rebase branch', continueRebase: 'continue rebase', abortRebase: 'abort rebase', skipRebase: 'skip commit' };
const actionLabel = (kind: string) => actionLabels[kind] ?? kind;
const hasChange = (status: string | null) => Boolean(status?.trim() && status !== '.' && status !== '?');
type Planned = { plan: Extract<RepositoryActionPlan, { kind: 'eligible' }>; action: RepositoryAction; cherryMarkerOnly?: boolean };
export function RepositoryView(props: { repository: RepositoryOpen; onBack?: () => void }) {
  return <RepositoryScreen key={props.repository.repositoryId} {...props} />;
}

function RepositoryScreen({ repository, onBack }: { repository: RepositoryOpen; onBack?: () => void }) {
  const id = repository.repositoryId;
  const view = useViewPreferences();
  const { preferences } = view;
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
  const showCommitInfo = preferences.showCommitDetails;
  const [workspace, setWorkspace] = useState<'changes' | 'history'>(preferences.openingView);
  const sidebar = useHistorySidebar(workspace === 'history');
  const [focusSearch, setFocusSearch] = useState(false);
  const [details, setDetails] = useState<CommitDetails | null>(null);
  const [diff, setDiff] = useState<DiffResult | null>(null);
  const [diffRequest, updateDiffRequest] = useState<DiffRequest | null>(null);
  const [diffFileName, setDiffFileName] = useState('');
  const [fileHistory, setFileHistory] = useState<{ request: DiffRequest; displayPath: string; previousPath: string | null } | null>(null);
  const fileHistoryRequest = useMemo(() => fileHistory ? { repositoryId: id, source: { snapshotId: fileHistory.request.snapshotId, fileId: fileHistory.request.fileId, path: 'current' as const } } : null, [fileHistory, id]);
  const diffGeneration = useRef(0);
  const diffSelection = useRef<{ request: DiffRequest; displayPath: string } | null>(null);
  const setDiffRequest = useCallback((request: DiffRequest | null, displayPath = '') => {
    diffGeneration.current++;
    diffSelection.current = request ? { request, displayPath } : null;
    updateDiffRequest(request); setDiffFileName(displayPath); setDiff(null);
  }, []);
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
  const [showStashes, setShowStashes] = useState(false);
  const [refreshVersion, setRefreshVersion] = useState(0);
  const generation = useRef(0);
  const loadingPage = useRef<number | null>(null);
  const detailGeneration = useRef(0);
  const submittedDrafts = useRef(new Map<string, string>());
  const seenOperations = useRef(new Map<string, string>());
  const dialog = useRef<HTMLDialogElement>(null);
  const switchControls = useRef<HTMLDetailsElement>(null);
  const branchSidebar = sidebar.panel;
  const [branchPanel, setBranchPanel] = useState<string | null>(null);
  const executing = useRef(false);
  const planning = useRef(false);
  const active = busy || Boolean(status.activeOperationId) || operations.some(operation => !terminal(operation));
  const cherryReviewRequired = status.operationContext?.kind === 'cherryPick' && status.operationContext.stopReason === 'unknown';
  const rebaseReviewRequired = status.operationContext?.kind === 'rebase' && status.operationContext.stopReason === 'unknown';
  const operationReviewRequired = cherryReviewRequired || rebaseReviewRequired;
  const graph = useMemo(() => search
    ? (history?.commits ?? []).map(commit => ({ commit, lane: 0, width: 0, segments: [], pending: [] }))
    : layoutPage(history?.commits ?? []).rows, [history, search]);
  const clearSearch = useCallback(() => {
    if (!search) { setSearchText(''); return; }
    generation.current++; setSearch(null); setSearchText(''); setHistory(null); setLoading(true); setError(null);
  }, [search]);
  const changeScope = (next: string | null) => {
    if (sidebar.compact) sidebar.close();
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
    const selectedDiff = diffSelection.current;
    diffGeneration.current++; updateDiffRequest(null); setDiff(null);
    try {
      const [nextStatus, nextChanges, nextRefs, nextRemotes, nextHistory] = await Promise.all([
        api.getStatus(id), api.getChanges(id), api.listRefs(id), api.listRemotes(id), search ? api.searchHistory(id, search, scope) : api.getHistory(id, scope),
      ]);
      if (epoch !== generation.current) return;
      setStatus(nextStatus); setChanges(nextChanges); setRefs(nextRefs); setRemotes(nextRemotes); setHistory(nextHistory);
      setRefreshVersion(version => version + 1);
      if (diffSelection.current === selectedDiff) {
        const context = selectedDiff?.request.context;
        const file = context && context !== 'commit' ? nextChanges.files.find(item => item.displayPath === selectedDiff.displayPath && (context === 'staged' ? hasChange(item.indexStatus) : hasChange(item.worktreeStatus) || item.untracked || item.conflicted)) : null;
        setDiffRequest(file && context ? { repositoryId: id, snapshotId: nextChanges.snapshotId, fileId: file.fileId, context } : null, file?.displayPath);
      }
      setRemoteId(previous => nextRemotes.some(remote => remote.id === previous) ? previous : (nextRemotes.find(remote => remote.defaultFetch)?.id ?? nextRemotes[0]?.id ?? ''));
    } catch (cause) { if (epoch === generation.current) setError(errorMessage(cause)); }
    finally { if (epoch === generation.current) setLoading(false); }
  }, [id, scope, search, setDiffRequest]);
  const currentRefresh = useRef(refresh);
  currentRefresh.current = refresh;

  useEffect(() => { void refresh(); return () => { generation.current++; }; }, [refresh]);
  useEffect(() => {
    const onFocus = () => { void refresh(); };
    const onKey = (event: KeyboardEvent) => {
      if (shortcutsBlocked() || event.isComposing || event.defaultPrevented) return;
      if (primaryShortcut(event, 'f')) {
        event.preventDefault(); setWorkspace('history'); setShowCombine(false); setShowStashes(false); setDiffRequest(null); setFocusSearch(true);
      } else if (event.key === 'Escape' && (search || searchText) && (event.target === searchInput.current || (event.target instanceof HTMLElement && event.target.closest('.history-list')))) {
        event.preventDefault(); clearSearch();
      } else if (primaryShortcut(event, 'r')) {
        event.preventDefault(); void refresh();
      }
    };
    window.addEventListener('focus', onFocus);
    window.addEventListener('keydown', onKey);
    return () => { window.removeEventListener('focus', onFocus); window.removeEventListener('keydown', onKey); };
  }, [refresh, search, searchText, clearSearch, setDiffRequest]);
  useEffect(() => {
    if (focusSearch && workspace === 'history') {
      searchInput.current?.focus(); searchInput.current?.select(); setFocusSearch(false);
    }
  }, [focusSearch, workspace]);
  useEffect(() => {
    if (!branchPanel || workspace !== 'history') return;
    const control = branchSidebar.current?.querySelector<HTMLDetailsElement>(branchPanel);
    if (control) {
      control.open = true;
      control.querySelector('summary')?.focus();
      control.scrollIntoView?.({ block: 'nearest' });
    }
    setBranchPanel(null);
  }, [branchPanel, workspace, branchSidebar]);
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
            if (operation.state === 'failed' || ((operation.action === 'checkoutRemoteBranch' || isCherryAction(operation.action) || isStashAction(operation.action) || isHunkAction(operation.action) || isRebaseAction(operation.action)) && !['succeeded', 'stopped'].includes(operation.state))) failure = operation.error?.message ?? operation.reason ?? `Operation ${operation.state}. Inspect activity and current repository state.`;
          }
          seenOperations.current.set(operation.operationId, operation.state);
          if (terminal(operation) && submittedDrafts.current.has(operation.operationId)) {
            const submitted = submittedDrafts.current.get(operation.operationId);
            submittedDrafts.current.delete(operation.operationId);
            if (operation.state === 'succeeded' && drafts.get(id) === submitted) { drafts.delete(id); setDraft(''); }
            changed = true;
          }
        }
        if (changed) await currentRefresh.current();
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
    setShowCombine(false); setShowStashes(false);
    const epoch = ++detailGeneration.current;
    setSelected(oid); setDetails(null); setDiffRequest(null); setError(null);
    try { const result = await api.getCommit(id, oid, parent); if (epoch === detailGeneration.current) setDetails(result); }
    catch (cause) { if (epoch === detailGeneration.current) setError(errorMessage(cause)); }
  };
  useEffect(() => {
    let disposed = false;
    const epoch = diffGeneration.current;
    setDiff(null);
    if (diffRequest) api.getDiff(diffRequest).then(result => { if (!disposed && epoch === diffGeneration.current) setDiff(result); }).catch(cause => { if (!disposed && epoch === diffGeneration.current) setDiff({ kind: 'unavailable', reason: errorMessage(cause) }); });
    return () => { disposed = true; };
  }, [diffRequest]);

  const execute = async ({ plan, action }: Planned) => {
    if (executing.current) return;
    executing.current = true;
    setBusy(true); setPlanned(null); setError(null);
    try {
      const operation = await api.runAction(plan.planId);
      if (action.kind === 'squashMerge' || action.kind === 'merge' || isCherryAction(action.kind) || isRebaseAction(action.kind)) {
        detailGeneration.current++; setSelected(null); setDetails(null); setDiffRequest(null); setWorkspace('changes');
      }
      seenOperations.current.set(operation.operationId, operation.state);
      setOperations(old => [...old.filter(item => item.operationId !== operation.operationId), operation]);
      if (action.kind === 'commit') {
        if (operation.state === 'succeeded' && drafts.get(id) === action.message) { drafts.delete(id); setDraft(''); }
        else if (!terminal(operation)) submittedDrafts.current.set(operation.operationId, action.message);
      }
      if (terminal(operation)) await currentRefresh.current();
      if (operation.state === 'failed' || ((action.kind === 'checkoutRemoteBranch' || isCherryAction(action.kind) || isStashAction(action.kind) || isHunkAction(action.kind) || isRebaseAction(action.kind)) && terminal(operation) && !['succeeded', 'stopped'].includes(operation.state))) setError(operation.error?.message ?? operation.reason ?? `Operation ${operation.state}. Inspect activity and current repository state.`);
    } catch (cause) {
      if (action.kind === 'checkoutRemoteBranch' || isCherryAction(action.kind) || isStashAction(action.kind) || isHunkAction(action.kind) || isRebaseAction(action.kind)) await currentRefresh.current();
      setError(errorMessage(cause));
    }
    finally { executing.current = false; setBusy(false); }
  };
  const act = async (action: RepositoryAction, preview = false, isCurrent?: () => boolean) => {
    if (planning.current) return;
    planning.current = true;
    const epoch = generation.current;
    const detailEpoch = detailGeneration.current;
    const diffEpoch = diffGeneration.current;
    setBusy(true); setError(null);
    try {
      const plan = await api.planAction(id, action);
      if ((action.kind === 'checkoutRemoteBranch' || isCherryAction(action.kind) || isStashAction(action.kind) || isHunkAction(action.kind) || isRebaseAction(action.kind)) && (epoch !== generation.current || ((action.kind === 'cherryPick' || isStashAction(action.kind) || isHunkAction(action.kind)) && detailEpoch !== detailGeneration.current) || (isHunkAction(action.kind) && diffEpoch !== diffGeneration.current) || (isCurrent && !isCurrent()))) {
        if (isHunkAction(action.kind)) await currentRefresh.current();
        setError(`Repository view changed while preparing ${action.kind === 'checkoutRemoteBranch' ? 'checkout' : isStashAction(action.kind) ? 'stash action' : isHunkAction(action.kind) ? 'hunk action' : isRebaseAction(action.kind) ? 'rebase' : 'cherry-pick'}. Review the source and create a new preview.`);
        return;
      }
      if (plan.kind === 'blocked') { if (isStashAction(action.kind) || isHunkAction(action.kind)) await currentRefresh.current(); setError(plan.reason); }
      else if (preview) setPlanned({ plan, action, cherryMarkerOnly: action.kind === 'abortCherryPick' && status.gitState === 'other' && status.operationContext?.kind === 'cherryPick' });
      else await execute({ plan, action });
    } catch (cause) { if (isStashAction(action.kind) || isHunkAction(action.kind)) await currentRefresh.current(); setError(errorMessage(cause)); }
    finally { planning.current = false; setBusy(false); }
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
  const working = () => { detailGeneration.current++; setWorkspace('changes'); setSelected(null); setDetails(null); setDiffRequest(null); setShowCombine(false); setShowStashes(false); };
  const browseHistory = () => {
    if (workspace === 'history' && !showCombine && !showStashes) return;
    detailGeneration.current++; setSelected(null); setDetails(null); setWorkspace('history'); setDiffRequest(null); setShowCombine(false); setShowStashes(false);
  };
  const cancelPreview = () => { const refreshStashes = planned && isStashAction(planned.action.kind); setPlanned(null); if (refreshStashes) void currentRefresh.current(); };
  const headLabel = status.head.kind === 'detached' ? `Detached at ${status.head.oid.slice(0, 8)}` : `${status.head.name}${status.head.kind === 'unborn' ? ' (no commits)' : ''}`;
  const repositoryName = repository.displayPath.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || repository.displayPath;
  const statusSummary = `${status.dirty ? 'Working changes' : 'Clean'}${status.upstream ? ` · ↑${status.upstream.ahead ?? '?'} ↓${status.upstream.behind ?? '?'} ${status.upstream.ref}` : ' · No upstream'}`;
  const openBranchPanel = (selector: string) => {
    // Navigating to a form does not change history selection or Git state.
    if (showCombine || showStashes) { detailGeneration.current++; setSelected(null); setDetails(null); }
    sidebar.reveal();
    setWorkspace('history'); setShowCombine(false); setShowStashes(false); setDiffRequest(null); setBranchPanel(selector);
  };
  const fileDiff = diffRequest ? <WorkingDiff key={`${diffRequest.snapshotId}:${diffRequest.fileId}:${diffRequest.context}`} request={diffRequest} displayPath={diffFileName} diff={diff} layout={preferences.diffLayout} onLayoutChange={diffLayout => view.update({ diffLayout })} onFileHistory={() => setFileHistory({ request: diffRequest, displayPath: diffFileName, previousPath: (diffRequest.context === 'commit' ? details?.files : changes?.files)?.find(file => file.fileId === diffRequest.fileId)?.oldDisplayPath ?? null })} active={active || loading} blockedReason={operationReviewRequired ? 'Resolve the recorded Git operation context before staging or unstaging hunks.' : null} onAction={action => { void act(action); }} /> : null;
  const composer = <form className="commit-form" onSubmit={event => { event.preventDefault(); void act({ kind: 'commit', statusSnapshotId: status.snapshotId, message: draft }); }}>
    <label htmlFor="commit-message">Commit message</label><textarea id="commit-message" value={draft} placeholder="Describe the staged changes" onChange={event => { setDraft(event.target.value); drafts.set(id, event.target.value); }} rows={3} />
    <small>Only staged content is committed. Draft kept for this session.</small>
    {status.gitState === 'merge' && <small>Use Continue merge above to finish this merge.</small>}
    {status.gitState === 'cherryPick' && <small>Use the cherry-pick controls above to continue, skip an empty result, or abort. This draft is retained separately.</small>}
    {status.gitState === 'rebase' && <small>Use the rebase controls above to continue, skip, or abort. This draft is retained separately.</small>}
    <button className="primary-action" type="submit" disabled={active || loading || !changes || operationReviewRequired || !['idle', 'squashMerge'].includes(status.gitState) || status.counts.staged === 0 || status.counts.conflicted > 0 || !draft.trim()}>Commit staged changes ({status.counts.staged})</button>
  </form>;

  return <section className="repository-screen" aria-label="Repository" style={{ '--changes-width': `${preferences.changesWidth}px`, '--branches-width': `${preferences.branchesWidth}px`, '--history-width': `${preferences.historyWidth}px` } as CSSProperties}>
    {fileHistory && fileHistoryRequest && <FileHistory request={fileHistoryRequest} displayPath={fileHistory.displayPath} previousPath={fileHistory.previousPath} layout={preferences.diffLayout} onLayoutChange={diffLayout => view.update({ diffLayout })} onClose={() => setFileHistory(null)} />}
    <header className="repository-toolbar">
      {onBack && <button onClick={onBack}>← Back</button>}
      <div className="repository-title"><strong title={repository.displayPath}>{repositoryName}</strong><small title={statusSummary}>{statusSummary}</small></div>
      <ToolbarPopover name="Branch actions" title={`Branch actions · ${headLabel}`} className="branch-actions" align="left" label={<><Icon name="branch" /><span className="branch-label">{headLabel}</span><Icon name="chevron" /></>}>{close => <>
        <p className="popover-caption">Current branch · {headLabel}</p>
        <button onClick={() => { close(); openBranchPanel('.switch-controls'); }}>Switch branch…</button>
        <button onClick={() => { close(); openBranchPanel('.create-controls'); }}>Create branch…</button>
        <button onClick={() => { close(); openBranchPanel('.remote-checkout'); }}>Check out remote branch…</button>
        <div className="popover-separator" />
        <button onClick={() => { close(); openBranchPanel('.merge-controls'); }}>Merge branches…</button>
        <button onClick={() => { close(); openBranchPanel('.rebase-controls'); }}>Rebase branch…</button>
      </>}</ToolbarPopover>
      <div className="repository-remote-actions" role="group" aria-label="Remote actions">
      <button className="refresh-action" aria-label={loading ? 'Refreshing…' : 'Refresh'} aria-keyshortcuts={shortcutAria('r')} title={`Refresh (${shortcutLabel('r')})`} disabled={loading} onClick={() => { void refresh(); }}><Icon name="refresh" /></button>
      <select className="toolbar-remote" aria-label="Remote" title="Remote for Fetch and Push; Pull uses the current branch’s upstream" value={remoteId} onChange={event => setRemoteId(event.target.value)}><option value="">No remote</option>{remotes.map(remote => <option key={remote.id} value={remote.id}>{remote.name}</option>)}</select>
      <button aria-label="Fetch" title="Fetch the selected remote" disabled={active || !remoteId} onClick={() => { void act({ kind: 'fetch', remoteId }, true); }}><Icon name="fetch" />Fetch</button>
      <button aria-label="Pull" title={status.upstream ? `Pull from ${status.upstream.ref}; last known ${status.upstream.behind ?? 'unknown'} behind` : 'Review pull; no upstream configured'} disabled={active || operationReviewRequired} onClick={() => { void act({ kind: 'pull', statusSnapshotId: status.snapshotId }, true); }}><Icon name="pull" />Pull{status.upstream?.behind !== null && status.upstream?.behind !== undefined && status.upstream.behind > 0 && <span className="toolbar-count" aria-hidden="true">{status.upstream.behind}</span>}</button>
      <ToolbarPopover name="Push…" className="push-controls" label={<><Icon name="push" />Push<Icon name="chevron" /></>}>{close => <>
        <p className="popover-caption">Push to {remotes.find(remote => remote.id === remoteId)?.name ?? 'a remote'}</p>
        <label>Destination ref<input value={destination} onChange={event => setDestination(event.target.value)} placeholder="refs/heads/main" /></label>
        <label><input type="checkbox" checked={setUpstream} onChange={event => setSetUpstream(event.target.checked)} /> Set upstream</label>
        <small>Review the destination before pushing.</small>
        <button className="push-preview" disabled={active || operationReviewRequired || !remoteId || !destination} onClick={() => { close(); void act({ kind: 'push', statusSnapshotId: status.snapshotId, remoteId, destinationRef: destination, setUpstream }, true); }}>Preview push</button>
      </>}</ToolbarPopover>
      </div>
      <ToolbarPopover name="More repository actions" className="repository-more" label={<Icon name="more" />}>{close => <>
        <p className="popover-caption">History actions</p>
        <button aria-label="Find in history…" aria-keyshortcuts={shortcutAria('f')} onClick={() => { close(); setWorkspace('history'); setShowCombine(false); setShowStashes(false); setDiffRequest(null); setFocusSearch(true); }}>Find in history… <kbd className="shortcut-hint" aria-hidden="true">{shortcutLabel('f')}</kbd></button>
        <button aria-label="Refresh repository" aria-keyshortcuts={shortcutAria('r')} onClick={() => { close(); void refresh(); }}>Refresh repository <kbd className="shortcut-hint" aria-hidden="true">{shortcutLabel('r')}</kbd></button>
        <button onClick={() => { close(); detailGeneration.current++; setWorkspace('history'); setShowCombine(true); setShowStashes(false); setDiffRequest(null); }}>Combine commits…</button>
      </>}</ToolbarPopover>
    </header>
    <nav className="repository-workspaces" aria-label="Repository workspace">
      <button aria-pressed={workspace === 'changes'} onClick={working}>Changes <span className="file-count">{changes?.files.length ?? '…'}</span></button>
      <button aria-pressed={workspace === 'history' && !showStashes} onClick={browseHistory}>History</button>
      <button aria-pressed={workspace === 'history' && showStashes} onClick={() => { detailGeneration.current++; setWorkspace('history'); setShowStashes(true); setShowCombine(false); setDiffRequest(null); }}>Stashes</button>
      <small>{workspace === 'changes' ? 'Review · stage · commit' : 'Browse commits and manage branches'}</small>
      <ViewPreferencesMenu {...view} />
    </nav>
    {error && <div role="alert" className="repository-error">{error}<button onClick={() => setError(null)} aria-label="Dismiss error">×</button></div>}
    <OperationContext status={status} files={changes?.files ?? null} active={active} onPlan={action => { void act(action, true); }} onReview={working} />
    {status.submoduleDirty && <div className="repository-notice">A submodule has working changes. Open the submodule separately to review it.</div>}
    {workspace === 'changes' && <ChangesWorkspace changes={changes} selection={diffRequest} disabled={active || loading}
      sidebarWidth={preferences.changesWidth} onResize={width => view.update({ changesWidth: width })}
      blockedReason={operationReviewRequired ? 'Resolve the recorded Git operation context before staging or unstaging files.' : null}
      onSelect={(file, context) => { if (changes) setDiffRequest({ repositoryId: id, snapshotId: changes.snapshotId, fileId: file.fileId, context }, file.displayPath); }}
      onAction={action => { void act(action); }} composer={composer} diff={fileDiff} />}
    <div className={`repository-panels${sidebar.compact ? ' compact-history' : ''}`} hidden={workspace !== 'history'}>
      <aside id={sidebar.id} aria-label="History scope and branches" className="repository-refs" ref={branchSidebar} hidden={sidebar.compact && !sidebar.expanded}>
        <div className="history-scope-heading"><h2>History scope</h2><button ref={sidebar.closeButton} hidden={!sidebar.compact} aria-label="Close history scope" onClick={sidebar.close}>×</button></div>
        <button className={scope === null ? 'selected' : ''} onClick={() => changeScope(null)}>All branches and tags</button>
        {(['local', 'remote', 'tag'] as const).map(kind => <div key={kind}><h3>{kind === 'local' ? 'Local branches' : kind === 'remote' ? 'Remote branches' : 'Tags'}</h3>{refs.filter(ref => ref.kind === kind).map(ref => <button title={ref.fullName} key={ref.fullName} className={scope === ref.fullName ? 'selected' : ''} onClick={() => changeScope(ref.fullName)}>{ref.label}</button>)}</div>)}
        <details className="switch-controls" ref={switchControls}><summary>Switch branch</summary><label>Local branch<select value={branchTarget} onChange={event => setBranchTarget(event.target.value)}><option value="">Choose branch</option>{refs.filter(ref => ref.kind === 'local').map(ref => <option key={ref.fullName} value={ref.fullName}>{ref.label}</option>)}</select></label><button title={status.gitState !== 'idle' ? 'Finish the current Git operation before switching branches.' : undefined} disabled={active || status.gitState !== 'idle' || !branchTarget} onClick={() => { void act({ kind: 'switchBranch', statusSnapshotId: status.snapshotId, localRef: branchTarget }, true); }}>Preview switch</button></details>
        <RemoteCheckout repositoryId={id} status={status} refs={refs} active={active} refreshVersion={refreshVersion} onPlan={action => { void act(action, true); }} onSwitch={localRef => { setBranchTarget(localRef); if (switchControls.current) { switchControls.current.open = true; switchControls.current.querySelector('select')?.focus(); } }} />
        <Rebase status={status} refs={refs} active={active} onPlan={(action, isCurrent) => { void act(action, true, isCurrent); }} />
        <details className="create-controls"><summary>Create branch</summary><label>Name<input value={branchName} onChange={event => setBranchName(event.target.value)} /></label><label><input type="checkbox" checked={switchNew} onChange={event => setSwitchNew(event.target.checked)} /> Switch after creating</label><button title={status.gitState !== 'idle' ? 'Finish the current Git operation before creating a branch.' : undefined} disabled={active || status.gitState !== 'idle' || !branchName || status.head.kind === 'unborn'} onClick={() => { if (status.head.kind !== 'unborn') void act({ kind: 'createBranch', statusSnapshotId: status.snapshotId, name: branchName, startOid: selected ?? status.head.oid, switchAfterCreate: switchNew }, true); }}>Preview create</button><small>Starts at {selected ? selected.slice(0, 8) : 'HEAD'}.</small></details>
        <details className="merge-controls"><summary>Merge branches</summary>
          <label>Source branch<select aria-label="Merge source branch" value={mergeSource} onChange={event => setMergeSource(event.target.value)}><option value="">Choose source</option>{refs.filter(ref => (ref.kind === 'local' || ref.kind === 'remote') && (status.head.kind !== 'branch' || ref.fullName !== `refs/heads/${status.head.name}`)).map(ref => <option key={ref.fullName} value={ref.fullName}>{ref.label}</option>)}</select></label>
          <small>Target: {headLabel}. Start with a clean working tree.</small>
          <button disabled={active || !mergeSource || status.gitState !== 'idle' || status.head.kind !== 'branch'} onClick={() => { void act({ kind: 'merge', statusSnapshotId: status.snapshotId, sourceRef: mergeSource }, true); }}>Merge branch</button>
          <button disabled={active || !mergeSource || status.gitState !== 'idle' || status.head.kind !== 'branch'} onClick={() => { void act({ kind: 'squashMerge', statusSnapshotId: status.snapshotId, sourceRef: mergeSource }, true); }}>Squash merge branch</button>
          <small>Squash merge stages the branch changes for review and an explicit commit.</small>
        </details>
      </aside>
      {!sidebar.compact && <PanelDivider name="Resize branches panel" value={preferences.branchesWidth} min={PANEL_LIMITS.branchesWidth[0]} max={PANEL_LIMITS.branchesWidth[1]} onChange={width => view.update({ branchesWidth: width })} />}
      <section className="repository-history" aria-label="History">
        <button ref={sidebar.trigger} hidden={!sidebar.compact} className="history-scope-trigger" aria-label="History scope and branches" aria-expanded={sidebar.expanded} aria-controls={sidebar.id} title={scope ?? 'All branches and tags'} onClick={sidebar.toggle}><Icon name="branch" /><span>{scope ?? 'All branches and tags'}</span><Icon name="chevron" /></button>
        <form className="history-search" aria-label="Search history" onSubmit={event => { event.preventDefault(); submitSearch(); }}>
          <label>Search by<select aria-label="History search field" value={searchField} onChange={event => setSearchField(event.target.value as HistorySearch['field'])}><option value="message">Message</option><option value="author">Author</option><option value="oid">Commit OID</option></select></label>
          <input ref={searchInput} aria-label="History search query" aria-keyshortcuts={shortcutAria('f')} type="search" maxLength={512} value={searchText} onChange={event => setSearchText(event.target.value)} placeholder={searchField === 'oid' ? 'At least four hex characters' : 'Literal text, case-insensitive'} title={`Search history (${shortcutLabel('f')}); Escape clears search`} />
          <div><button type="submit" disabled={!searchText.trim()}>Search history</button>{search && <button type="button" onClick={clearSearch}>Clear search</button>}</div>
          {search && <small role="status">{loading ? 'Searching…' : history ? `${history.commits.length} matches loaded` : 'Search unavailable. Retry or clear the search.'} · {search.field}: {search.text} · {scope ?? 'All branches and tags'}. Results omit graph edges.</small>}
        </form>
        <button className={`working-entry ${selected === null ? 'selected' : ''}`} onClick={working}>Working changes <small>{status.counts.staged} staged · {status.counts.unstaged} unstaged · {status.counts.untracked} untracked · {status.counts.conflicted} conflicts</small></button>
        <HistoryGraph key={`${scope ?? ''}:${search ? `${search.field}:${search.text}` : 'history'}`} rows={graph} selected={selected} onSelect={oid => { void inspect(oid); }} loading={loading} failed={!history} searchResults={search !== null} more={Boolean(history?.nextCursor)} onMore={() => { void loadMore(); }} />
      </section>
      <PanelDivider name="Resize history panel" value={preferences.historyWidth} min={PANEL_LIMITS.historyWidth[0]} max={PANEL_LIMITS.historyWidth[1]} onChange={width => view.update({ historyWidth: width })} />
      <section className={`repository-details${!showStashes && !showCombine ? ' commit-inspector' : ''}`} aria-label={showStashes ? 'Stash workspace' : showCombine ? 'History rewrite' : 'Commit details'}>
        {showStashes ? <Stashes repositoryId={id} status={status} files={changes?.files ?? []} active={active} refreshVersion={refreshVersion} diffLayout={preferences.diffLayout} onDiffLayoutChange={diffLayout => view.update({ diffLayout })} onPlan={(action, isCurrent) => { void act(action, true, isCurrent); }} onClose={working} /> : showCombine ? <CombineCommits repositoryId={id} status={status} active={active} refreshVersion={refreshVersion} onPlan={action => { void act(action, true); }} onClose={working} /> : selected ? <>
          {!details ? <p className="empty">Loading commit…</p> : <>
            <div className="commit-inspector-heading"><h2 title={selected}>Commit {selected.slice(0, 8)}</h2><button aria-expanded={showCommitInfo} onClick={() => view.update({ showCommitDetails: !showCommitInfo })}>{showCommitInfo ? 'Hide details' : 'Show details'}</button></div>
            <div className="commit-overview" hidden={!showCommitInfo}>
            <pre className="commit-message">{details.message}</pre>
            <details className="commit-identities"><summary>Commit information</summary>
            <dl className="commit-metadata"><dt>Author</dt><dd>{details.author.name} &lt;{details.author.email}&gt; · {details.author.at}</dd><dt>Committer</dt><dd>{details.committer.name} &lt;{details.committer.email}&gt; · {details.committer.at}</dd><dt>Commit</dt><dd>{details.oid}</dd></dl>
            </details>
            <section className="cherry-pick-selection" aria-label="Cherry-pick selected commit">
              <p>Current target: {headLabel}{status.head.kind !== 'unborn' && ` at ${status.head.oid}`}. History scope does not change this target.</p>
              <p>{details.parents.length > 1 ? 'Merge commits are not supported by this single-commit cherry-pick workflow.' : status.head.kind !== 'branch' ? 'Switch to an existing local branch before cherry-picking.' : status.gitState !== 'idle' ? 'Finish the current Git operation before cherry-picking.' : status.dirty || status.submoduleDirty ? 'Cherry-pick requires a clean index and working tree, including untracked files and submodules.' : active ? 'Wait for the current repository operation to finish.' : 'Apply this commit’s changes to the current branch, preserving its message and author.'}</p>
              <button disabled={active || details.parents.length > 1 || status.head.kind !== 'branch' || status.gitState !== 'idle' || status.dirty || status.submoduleDirty} onClick={() => { void act({ kind: 'cherryPick', statusSnapshotId: status.snapshotId, sourceOid: details.oid }, true); }}>Cherry-pick commit</button>
            </section>
            </div>
            <div className="commit-comparison">{details.parents.length > 0 ? <label>Compare with parent<select aria-label="Compare with parent" value={details.selectedParent ?? ''} onChange={event => { void inspect(details.oid, event.target.value); }}>{details.parents.map((parent, index) => <option key={parent} value={parent}>Parent {index + 1}: {parent.slice(0, 8)}</option>)}</select></label> : <p>Root commit · compared with empty tree</p>}</div>
            <CommitFiles key={details.snapshotId} files={details.files} selection={diffRequest} onSelect={file => setDiffRequest({ repositoryId: id, snapshotId: details.snapshotId, fileId: file.fileId, context: 'commit' }, file.displayPath)} diff={workspace === 'history' ? fileDiff : null} />
          </>}
        </> : <div className="diff-empty"><h2>Select a commit</h2><p>Inspect its message and changed files, or return to Changes to prepare your next commit.</p></div>}
      </section>
    </div>
    <ActivityDrawer operations={operations} />
    <dialog ref={dialog} onCancel={cancelPreview} aria-labelledby="action-preview-title">
      {planned && <><h2 id="action-preview-title">Review {planned.cherryMarkerOnly ? 'clear interrupted cherry-pick' : planned.action.kind === 'checkoutRemoteBranch' ? 'remote checkout' : planned.action.kind === 'squashMerge' ? 'squash merge' : actionLabel(planned.action.kind)}</h2><p>{planned.plan.summary}</p>{(planned.action.kind === 'merge' || planned.action.kind === 'squashMerge') && <p><strong>Source:</strong> {planned.action.sourceRef}<br /><strong>Current target:</strong> {headLabel}</p>}{planned.plan.destination && <p><strong>Destination:</strong> {planned.plan.destination}</p>}{planned.action.kind === 'squashMerge' && <p>The result stays staged for review. You must explicitly create the normal commit afterward. Squash conflicts have no ordinary merge abort.</p>}{planned.action.kind === 'abortMerge' && <p>Abort the current ordinary merge. Git will attempt to restore the state before the merge; inspect any changes you made during conflict resolution before proceeding.</p>}{planned.cherryMarkerOnly && <p>Clear only the recorded app cherry-pick marker after Git state is rechecked. The reviewed branch, index, and working files remain unchanged.</p>}{planned.action.kind === 'abortCherryPick' && !planned.cherryMarkerOnly && <p>Abort this cherry-pick using the reviewed Git state. Git may discard edits made while resolving it, including unrelated staged edits, and restore files to the reviewed current HEAD. If original target context is unknown, no return to an earlier target is promised. Inspect actual state if Git cannot abort.</p>}{planned.action.kind === 'skipCherryPick' && <p>Finish this empty cherry-pick without creating a commit. The source commit will not be added to the target history.</p>}{planned.action.kind === 'applyStash' && <p>Apply retains the saved stash. Inspect actual working-file and index state after success or conflict; stash conflicts do not provide an ordinary merge abort.</p>}{planned.action.kind === 'deleteStash' && <p>Delete only the reviewed saved entry. Previously applied working changes remain; a recovery backup is retained.</p>}{planned.action.kind === 'finalizeStashDelete' && <p>Finalize may complete the previously reviewed stash deletion. The recovery backup remains; working files are not restored.</p>}{planned.action.kind === 'combineCommits' && <><p>This rewrites history and creates a backup. The selected commits may already be published.</p><ol>{planned.action.commitOids.map(oid => <li key={oid}><code>{oid}</code></li>)}</ol><pre className="commit-message">{planned.action.message}</pre></>}{planned.action.kind === 'recoverCombine' && <p>Recovery rechecks the original branch and backup, and preserves working-file edits. Later branch commits require manual review.</p>}{planned.plan.rebasePreview && <section aria-label="Rebase preview"><p><strong>Current branch:</strong> {planned.plan.rebasePreview.branch} at {planned.plan.rebasePreview.originalHead}</p><p><strong>Onto:</strong> {planned.plan.rebasePreview.ontoRef} at {planned.plan.rebasePreview.ontoOid}</p><p><strong>Merge base:</strong> {planned.plan.rebasePreview.baseOid}</p><p>These commits will be replayed in order with new IDs:</p><ol>{planned.plan.rebasePreview.commits.map(commit => <li key={commit.oid}>{commit.subject} · <code>{commit.oid}</code></li>)}</ol><p>A normal push may reject a branch whose old commits were published. Fetch and push are separate actions.</p></section>}{planned.action.kind === "skipRebase" && <p>Skip omits the commit being replayed. Git may discard edits made while resolving it. Review the exact commit and current files before confirming.</p>}{planned.action.kind === "abortRebase" && <p>Abort asks Git to restore the original branch and HEAD. Resolution edits may be discarded. Inspect actual files and refs afterward.</p>}<p>Preconditions are checked again before execution.</p><div className="dialog-actions"><button onClick={cancelPreview}>Cancel</button><button disabled={busy} onClick={() => { void execute(planned); }}>{planned.cherryMarkerOnly ? 'Clear interrupted cherry-pick' : planned.action.kind === 'checkoutRemoteBranch' ? 'Create and switch' : `Run ${actionLabel(planned.action.kind)}`}</button></div></>}
    </dialog>
  </section>;
}
