import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { open, save } from '@tauri-apps/plugin-dialog';
import { api, errorMessage } from '../../api/client';
import { withNavigationBlock } from '../../api/navigation';
import type { BatchPlan, DiscoveryCandidate, OperationResult, Project, ProjectContent, RepositoryOpen } from '../../api/types';
import { DiscoveryPicker } from './DiscoveryView';
import { branchName, matchesFilters, workingState, type GroupFilters } from './groupState';
import './groups.css';

interface Props { project: Project; onOpenRepository: (repository: RepositoryOpen) => void; onProjectChange?: (project: Project) => void }
const terminal = (operation: OperationResult) => !['queued', 'running'].includes(operation.state);
const identity = (project: Project) => JSON.stringify([project.projectId, project.revision, project.stale]);

export function GroupView({ project: initial, onOpenRepository, onProjectChange }: Props) {
  const [project, setProject] = useState(initial);
  const [filters, setFilters] = useState<GroupFilters>({ search: '', branch: '', state: 'all' });
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [preview, setPreview] = useState<BatchPlan | null>(null);
  const [previewScope, setPreviewScope] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [operations, setOperations] = useState<OperationResult[]>([]);
  const [draft, setDraft] = useState<ProjectContent | null>(null);
  const [editRevision, setEditRevision] = useState('');
  const [saveConflict, setSaveConflict] = useState(false);
  const [discovering, setDiscovering] = useState(false);
  const discoveredIds = useRef(new Map<string, string>());
  const trackedBatches = useRef(new Set<string>());
  const knownMembers = useRef(new Map(initial.members.map(row => [row.memberId, row.label])));
  const scanInFlight = useRef(false);
  const activityInFlight = useRef(false);
  const root = useRef<HTMLElement>(null);
  const generation = useRef(0);
  const currentIdentity = useRef(identity(initial));
  const editPaths = useRef(new Map<string, string>());
  const projectChange = useRef(onProjectChange);
  projectChange.current = onProjectChange;

  const acceptProject = useCallback((next: Project) => {
    if (currentIdentity.current !== identity(next)) {
      currentIdentity.current = identity(next);
      generation.current++;
    }
    setProject(next);
    projectChange.current?.(next);
  }, []);
  useEffect(() => {
    if (currentIdentity.current !== identity(initial)) {
      currentIdentity.current = identity(initial);
      generation.current++;
    }
    setProject(initial);
  }, [initial]);
  useEffect(() => {
    for (const row of project.members) knownMembers.current.set(row.memberId, row.label);
    setPreview(null);
  }, [project.revision, project.stale]);

  const refresh = useCallback(async (manual = false) => {
    if (scanInFlight.current) return;
    if (!manual && (document.visibilityState === 'hidden' || root.current?.closest('[hidden]'))) return;
    scanInFlight.current = true;
    const requestedGeneration = generation.current;
    try {
      const next = await api.refreshProject(project.projectId);
      if (generation.current === requestedGeneration) acceptProject(next);
    }
    catch (cause) { setError(errorMessage(cause)); }
    finally { scanInFlight.current = false; }
  }, [acceptProject, project.projectId]);

  const loadOperations = useCallback(async () => {
    if (activityInFlight.current) return;
    activityInFlight.current = true;
    try {
      const results = await api.getOperations();
      setOperations(results.filter(operation => operation.batchId && (
        trackedBatches.current.has(operation.batchId) || (operation.memberId !== null && knownMembers.current.has(operation.memberId))
      )));
    } catch (cause) { setError(`Activity refresh failed: ${errorMessage(cause)}`); }
    finally { activityInFlight.current = false; }
  }, []);

  useEffect(() => {
    void loadOperations();
    const activity = window.setInterval(() => { void loadOperations(); }, 1500);
    const status = window.setInterval(() => { void refresh(); }, 10000);
    const focus = () => { void refresh(); void loadOperations(); };
    window.addEventListener('focus', focus);
    return () => { window.clearInterval(activity); window.clearInterval(status); window.removeEventListener('focus', focus); };
  }, [loadOperations, refresh]);
  const completedCount = operations.filter(terminal).length;
  useEffect(() => { if (completedCount) void refresh(); }, [completedCount, refresh]);

  const rows = project.members.filter(row => matchesFilters(row, filters));
  const memberIds = new Set(project.members.map(row => row.memberId));
  const selectedIds = [...selected].filter(id => memberIds.has(id));
  const visibleIds = new Set(rows.map(row => row.memberId));
  const hiddenCount = selectedIds.filter(id => !visibleIds.has(id)).length;
  const branches = [...new Set(project.members.flatMap(row => row.kind === 'ready' && row.status.head.kind !== 'detached' ? [row.status.head.name] : []))].sort();
  if (filters.branch && !branches.includes(filters.branch)) branches.push(filters.branch);
  const batches = useMemo(() => {
    const grouped = new Map<string, OperationResult[]>();
    for (const operation of operations) {
      if (operation.batchId) grouped.set(operation.batchId, [...(grouped.get(operation.batchId) ?? []), operation]);
    }
    return [...grouped.entries()].reverse();
  }, [operations]);
  const latest = new Map(operations.filter(op => op.memberId).map(op => [op.memberId, op]));

  async function perform(task: () => Promise<void>) {
    if (busy) return;
    setBusy(true); setError(null);
    try { await task(); } catch (cause) { setError(errorMessage(cause)); }
    finally { setBusy(false); }
  }
  async function plan(action: 'fetch' | 'pull', ids: string[], scope: string, branchFilter: string | null) {
    await perform(async () => {
      const requestedGeneration = generation.current;
      const result = await api.planBatch({ projectId: project.projectId, revision: project.revision, memberIds: ids, action, branchFilter });
      if (requestedGeneration !== generation.current) return;
      setPreview(result); setPreviewScope(scope);
    });
  }
  function toggle(id: string) {
    setSelected(current => { const next = new Set(current); if (next.has(id)) next.delete(id); else next.add(id); return next; });
  }
  function startEdit() {
    captureEditPaths();
    setDraft(structuredClone(project.content)); setEditRevision(project.revision); setSaveConflict(false);
  }
  function captureEditPaths() {
    editPaths.current = new Map(project.content.repositories.map((entry, index) => [entry.path, project.members[index]?.displayPath ?? entry.path]));
  }
  function addDiscovered(candidates: DiscoveryCandidate[]) {
    if (!draft) { captureEditPaths(); setEditRevision(project.revision); setSaveConflict(false); }
    for (const candidate of candidates) discoveredIds.current.set(candidate.displayPath, candidate.repositoryId);
    setDraft(current => {
      const next = structuredClone(current ?? project.content);
      const paths = new Set(next.repositories.map(entry => editPaths.current.get(entry.path) ?? entry.path));
      for (const candidate of candidates) if (!paths.has(candidate.displayPath)) { next.repositories.push({ path: candidate.displayPath, label: candidate.label }); paths.add(candidate.displayPath); }
      return next;
    });
    setDiscovering(false);
  }
  async function pickFolders(index?: number) {
    await perform(async () => {
      const result = await withNavigationBlock(() => open({ directory: true, multiple: index === undefined, title: index === undefined ? 'Add repositories' : 'Locate repository' }));
      if (!result) return;
      const paths = Array.isArray(result) ? result : [result];
      setDraft(current => {
        const next = structuredClone(current ?? project.content);
        if (index === undefined) next.repositories.push(...paths.map(path => ({ path })));
        else next.repositories[index].path = paths[0];
        return next;
      });
      if (!draft) { captureEditPaths(); setEditRevision(project.revision); setSaveConflict(false); }
    });
  }
  async function saveDraft(saveAs: boolean) {
    if (!draft) return;
    await perform(async () => {
      generation.current++;
      try {
        let next: Project;
        if (saveAs) {
          const path = await withNavigationBlock(() => save({ title: 'Save project as', defaultPath: `${draft.name || 'project'}.git-view.json`, filters: [{ name: 'Git View project', extensions: ['json'] }] }));
          if (!path) return;
          // Relative entries must continue pointing at the same repository after moving the project file.
          const content = { ...draft, repositories: draft.repositories.map(entry => {
            return { ...entry, path: editPaths.current.get(entry.path) ?? entry.path };
          }) };
          next = await api.createProject(path, content);
        } else next = await api.saveProject(project.projectId, editRevision, draft);
        acceptProject(next); setDraft(null); setSaveConflict(false);
      } catch (cause) {
        if (!saveAs) setSaveConflict(true);
        throw cause;
      }
    });
  }

  return <section className="group-view" ref={root} aria-label="Repository group">
    <header className="group-heading"><div><h1>{project.name}</h1><p className="group-path" title={project.displayPath}>{project.displayPath}</p></div>
      <button onClick={startEdit} disabled={busy}>Edit project</button><button onClick={() => void pickFolders()} disabled={busy}>Add repositories</button>
      <button onClick={() => setDiscovering(true)} disabled={busy}>Discover repositories</button>
      <button onClick={() => void refresh(true)} disabled={busy}>Refresh</button>
    </header>
    {discovering && <DiscoveryPicker onConfirm={addDiscovered} onCancel={() => setDiscovering(false)} confirmLabel="Add selected to draft" disabled={busy}
      excludedRepositoryIds={new Set([...project.members.flatMap(row => row.kind === 'ready' ? [row.repositoryId] : []), ...(draft?.repositories.flatMap(entry => discoveredIds.current.has(entry.path) ? [discoveredIds.current.get(entry.path)!] : []) ?? [])])}
      excludedPaths={new Set((draft ?? project.content).repositories.map(entry => editPaths.current.get(entry.path) ?? entry.path))} />}
    {error && <div role="alert" className="group-notice">{error}<button aria-label="Dismiss error" onClick={() => setError(null)}>Dismiss</button></div>}
    {(project.stale || saveConflict) && <div role="alert" className="group-notice">
      <p>{project.error?.message ?? 'The project may have changed or cannot be saved. Your current dashboard and edits are retained.'} New batches are blocked until the project is reloaded.</p>
      <button disabled={busy} onClick={() => void perform(async () => { generation.current++; acceptProject(await api.openProject(project.displayPath)); setDraft(null); setSaveConflict(false); })}>Reload project</button>
      {draft && <button disabled={busy} onClick={() => void saveDraft(true)}>Save edits as</button>}
    </div>}
    {draft && <section className="group-editor" aria-label="Edit project">
      <h2>Edit project</h2><label>Project name <input value={draft.name} onChange={event => setDraft({ ...draft, name: event.target.value })} /></label>
      <p>Removing an entry only changes this project file. Repository files remain on disk.</p>
      {draft.repositories.map((entry, index) => <div className="group-edit-row" key={index}>
        <label>Label {index + 1}<input value={entry.label ?? ''} onChange={event => setDraft({ ...draft, repositories: draft.repositories.map((row, i) => i === index ? { path: row.path, ...(event.target.value.trim() ? { label: event.target.value } : {}) } : row) })} /></label>
        <span className="group-path" title={entry.path}>{entry.path}</span>
        <button disabled={busy} onClick={() => void pickFolders(index)}>Locate {index + 1}</button>
        <button disabled={busy} onClick={() => setDraft({ ...draft, repositories: draft.repositories.filter((_, i) => i !== index) })}>Remove {index + 1}</button>
      </div>)}
      <div className="group-toolbar"><button disabled={busy || !draft.name.trim() || saveConflict || project.stale} onClick={() => void saveDraft(false)}>Save project</button>
        <button disabled={busy || !draft.name.trim()} onClick={() => void saveDraft(true)}>Save as</button><button onClick={() => setDraft(null)} disabled={busy}>Cancel editing</button></div>
    </section>}
    <div className="group-filters">
      <label>Search <input type="search" value={filters.search} placeholder="Name or path" onChange={event => setFilters({ ...filters, search: event.target.value })} /></label>
      <label>Branch <select value={filters.branch} onChange={event => setFilters({ ...filters, branch: event.target.value })}><option value="">All branches</option>{branches.map(branch => <option key={branch}>{branch}</option>)}</select></label>
      <label>Working state <select value={filters.state} onChange={event => setFilters({ ...filters, state: event.target.value as GroupFilters['state'] })}><option value="all">All states</option><option value="dirty">Dirty</option><option value="behind">Behind upstream</option><option value="attention">Attention needed</option></select></label>
      <button onClick={() => setFilters({ search: '', branch: '', state: 'all' })}>Clear filters</button>
    </div>
    <div className="group-toolbar">
      <button className="group-primary" disabled={busy || project.stale || saveConflict || !rows.length} onClick={() => void plan('pull', rows.map(row => row.memberId), `All ${rows.length} matching members`, filters.branch || null)}>Pull all ({rows.length})</button>
      <button disabled={busy || project.stale || saveConflict || !rows.length} onClick={() => void plan('fetch', rows.map(row => row.memberId), `All ${rows.length} matching members`, filters.branch || null)}>Fetch all ({rows.length})</button>
      <button disabled={busy || project.stale || saveConflict || !selectedIds.length} onClick={() => void plan('pull', selectedIds, `${selectedIds.length} selected members, including ${hiddenCount} hidden by filters`, null)}>Pull selected ({selectedIds.length})</button>
      <button disabled={busy || project.stale || saveConflict || !selectedIds.length} onClick={() => void plan('fetch', selectedIds, `${selectedIds.length} selected members, including ${hiddenCount} hidden by filters`, null)}>Fetch selected ({selectedIds.length})</button>
      <span>{selectedIds.length} selected{hiddenCount > 0 ? ` · ${hiddenCount} hidden by filters` : ''}</span>
    </div>
    <p className="group-hint">Pull all uses the matching rows below. Pull selected includes checked rows hidden by filters. Pull is fast-forward only; dirty repositories are skipped. Ahead/behind counts reflect the last known upstream state.</p>
    {preview && <section className="group-preview" aria-label="Batch preview">
      <h2>{preview.action === 'pull' ? 'Pull' : 'Fetch'} preview</h2><p>{previewScope}. This candidate list is fixed; changing filters does not change it.</p>
      <p>Plan expires {new Date(preview.expiresAt).toLocaleTimeString()}. Each repository is rechecked before it runs.</p>
      <ul>{preview.candidates.map(candidate => <li key={candidate.memberId}><strong>{candidate.label}</strong> — {candidate.eligible ? 'Eligible' : 'Skipped'}{candidate.destination ? ` → ${candidate.destination}` : ''}{candidate.reason ? `: ${candidate.reason}` : ''}</li>)}</ul>
      <button disabled={busy || project.stale || saveConflict || !preview.candidates.some(candidate => candidate.eligible)} onClick={() => void perform(async () => { const batch = await api.runBatch(preview.planId); trackedBatches.current.add(batch.batchId); setPreview(null); await loadOperations(); })}>Run {preview.candidates.filter(candidate => candidate.eligible).length} eligible</button>
      <button disabled={busy} onClick={() => setPreview(null)}>Close preview</button>
    </section>}
    <div className="group-table-scroll"><table className="group-table"><caption>{rows.length} of {project.members.length} repositories</caption>
      <thead><tr><th><input type="checkbox" aria-label="Select all matching" checked={rows.length > 0 && rows.every(row => selected.has(row.memberId))} onChange={event => { const checked = event.target.checked; setSelected(current => { const next = new Set(current); for (const row of rows) { if (checked) next.add(row.memberId); else next.delete(row.memberId); } return next; }); }} /></th><th>Name / path</th><th>Branch</th><th>Working state</th><th>Upstream / freshness</th><th>Last result</th></tr></thead>
      <tbody>{rows.map(row => <tr key={row.memberId}>
        <td><input type="checkbox" aria-label={`Select ${row.label}`} checked={selected.has(row.memberId)} onChange={() => toggle(row.memberId)} /></td>
        <td>{row.kind === 'ready' ? <button className="group-link" onClick={() => onOpenRepository({ repositoryId: row.repositoryId, displayPath: row.displayPath, status: row.status })}>{row.label}</button> : <strong>{row.label}</strong>}<span className="group-path" title={row.displayPath}>{row.displayPath}</span>{row.kind === 'unavailable' && <button disabled={busy} onClick={() => { startEdit(); void pickFolders(project.members.findIndex(member => member.memberId === row.memberId)); }}>Locate</button>}</td>
        <td title={branchName(row)}>{branchName(row)}{row.kind === 'ready' && row.status.head.kind === 'unborn' ? ' (unborn)' : ''}</td><td>{workingState(row)}</td>
        <td>{row.kind === 'ready' ? <>{row.status.upstream ? <span title={row.status.upstream.ref}>Ahead {row.status.upstream.ahead ?? '?'} · Behind {row.status.upstream.behind ?? '?'}</span> : 'No upstream'}<small>Observed {new Date(row.status.observedAt).toLocaleTimeString()}</small><small>{row.status.lastSuccessfulFetchAt ? `Fetched ${new Date(row.status.lastSuccessfulFetchAt).toLocaleString()}` : 'Fetch time unknown'}</small></> : 'Unknown'}</td>
        <td>{latest.get(row.memberId)?.state ?? 'No activity'}{latest.get(row.memberId)?.reason && <small>{latest.get(row.memberId)?.reason}</small>}</td>
      </tr>)}</tbody></table></div>
    {!rows.length && <p>No repositories match these filters.</p>}
    <section aria-label="Batch activity"><h2>Batch activity</h2><p>Results stay visible independently of filters. Cancel stops queued work and requests cancellation of running jobs; it does not undo completed changes.</p>
      {!batches.length && <p>No batch activity yet.</p>}
      {batches.map(([id, results]) => <BatchActivity key={id} id={id} results={results} labels={knownMembers.current} busy={busy} blocked={project.stale || saveConflict} onCancel={() => void perform(async () => { await api.cancelBatch(id); await loadOperations(); await refresh(true); })} onRetry={(action, ids) => void plan(action, ids, `Retry ${ids.length} chosen members`, null)} />)}
    </section>
  </section>;
}

