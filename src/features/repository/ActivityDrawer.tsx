import { useEffect, useId, useRef, useState } from 'react';
import type { OperationResult } from '../../api/types';
import { shortcutsBlocked } from '../../app/shortcuts';
import { Icon } from '../../app/Icon';
import './activity.css';

const active = (operation: OperationResult) => ['queued', 'running'].includes(operation.state);
const attention = (operation: OperationResult) => operation.outcomeUncertain || Boolean(operation.error)
  || ['failed', 'stopped', 'skipped', 'canceled'].includes(operation.state);
const label = (action: string) => action.replace(/([a-z])([A-Z])/g, '$1 $2').toLowerCase();
function timestamp(value: string | null) {
  if (!value) return 'Not recorded';
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

// Read-only presentation of the repository's existing backend operation snapshot.
export function ActivityDrawer({ operations }: { operations: OperationResult[] }) {
  const [open, setOpen] = useState(false);
  const [filter, setFilter] = useState('all');
  const trigger = useRef<HTMLButtonElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const id = useId();
  const latest = operations.at(-1);
  const running = operations.filter(active).length;
  const needingAttention = operations.filter(attention).length;
  const matching = operations.filter(operation => filter === 'all' || attention(operation));
  // Bound the rendered history, without losing older queued/running operations.
  const recentIds = new Set(matching.slice(-30).map(operation => operation.operationId));
  const shown = matching.filter(operation => recentIds.has(operation.operationId) || active(operation)).reverse();
  const close = () => { setOpen(false); trigger.current?.focus(); };
  useEffect(() => { if (open) closeButton.current?.focus(); }, [open]);

  return <footer className={`repository-activity ${open ? 'activity-open' : ''}`}>
    <button className="activity-toggle" ref={trigger} aria-label="Repository activity" aria-expanded={open} aria-controls={id}
      onClick={() => setOpen(value => !value)}>
      <Icon name="chevron" /><strong>Activity</strong>
      <span className="activity-latest" role="status">{latest ? `${label(latest.action)}: ${latest.state}` : 'No operations this session'}</span>
      {running > 0 && <span className="activity-count">{running} active</span>}
      {needingAttention > 0 && <span className="activity-count">{needingAttention} need attention</span>}
    </button>
    <section id={id} hidden={!open} className="activity-drawer" aria-label="Repository activity details" onKeyDown={event => {
      if (event.key !== 'Escape' || event.defaultPrevented || event.nativeEvent.isComposing || shortcutsBlocked()) return;
      event.preventDefault(); event.stopPropagation(); close();
    }}>
      <header className="activity-heading"><div><h2>Repository activity</h2><small>This app session · newest first · read-only</small></div>
        <label>Show <select aria-label="Activity filter" value={filter} onChange={event => setFilter(event.target.value)}>
          <option value="all">All</option><option value="attention">Needs attention</option>
        </select></label><button ref={closeButton} onClick={close}>Close activity</button>
      </header>
      <div className="activity-scroll" tabIndex={0} aria-label="Operation results">
        {shown.length === 0 ? <p className="activity-empty">{filter === 'all' ? 'No operations recorded for this repository in this app session.' : 'No operations need attention.'}</p> : <>
          <p className="activity-scope">Showing {shown.length} of {matching.length} matching operations{shown.length < matching.length ? ' · latest 30 plus any older active operations' : ''}.</p>
          <ol>{shown.map(operation => <li key={operation.operationId}>
            <div className="activity-result-heading"><strong>{label(operation.action)}</strong><span className={`activity-state state-${operation.state}`}>{operation.state}</span>
              <time dateTime={operation.finishedAt ?? operation.startedAt ?? undefined}>{timestamp(operation.finishedAt ?? operation.startedAt)}</time></div>
            {operation.reason && <p>{operation.reason}</p>}
            {operation.error && <p className="activity-error">{operation.error.message}</p>}
            {operation.outcomeUncertain && <p className="activity-warning">Outcome uncertain. Refresh and inspect the repository before retrying; cancellation does not undo completed work.</p>}
            {(operation.outputTruncated || operation.error?.outputTruncated) && <p className="activity-warning">Operation output truncated. Details may be incomplete.</p>}
            <details><summary>Operation details</summary><dl>
              <dt>Operation ID</dt><dd>{operation.operationId}</dd>
              <dt>Started</dt><dd>{timestamp(operation.startedAt)}</dd><dt>Finished</dt><dd>{timestamp(operation.finishedAt)}</dd>
              <dt>Exit code</dt><dd>{operation.exitCode ?? operation.error?.exitCode ?? 'Not recorded'}</dd>
              {operation.batchId && <><dt>Batch ID</dt><dd>{operation.batchId}</dd></>}
              {operation.error && <><dt>Error code</dt><dd>{operation.error.code}</dd></>}
            </dl>{operation.error?.details ? <pre aria-label="Diagnostic output">{operation.error.details}</pre> : <p>No diagnostic output recorded.</p>}</details>
          </li>)}</ol>
        </>}
      </div>
    </section>
  </footer>;
}
