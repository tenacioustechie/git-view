import { useEffect, useRef, useState } from 'react';
import { api, errorMessage } from '../../api/client';
import type { ChangedFile, CommitDetails, DiffResult, RepositoryAction, RepositoryStatus, StashList } from '../../api/types';
import { DiffLayoutControl, Patch, type DiffLayout } from './Patch';

const messages = new Map<string, string>();
type Section = 'worktree' | 'index' | 'untracked';
export function Stashes({ repositoryId, status, files, active, refreshVersion, onPlan, onClose, diffLayout = 'unified', onDiffLayoutChange }: {
  repositoryId: string; status: RepositoryStatus; files: ChangedFile[]; active: boolean; refreshVersion: number;
  onPlan: (action: RepositoryAction, isCurrent: () => boolean) => void; onClose: () => void;
  diffLayout?: DiffLayout; onDiffLayoutChange?: (layout: DiffLayout) => void;
}) {
  const [list, setList] = useState<StashList | null>(null);
  const [loading, setLoading] = useState(true);
  const [listError, setListError] = useState<string | null>(null);
  const [selected, setSelected] = useState('');
  const [section, setSection] = useState<Section>('worktree');
  const [details, setDetails] = useState<CommitDetails | null>(null);
  const [loadedSection, setLoadedSection] = useState<Section | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [diff, setDiff] = useState<DiffResult | null>(null);
  const [diffLoading, setDiffLoading] = useState(false);
  const [message, setMessage] = useState(messages.get(repositoryId) ?? '');
  const [includeUntracked, setIncludeUntracked] = useState(false);
  const [restoreIndex, setRestoreIndex] = useState(false);
  const selectionVersion = useRef(0);
  const diffVersion = useRef(0);
  useEffect(() => {
    let disposed = false;
    selectionVersion.current++; diffVersion.current++;
    setLoading(true); setListError(null); setDetails(null); setDiff(null);
    api.listStashes(repositoryId).then(result => {
      if (disposed) return;
      // A refresh must invalidate detail reads even if an API cache returns the
      // same list object and React batches the loading transitions together.
      setList({ ...result });
      // Do not substitute another entry with the same object ID or list index.
      setSelected(previous => result.entries.some(entry => entry.entryId === previous) ? previous : '');
    }).catch(cause => { if (!disposed) setListError(errorMessage(cause)); }).finally(() => { if (!disposed) setLoading(false); });
    return () => { disposed = true; selectionVersion.current++; };
  }, [repositoryId, status.snapshotId, refreshVersion]);
  const entry = list?.entries.find(entry => entry.entryId === selected);
  useEffect(() => {
    let disposed = false;
    diffVersion.current++; setDetails(null); setLoadedSection(null); setDetailError(null); setDiff(null); setDiffLoading(false);
    if (!list || !entry || loading || listError || entry.unavailableReason) return;
    api.getStashDetails(repositoryId, list.snapshotId, entry.entryId, section).then(value => { if (!disposed) { setDetails(value); setLoadedSection(section); } })
      .catch(cause => { if (!disposed) setDetailError(errorMessage(cause)); });
    return () => { disposed = true; diffVersion.current++; };
  }, [repositoryId, list, entry, section, loading, listError]);
  const plan = (action: RepositoryAction) => {
    const version = selectionVersion.current;
    onPlan(action, () => selectionVersion.current === version);
  };
  const choose = (entryId: string) => { if (entryId === selected && section === 'worktree') return; selectionVersion.current++; diffVersion.current++; setDetails(null); setDiff(null); setSelected(entryId); setRestoreIndex(false); setSection('worktree'); };
  const inspect = async (fileId: string) => {
    if (!details) return;
    const version = ++diffVersion.current;
    setDiff(null); setDiffLoading(true);
    try {
      const value = await api.getDiff({ repositoryId, snapshotId: details.snapshotId, fileId, context: 'commit' });
      if (version === diffVersion.current) setDiff(value);
    } catch (cause) { if (version === diffVersion.current) setDiff({ kind: 'unavailable', reason: errorMessage(cause) }); }
    finally { if (version === diffVersion.current) setDiffLoading(false); }
  };
  const blocked = status.gitState !== 'idle' ? 'Finish the current Git operation before changing stashes.'
    : status.counts.conflicted > 0 ? 'Resolve and stage the current index conflicts before changing stashes.'
      : list?.recovery ? 'Review the interrupted stash deletion before changing the stash list.' : null;
  const createReason = blocked ?? (status.head.kind === 'unborn' ? 'Create an initial commit before creating a stash.'
    : status.submoduleDirty || files.some(file => file.submodule) ? 'Stashing submodule changes is unsupported. Review the submodule separately.'
      : status.counts.staged + status.counts.unstaged + (includeUntracked ? status.counts.untracked : 0) === 0 ? 'No selected changes to stash. Include untracked files if you want to save them.' : null);
  const applyReason = blocked ?? (status.head.kind === 'unborn' ? 'Switch to a branch with an existing commit before applying a stash.'
    : status.dirty || status.submoduleDirty ? 'Apply requires a clean destination, including untracked files and submodules.' : entry?.unavailableReason ?? null);
  const deleteReason = blocked ?? list?.deleteUnavailableReason ?? entry?.unavailableReason ?? null;
  return <section className="stash-panel" aria-label="Stashes">
    <div className="combine-heading"><h2>Stashes</h2><button onClick={onClose}>Close stashes</button></div>
    <p>Save changes for later. Apply keeps the saved entry; Delete is a separate reviewed action. Stashes are shared by linked worktrees.</p>
    <section aria-label="Create stash" className="stash-create">
      <label>Stash message<input aria-label="Stash message" value={message} disabled={active} onChange={event => { selectionVersion.current++; setMessage(event.target.value); messages.set(repositoryId, event.target.value); }} /></label>
      <label><input type="checkbox" checked={includeUntracked} disabled={active} onChange={event => { selectionVersion.current++; setIncludeUntracked(event.target.checked); }} /> Include untracked files</label>
      <p>Tracked staged and unstaged changes will be saved and removed from the working tree. {includeUntracked ? 'Untracked files will also be saved and removed.' : 'Untracked files remain in place.'} Ignored files are excluded.</p>
      {createReason && <p role="status">{createReason}</p>}
      <button disabled={active || loading || Boolean(listError || createReason) || !message.trim()} onClick={() => plan({ kind: 'createStash', statusSnapshotId: status.snapshotId, message, includeUntracked })}>Create stash</button>
    </section>
    {loading && <p role="status">Loading stashes…</p>}
    {listError && <p role="alert">{listError} Refresh to reload the stash list before acting.</p>}
    {list?.recovery && <section className="stash-recovery" aria-label="Stash deletion recovery"><h3>Interrupted stash deletion</h3><p>{list.recovery.message}</p><p>Recovery backup: <code>{list.recovery.backupRef}</code></p><p>Finalizing may complete the previously approved removal. It does not restore working files; the recovery backup remains.</p><button disabled={active || loading || Boolean(listError) || !list.recovery.canFinalize} onClick={() => plan({ kind: 'finalizeStashDelete', statusSnapshotId: status.snapshotId, recoveryId: list.recovery!.recoveryId })}>Review deletion recovery</button></section>}
    {!loading && !listError && !list?.entries.length && <p>No saved stashes.</p>}
    <ul className="stash-list" aria-label="Saved stashes">{list?.entries.map(item => <li key={item.entryId}><button aria-pressed={selected === item.entryId} title={item.oid} onClick={() => choose(item.entryId)}><strong>{item.message || '(no message)'}</strong><small>{item.createdAt} · {item.branch ?? 'Originating branch unknown'}{item.hasUntracked ? ' · includes untracked files' : ''}</small><code>{item.oid}</code></button></li>)}</ul>
    {entry && <section aria-label="Selected stash" className="selected-stash">
      <h3>{entry.message || '(no message)'}</h3><p><code>{entry.oid}</code> · {entry.createdAt} · {entry.branch ?? 'Originating branch unknown'}</p>
      {entry.unavailableReason && <p role="status">{entry.unavailableReason}</p>}
      <label>Inspect saved changes<select aria-label="Stash section" value={section} onChange={event => { diffVersion.current++; setSection(event.target.value as Section); }}><option value="worktree">All tracked changes</option><option value="index">Saved staged changes</option><option value="untracked" disabled={!entry.hasUntracked}>Saved untracked files</option></select></label>
      {!entry.hasUntracked && <p>This stash contains no saved untracked files.</p>}
      {detailError && <p role="alert">{detailError} Refresh and review the selected entry again.</p>}
      {!details && !detailError && !entry.unavailableReason && <p>Loading saved changes…</p>}
      {details && loadedSection === section && <div data-loaded-section={loadedSection}><p>{details.files.length} changed files in this section.</p><ul className="file-list">{details.files.map(file => <li key={file.fileId}><button title={file.displayPath} onClick={() => { void inspect(file.fileId); }}>{file.changeKind} · {file.displayPath}</button></li>)}</ul></div>}
      {(diffLoading || diff) && <section aria-label="Stash file diff"><h3>Saved file diff</h3>{diffLoading ? <p>Loading diff…</p> : diff?.kind === 'text' ? <>{onDiffLayoutChange && <DiffLayoutControl value={diffLayout} onChange={onDiffLayoutChange} />}{diff.truncated && <p>Preview truncated: this is not the full patch.</p>}<Patch text={diff.text} layout={diffLayout} />{!diff.text && <p>No text changes.</p>}</> : <p>{diff?.kind === 'binary' ? diff.description : diff?.reason}</p>}</section>}
      <p>Current target: {status.head.kind === 'detached' ? `detached at ${status.head.oid}` : status.head.name}. Applying always retains this stash.</p>
      <label><input type="checkbox" checked={restoreIndex} disabled={active} onChange={event => { selectionVersion.current++; setRestoreIndex(event.target.checked); }} /> Restore staged state</label>
      <p>{restoreIndex ? 'Restore the saved index as well as working-file contents.' : 'Use Git’s default apply behavior; some newly added files may be staged. Review the resulting index.'}</p>
      {applyReason && <p role="status">{applyReason}</p>}
      <button disabled={active || loading || Boolean(listError || detailError || applyReason)} onClick={() => plan({ kind: 'applyStash', statusSnapshotId: status.snapshotId, stashSnapshotId: list!.snapshotId, entryId: entry.entryId, restoreIndex })}>Apply stash</button>
      <p>Delete removes this saved entry without undoing applied working changes. A recovery backup is retained.</p>
      {deleteReason && <p role="status">{deleteReason}</p>}
      <button disabled={active || loading || Boolean(listError || detailError || deleteReason)} onClick={() => plan({ kind: 'deleteStash', statusSnapshotId: status.snapshotId, stashSnapshotId: list!.snapshotId, entryId: entry.entryId })}>Delete stash</button>
    </section>}
  </section>;
}
