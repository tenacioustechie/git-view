import type { DiffRequest, DiffResult, RepositoryAction } from '../../api/types';

function Patch({ text }: { text: string }) {
  const lines = text.split('\n');
  return <pre className="unified-diff">{lines.map((line, index) => <span key={index} className={line.startsWith('+') ? 'diff-add' : line.startsWith('-') ? 'diff-remove' : line.startsWith('@@') ? 'diff-hunk' : ''}>{line}{index < lines.length - 1 ? '\n' : ''}</span>)}</pre>;
}

export function WorkingDiff({ request, displayPath, diff, active, blockedReason, onAction }: {
  request: DiffRequest; displayPath: string; diff: DiffResult | null; active: boolean;
  blockedReason: string | null; onAction: (action: RepositoryAction) => void;
}) {
  const working = request.context !== 'commit';
  const label = request.context === 'staged' ? 'Unstage hunk' : 'Stage hunk';
  const actions = diff?.kind === 'text' ? diff.hunkActions : undefined;
  const available = working && diff?.kind === 'text' && !diff.truncated && actions?.kind === 'available' && actions.context === request.context ? actions : null;
  const reason = !working || !diff ? null : blockedReason ?? (diff.kind === 'binary' ? 'Binary changes cannot be staged or unstaged by hunk.'
    : diff.kind === 'unavailable' ? diff.reason
      : diff.truncated ? 'Hunk actions require a complete diff. This preview is truncated.'
        : actions?.kind === 'unavailable' ? actions.reason
          : !available ? 'Hunk actions are unavailable for this diff. Refresh and inspect the file again.' : null);
  return <section className="diff-panel" aria-label="File diff">
    <h3>Diff · {request.context}</h3><p className="diff-file-path">{displayPath}</p>
    {working && <p>Hunk actions change the index only. Working-file contents stay unchanged; whole-file Stage and Unstage remain available above.</p>}
    {!diff ? <p>Loading diff…</p> : <>
      {reason && <p role="status">{reason}</p>}
      {diff.kind === 'text' ? <>
        {diff.truncated && <p role="status">Preview truncated: this is not the full patch.</p>}
        {available ? <>
          {available.preamble && <Patch text={available.preamble} />}
          {available.hunks.map((hunk, index) => <section className="diff-hunk-block" aria-label={`Hunk ${index + 1}`} key={hunk.hunkId}>
            <div className="diff-hunk-toolbar"><strong>Hunk {index + 1} · {hunk.header}</strong><button disabled={active || Boolean(reason)} onClick={() => onAction({ kind: request.context === 'staged' ? 'unstageHunk' : 'stageHunk', diffSnapshotId: available.diffSnapshotId, fileId: request.fileId, hunkId: hunk.hunkId })}>{label}</button></div>
            <Patch text={hunk.text} />
          </section>)}
          {!available.hunks.length && <p>No text changes.</p>}
        </> : <><Patch text={diff.text} />{!diff.text && <p>No text changes.</p>}</>}
      </> : <p>{diff.kind === 'binary' ? diff.description : diff.reason}</p>}
      {working && !available && <button disabled>{label}</button>}
    </>}
  </section>;
}