function BatchActivity({ id, results, labels, busy, blocked, onCancel, onRetry }: { id: string; results: OperationResult[]; labels: Map<string, string>; busy: boolean; blocked: boolean; onCancel: () => void; onRetry: (action: 'fetch' | 'pull', ids: string[]) => void }) {
  const [retryIds, setRetryIds] = useState<Set<string>>(new Set());
  const retryable = results.filter(result => ['failed', 'skipped', 'canceled'].includes(result.state) && result.memberId);
  const ids = [...new Set(retryable.flatMap(result => result.memberId && retryIds.has(result.memberId) ? [result.memberId] : []))];
  const action = results[0]?.action === 'fetch' ? 'fetch' : 'pull';
  return <article className="group-activity"><h3>{action === 'pull' ? 'Pull' : 'Fetch'} batch <small>{id}</small></h3>
    <table className="group-table"><thead><tr><th>Retry</th><th>Repository</th><th>State</th><th>Details</th></tr></thead><tbody>{results.map(result => <tr key={result.operationId}>
      <td>{retryable.includes(result) && result.memberId && <input type="checkbox" aria-label={`Retry ${labels.get(result.memberId) ?? result.memberId}`} checked={retryIds.has(result.memberId)} onChange={event => { const checked = event.target.checked; setRetryIds(current => { const next = new Set(current); if (checked) next.add(result.memberId!); else next.delete(result.memberId!); return next; }); }} />}</td>
      <td>{labels.get(result.memberId ?? '') ?? result.memberId ?? result.repositoryId ?? 'Repository'}</td><td>{result.state}</td><td>{result.error?.message ?? result.reason ?? '—'}{result.outcomeUncertain && <strong> Outcome uncertain; refresh and inspect the repository before retrying.</strong>}{result.outputTruncated && <span> Output was truncated.</span>}</td>
    </tr>)}</tbody></table>
    {results.some(result => !terminal(result)) && <button disabled={busy} onClick={onCancel}>Cancel batch</button>}
    {retryable.length > 0 && <button disabled={busy || blocked || !ids.length || results.some(result => !terminal(result))} onClick={() => onRetry(action, ids)}>Preview retry ({ids.length})</button>}
  </article>;
}
