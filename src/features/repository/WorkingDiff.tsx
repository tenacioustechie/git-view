import { useMemo, useRef } from 'react';
import type { DiffRequest, DiffResult, RepositoryAction } from '../../api/types';
import { DiffLayoutControl, limitPatch, Patch, type DiffLayout } from './Patch';

export function WorkingDiff({ request, displayPath, diff, active, blockedReason, onAction, layout = 'unified', onLayoutChange, onFileHistory, onBlame }: {
  request: DiffRequest; displayPath: string; diff: DiffResult | null; active: boolean;
  blockedReason: string | null; onAction: (action: RepositoryAction) => void;
  layout?: DiffLayout; onLayoutChange?: (layout: DiffLayout) => void;
  onFileHistory?: () => void;
  onBlame?: () => void;
}) {
  const root = useRef<HTMLElement>(null);
  const lastHunk = useRef<HTMLElement | null>(null);
  const working = request.context !== 'commit';
  const label = request.context === 'staged' ? 'Unstage hunk' : 'Stage hunk';
  const actions = diff?.kind === 'text' ? diff.hunkActions : undefined;
  const displayLimited = useMemo(() => diff?.kind === 'text' && (limitPatch(diff.text).limited || (diff.hunkActions?.kind === 'available'
    && limitPatch(diff.hunkActions.preamble + diff.hunkActions.hunks.map(hunk => hunk.text).join('')).limited)), [diff]);
  const available = working && diff?.kind === 'text' && !diff.truncated && !displayLimited && actions?.kind === 'available' && actions.context === request.context ? actions : null;
  const reason = !working || !diff ? null : blockedReason ?? (diff.kind === 'binary' ? 'Binary changes cannot be staged or unstaged by hunk.'
    : diff.kind === 'unavailable' ? diff.reason
      : diff.truncated ? 'Hunk actions require a complete diff. This preview is truncated.'
        : displayLimited ? 'Hunk actions are unavailable because this display exceeds 10,000 patch lines. Use whole-file staging or Git after reviewing the full change.'
        : actions?.kind === 'unavailable' ? actions.reason
          : !available ? 'Hunk actions are unavailable for this diff. Refresh and inspect the file again.' : null);
  const hasHunks = diff?.kind === 'text' && (available ? available.hunks.length > 0 : /^@@ -\d+(?:,\d+)? \+\d+(?:,\d+)? @@/m.test(diff.text));
  const jump = (direction: number) => {
    const hunks = [...(root.current?.querySelectorAll<HTMLElement>('[data-diff-hunk]') ?? [])];
    if (!hunks.length) return;
    const current = hunks.indexOf(lastHunk.current!);
    const index = current < 0 ? (direction > 0 ? 0 : hunks.length - 1) : (current + direction + hunks.length) % hunks.length;
    lastHunk.current = hunks[index]; hunks[index].focus({ preventScroll: true }); hunks[index].scrollIntoView?.({ block: 'nearest', inline: 'nearest' });
  };
  return <section ref={root} className="diff-panel" aria-label="File diff">
    <header className="diff-heading"><h3>Diff · {request.context}</h3><p className="diff-file-path" title={displayPath}>{displayPath}</p>
      {onFileHistory && <button className="file-history-open" onClick={onFileHistory}>File history</button>}
      {onBlame && <button onClick={onBlame}>Blame</button>}
      {diff?.kind === 'text' && <div className="diff-view-tools">
        {onLayoutChange && <DiffLayoutControl value={layout} onChange={onLayoutChange} />}
        {hasHunks && <div role="group" aria-label="Hunk navigation"><button onClick={() => jump(-1)}>Previous hunk</button><button onClick={() => jump(1)}>Next hunk</button></div>}
      </div>}
    </header>
    <div className="diff-body">
    {working && <p className="diff-guidance">{request.context === 'staged' ? 'Staged changes will be included in your next commit.' : 'Unstaged changes are not included in your next commit.'} Hunk actions keep your working files unchanged.</p>}
    {!diff ? <p>Loading diff…</p> : <>
      {reason && <p role="status">{reason}</p>}
      {diff.kind === 'text' ? <>
        {diff.truncated && <p role="status">Preview truncated: this is not the full patch.</p>}
        {available ? <>
          {available.preamble && <Patch text={available.preamble} layout={layout} />}
          {available.hunks.map((hunk, index) => <section className="diff-hunk-block" aria-label={`Hunk ${index + 1}`} key={hunk.hunkId}>
            <div className="diff-hunk-toolbar"><strong>Hunk {index + 1} · {hunk.header}</strong><button disabled={active || Boolean(reason)} onClick={() => onAction({ kind: request.context === 'staged' ? 'unstageHunk' : 'stageHunk', diffSnapshotId: available.diffSnapshotId, fileId: request.fileId, hunkId: hunk.hunkId })}>{label}</button></div>
            <Patch text={hunk.text} layout={layout} />
          </section>)}
          {!available.hunks.length && <p>No text changes.</p>}
        </> : <><Patch text={diff.text} layout={layout} />{!diff.text && <p>No text changes.</p>}</>}
      </> : <p>{diff.kind === 'binary' ? diff.description : diff.reason}</p>}
      {working && !available && <button disabled>{label}</button>}
    </>}
    </div>
  </section>;
}
