import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { api, errorMessage } from '../../api/client';
import type { BlameResult, CommitDetails, DiffRequest, DiffResult } from '../../api/types';
import { WorkingDiff } from './WorkingDiff';
import type { DiffLayout } from './Patch';

const PAGE = 200;
export function Blame({ request, displayPath, previousPath, layout, onLayoutChange, onClose }: {
  request: DiffRequest; displayPath: string; previousPath: string | null;
  layout: DiffLayout; onLayoutChange: (layout: DiffLayout) => void; onClose: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const returningToLines = useRef(false);
  const backToLines = useRef<HTMLButtonElement>(null);
  const [trigger] = useState(() => document.activeElement instanceof HTMLElement ? document.activeElement : null);
  const [side, setSide] = useState<'current' | 'previous'>('current');
  const [retry, setRetry] = useState(0);
  const [result, setResult] = useState<BlameResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [page, setPage] = useState(0);
  const [selection, setSelection] = useState<{ line: number; oid: string; parent: string | null } | null>(null);
  const [details, setDetails] = useState<CommitDetails | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [diffRequest, setDiffRequest] = useState<DiffRequest | null>(null);
  const [diff, setDiff] = useState<DiffResult | null>(null);
  const historical = request.context === 'commit';
  useLayoutEffect(() => {
    if (!selection && returningToLines.current) { returningToLines.current = false; list.current?.focus(); }
    if (selection && list.current?.getClientRects().length === 0) backToLines.current?.focus();
  }, [selection]);
  useEffect(() => {
    const node = dialog.current!; node.showModal();
    return () => { node.close(); if (trigger?.isConnected) trigger.focus(); };
  }, [trigger]);
  useEffect(() => {
    let ignore = false;
    setResult(null); setError(null); setPage(0); setSelection(null); setDetails(null); setDiffRequest(null);
    api.getBlame({ repositoryId: request.repositoryId, snapshotId: request.snapshotId, fileId: request.fileId, side })
      .then(value => { if (!ignore) setResult(value); }).catch(cause => { if (!ignore) setError(errorMessage(cause)); });
    return () => { ignore = true; };
  }, [request, side, retry]);
  const snapshotId = result?.snapshotId;
  useEffect(() => {
    let ignore = false;
    setDetails(null); setDetailError(null); setDiffRequest(null);
    if (selection && snapshotId) api.getBlameCommit(request.repositoryId, snapshotId, selection.oid, selection.parent)
      .then(value => {
        if (ignore) return;
        setDetails(value);
        const origin = result?.lines.find(line => line.line === selection.line)?.originalPath;
        const file = value.files.find(file => file.displayPath === origin || file.oldDisplayPath === origin) ?? value.files[0];
        if (file) setDiffRequest({ repositoryId: request.repositoryId, snapshotId: value.snapshotId, fileId: file.fileId, context: 'commit' });
      }).catch(cause => { if (!ignore) setDetailError(errorMessage(cause)); });
    return () => { ignore = true; };
  }, [request.repositoryId, snapshotId, selection, result]);
  useEffect(() => {
    let ignore = false; setDiff(null);
    if (diffRequest) api.getDiff(diffRequest).then(value => { if (!ignore) setDiff(value); })
      .catch(cause => { if (!ignore) setDiff({ kind: 'unavailable', reason: errorMessage(cause) }); });
    return () => { ignore = true; };
  }, [diffRequest]);
  const selectedLine = result?.lines.find(line => line.line === selection?.line);
  const pageLines = result?.lines.slice(page * PAGE, (page + 1) * PAGE) ?? [];
  const changePage = (value: number) => { setPage(value); list.current?.scrollTo?.(0, 0); list.current?.focus(); };
  return <dialog ref={dialog} className="file-history-dialog blame-dialog" aria-label="Blame" onCancel={event => { event.preventDefault(); onClose(); }}>
    <header className="file-history-heading"><div><h2>Blame</h2><p>{result?.displayPath ?? displayPath}</p></div><button autoFocus onClick={onClose}>Close blame</button></header>
    <div className="file-history-context">
      {(historical || previousPath) && <label>Version<select aria-label="Blame version" value={side} onChange={event => setSide(event.target.value as typeof side)}><option value="current">{historical ? 'Selected commit' : 'HEAD · current path'}</option><option value="previous">{historical ? 'Comparison parent · original path' : 'HEAD · previous path'}</option></select></label>}
      <p>{historical ? 'Committed file at the selected revision.' : 'HEAD content only — staged and unstaged edits are not included.'}</p>
      <details><summary>Revision: <code>{result?.revisionOid?.slice(0, 12) ?? (result ? 'No revision' : 'Loading…')}</code> · Attribution scope</summary><p><code>{result?.revisionOid}</code></p><p>Git attribution is not code ownership. Whole-file renames are followed by Git; extra copy/move detection and ignored-revision files are not used. Author identities come from the original commits, without mailmap. The branch filter does not limit this read.</p><p>Regular UTF-8 text only; 2 MiB, 10,000 lines and 30 seconds maximum. No partial attribution is shown.</p></details>
      {result?.shallow && <p role="status">Shallow history: boundary lines may have earlier origins that are not available locally.</p>}
      {error && <p role="alert">{error} <button onClick={() => setRetry(value => value + 1)}>Retry blame</button></p>}
      {!result && !error && <p role="status">Loading blame…</p>}
      {result?.unavailableReason && <p role="status">{result.unavailableReason}</p>}
    </div>
    <div className={`blame-panels${selection ? ' has-selection' : ''}`}>
      <section className="blame-source" aria-label="Line attribution">
        {result && !result.unavailableReason && <div className="blame-paging"><span role="status">{result.lines.length ? `Lines ${page * PAGE + 1}–${Math.min((page + 1) * PAGE, result.lines.length)} of ${result.lines.length}` : 'Empty committed file — no lines to attribute.'}</span><button disabled={page === 0} onClick={() => changePage(page - 1)}>Previous lines</button><button disabled={(page + 1) * PAGE >= result.lines.length} onClick={() => changePage(page + 1)}>Next lines</button></div>}
        <div className="blame-lines" ref={list} tabIndex={0} aria-label="Blame lines" onKeyDown={event => {
          if (!(event.target instanceof HTMLButtonElement) || !['ArrowUp', 'ArrowDown', 'Home', 'End'].includes(event.key)) return;
          const buttons = [...event.currentTarget.querySelectorAll<HTMLButtonElement>('button')];
          const current = buttons.indexOf(event.target); if (current < 0) return;
          event.preventDefault();
          const next = event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : Math.max(0, Math.min(buttons.length - 1, current + (event.key === 'ArrowDown' ? 1 : -1)));
          buttons[next].focus();
        }}>
          <table><caption className="sr-only">Select a line's attribution to inspect its commit. Arrow keys move between lines on this page; Enter opens the commit.</caption><thead><tr><th>Commit / author</th><th>Line</th><th>Committed content</th></tr></thead><tbody>{pageLines.map(line => <tr key={line.line} className={selection?.line === line.line ? 'blame-selected' : undefined}>
            <td><button aria-label={`Inspect line ${line.line}: ${line.author.name}, ${line.oid.slice(0, 12)}${line.boundary ? ', shallow boundary' : ''}`} aria-pressed={selection?.line === line.line} title={`${line.subject}\n${line.author.at}\n${line.originalPath}:${line.originalLine}`} onClick={() => setSelection({ line: line.line, oid: line.oid, parent: null })}><code>{line.oid.slice(0, 8)}{line.boundary ? ' · boundary' : ''}</code><span>{line.author.name}</span></button></td><td className="blame-line-number">{line.line}</td><td><pre>{line.content || ' '}</pre></td>
          </tr>)}</tbody></table>
        </div>
      </section>
      {selection && <section className="repository-details commit-inspector" aria-label="Blame commit">
        <div className="file-history-commit-heading"><button ref={backToLines} onClick={() => { returningToLines.current = true; setSelection(null); }}>Back to lines</button>
          {selectedLine && <p>Line {selectedLine.line} ← {selectedLine.originalPath}:{selectedLine.originalLine}{selectedLine.boundary ? ' · shallow boundary, earlier origin unknown' : ''}</p>}
          {detailError ? <p role="alert">{detailError} <button onClick={() => setSelection({ ...selection })}>Retry commit</button></p> : details ? <><h3>{details.message.split('\n')[0] || '(no subject)'}</h3><p>{details.author.name} · {details.author.at}</p><details><summary>Commit information</summary><code>{details.oid}</code><pre className="commit-message">{details.message}</pre></details>
            {details.parents.length > 1 && <label>Compare with parent<select aria-label="Blame comparison parent" value={details.selectedParent ?? ''} onChange={event => setSelection({ ...selection, parent: event.target.value })}>{details.parents.map(parent => <option key={parent} value={parent}>{parent}</option>)}</select></label>}
            {!details.files.length && <p>No changed files against this parent. Try another comparison parent for a merge.</p>}</> : <p>Loading commit…</p>}
        </div>
        {details && details.files.length > 0 && <label className="blame-change">Changed file<select aria-label="Blame changed file" value={diffRequest?.fileId ?? ''} onChange={event => setDiffRequest({ repositoryId: request.repositoryId, snapshotId: details.snapshotId, fileId: event.target.value, context: 'commit' })}>{details.files.map(file => <option key={file.fileId} value={file.fileId}>{file.changeKind} · {file.displayPath}</option>)}</select></label>}
        <div className="commit-diff">{diffRequest && <WorkingDiff key={`${diffRequest.snapshotId}:${diffRequest.fileId}`} request={diffRequest} displayPath={details?.files.find(file => file.fileId === diffRequest.fileId)?.displayPath ?? ''} diff={diff} layout={layout} onLayoutChange={onLayoutChange} active={false} blockedReason={null} onAction={() => { /* Read-only commit inspection. */ }} />}</div>
      </section>}
    </div>
  </dialog>;
}
