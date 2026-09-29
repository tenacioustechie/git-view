import { useEffect, useMemo, useRef, useState } from 'react';
import { api, errorMessage } from '../../api/client';
import type { CommitDetails, DiffRequest, DiffResult, FileHistoryPage, FileHistoryRequest } from '../../api/types';
import { HistoryGraph } from '../../graph/HistoryGraph';
import { WorkingDiff } from './WorkingDiff';
import type { DiffLayout } from './Patch';

export function FileHistory({ request, displayPath, previousPath, layout, onLayoutChange, onClose }: {
  request: FileHistoryRequest; displayPath: string; previousPath: string | null;
  layout: DiffLayout; onLayoutChange: (layout: DiffLayout) => void; onClose: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [trigger] = useState(() => document.activeElement instanceof HTMLElement ? document.activeElement : null);
  const [path, setPath] = useState<'current' | 'previous'>('current');
  const [retry, setRetry] = useState(0);
  const [page, setPage] = useState<FileHistoryPage | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selection, setSelection] = useState<{ oid: string; parent: string | null } | null>(null);
  const [details, setDetails] = useState<CommitDetails | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [diffRequest, setDiffRequest] = useState<DiffRequest | null>(null);
  const [diff, setDiff] = useState<DiffResult | null>(null);
  const epoch = useRef(0);
  const paging = useRef(false);
  const source = useMemo(() => ({ ...request, source: { ...request.source, path } }), [request, path]);
  useEffect(() => {
    const node = dialog.current!;
    node.showModal();
    return () => { node.close(); if (trigger?.isConnected) trigger.focus(); };
  }, [trigger]);
  useEffect(() => {
    const current = ++epoch.current;
    setPage(null); setLoading(true); setError(null); setSelection(null); setDetails(null); setDiffRequest(null);
    paging.current = false;
    api.getFileHistory(source).then(result => { if (epoch.current === current) setPage(result); })
      .catch(cause => { if (epoch.current === current) setError(errorMessage(cause)); })
      .finally(() => { if (epoch.current === current) setLoading(false); });
    return () => { epoch.current++; };
  }, [source, retry]);
  const snapshotId = page?.history.snapshotId;
  useEffect(() => {
    let ignore = false;
    setDetails(null); setDetailError(null); setDiffRequest(null);
    if (snapshotId && selection) api.getFileHistoryCommit(request.repositoryId, snapshotId, selection.oid, selection.parent)
      .then(result => {
        if (ignore) return;
        setDetails(result);
        if (result.files[0]) setDiffRequest({ repositoryId: request.repositoryId, snapshotId: result.snapshotId, fileId: result.files[0].fileId, context: 'commit' });
      }).catch(cause => { if (!ignore) setDetailError(errorMessage(cause)); });
    return () => { ignore = true; };
  }, [request.repositoryId, snapshotId, selection]);
  useEffect(() => {
    let ignore = false;
    setDiff(null);
    if (diffRequest) api.getDiff(diffRequest).then(result => { if (!ignore) setDiff(result); })
      .catch(cause => { if (!ignore) setDiff({ kind: 'unavailable', reason: errorMessage(cause) }); });
    return () => { ignore = true; };
  }, [diffRequest]);
  const more = async () => {
    if (!page?.history.nextCursor || loading || paging.current) return;
    paging.current = true; setLoading(true); setError(null);
    const current = epoch.current;
    try {
      const next = await api.getFileHistory({ ...source, historySnapshotId: page.history.snapshotId, cursor: page.history.nextCursor });
      if (epoch.current !== current) return;
      if (next.history.snapshotId !== page.history.snapshotId) throw new Error('File history changed. Reopen it from the file.');
      setPage({ ...next, history: { ...next.history, commits: [...page.history.commits, ...next.history.commits] } });
    } catch (cause) { if (epoch.current === current) setError(errorMessage(cause)); }
    finally { if (epoch.current === current) { paging.current = false; setLoading(false); } }
  };
  const rows = useMemo(() => (page?.history.commits ?? []).map(commit => ({ commit, lane: 0, width: 0, segments: [], pending: [] })), [page]);
  const currentPath = page?.displayPath ?? (path === 'previous' ? previousPath : displayPath) ?? displayPath;
  return <dialog ref={dialog} className="file-history-dialog" aria-label="File history" onCancel={event => { event.preventDefault(); onClose(); }}>
    <header className="file-history-heading"><div><h2>File history</h2><p title={currentPath}>{currentPath}</p></div><button autoFocus onClick={onClose}>Close file history</button></header>
    <div className="file-history-context">
      {previousPath && <label>Path history<select aria-label="Path history" value={path} onChange={event => { epoch.current++; setSelection(null); setDetails(null); setDiffRequest(null); setPath(event.target.value as typeof path); }}><option value="current">Current path · {displayPath}</option><option value="previous">Previous path · {previousPath}</option></select></label>}
      <p>Literal path · committed changes only · renames are not followed automatically.</p>
      {page && <details><summary>Starting revision: <code>{page.startOid?.slice(0, 12) ?? 'Unborn HEAD — no commits yet'}</code> · Scope and limits</summary><p><code>{page.startOid}</code> — only ancestors of this revision are included; the repository branch filter is not used. If the path was formerly a directory, commits affecting its descendants can appear; previews show only exact-path changes.</p></details>}
      {page?.shallow && <p role="status">Shallow repository: earlier history may be unavailable.</p>}
      {error && <p role="alert">{error} <button onClick={() => setRetry(value => value + 1)}>Retry file history</button></p>}
      <p role="status">{loading ? 'Loading file history…' : page ? `${page.history.commits.length} commits loaded` : 'File history unavailable.'}</p>
    </div>
    <div className="file-history-panels">
      <section className="file-history-list" aria-label="File history commits"><HistoryGraph key={`${path}:${retry}`} rows={rows} selected={selection?.oid ?? null} onSelect={oid => setSelection({ oid, parent: null })} loading={loading} more={Boolean(page?.history.nextCursor)} onMore={() => { void more(); }} searchResults failed={Boolean(error) && !page} label="File history results" emptyMessage="No committed changes for this path at the starting revision." /></section>
      <section className="repository-details commit-inspector" aria-label="File history commit">
        {detailError ? <p role="alert">{detailError}</p> : details ? <>
          <div className="file-history-commit-heading"><h3>{details.message.split('\n')[0] || '(no subject)'}</h3><p>{details.author.name} · {details.author.at}</p><details><summary>Commit information</summary><code>{details.oid}</code><pre className="commit-message">{details.message}</pre></details>
          {details.parents.length > 1 && <label>Compare with parent<select aria-label="File history comparison parent" value={details.selectedParent ?? ''} onChange={event => setSelection({ oid: details.oid, parent: event.target.value })}>{details.parents.map(parent => <option key={parent} value={parent}>{parent}</option>)}</select></label>}
          {!details.files.length && <p>No exact-path change against this parent. For a merge, try another parent. If this path was a directory, its descendant changes are not shown here.</p>}</div>
          {details.files.length > 1 && <label>Change<select aria-label="File history change" value={diffRequest?.fileId ?? ''} onChange={event => setDiffRequest({ repositoryId: request.repositoryId, snapshotId: details.snapshotId, fileId: event.target.value, context: 'commit' })}>{details.files.map(file => <option key={file.fileId} value={file.fileId}>{file.changeKind} · {file.displayPath}</option>)}</select></label>}
          <div className="commit-diff">{diffRequest && <WorkingDiff key={`${diffRequest.snapshotId}:${diffRequest.fileId}`} request={diffRequest} displayPath={details.files.find(file => file.fileId === diffRequest.fileId)?.displayPath ?? currentPath} diff={diff} layout={layout} onLayoutChange={onLayoutChange} active={false} blockedReason={null} onAction={() => { /* Historical inspection has no mutation controls. */ }} />}</div>
        </> : <p>{selection ? 'Loading commit…' : 'Select a commit to inspect this path’s changes. No graph connections are inferred between filtered results.'}</p>}
      </section>
    </div>
  </dialog>;
}
