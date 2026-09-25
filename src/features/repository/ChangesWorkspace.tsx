import { useRef, useState, type ReactNode } from 'react';
import type { ChangedFile, Changes, DiffRequest, RepositoryAction } from '../../api/types';

const hasChange = (status: string | null) => Boolean(status?.trim() && status !== '.' && status !== '?');
const groups = ['conflicted', 'staged', 'unstaged', 'untracked'] as const;
type Group = typeof groups[number];
const labels: Record<Group, string> = { conflicted: 'Conflicted', staged: 'Staged', unstaged: 'Unstaged', untracked: 'Untracked' };
function belongs(file: ChangedFile, group: Group) {
  if (group === 'conflicted') return file.conflicted;
  return !file.conflicted && (group === 'untracked' ? file.untracked : group === 'staged' ? hasChange(file.indexStatus) : hasChange(file.worktreeStatus) && !file.untracked);
}
function statusLabel(file: ChangedFile, group: Group) {
  if (file.conflicted) return ['!', 'Conflict'];
  if (file.untracked) return ['?', 'New file'];
  const code = (group === 'staged' ? file.indexStatus : file.worktreeStatus) ?? 'M';
  return [code, ({ A: 'Added', D: 'Deleted', R: 'Renamed', C: 'Copied', T: 'Type changed', M: 'Modified' } as Record<string, string>)[code] ?? 'Changed'];
}

export function ChangesWorkspace({ changes, selection, disabled, blockedReason, onSelect, onAction, composer, diff }: {
  changes: Changes | null; selection: DiffRequest | null; disabled: boolean; blockedReason: string | null;
  onSelect: (file: ChangedFile, context: 'staged' | 'unstaged') => void;
  onAction: (action: RepositoryAction) => void; composer: ReactNode; diff: ReactNode;
}) {
  const [filter, setFilter] = useState('');
  const list = useRef<HTMLDivElement>(null);
  const query = filter.trim().toLocaleLowerCase();
  const matches = (file: ChangedFile) => `${file.displayPath}\n${file.oldDisplayPath ?? ''}`.toLocaleLowerCase().includes(query);
  const filtered = changes?.files.filter(matches) ?? [];
  const mutate = (files: ChangedFile[], group: Group) => {
    if (!changes || disabled || blockedReason || !files.length) return;
    onAction({ kind: group === 'staged' ? 'unstage' : 'stage', snapshotId: changes.snapshotId, fileIds: files.map(file => file.fileId) });
  };
  return <section className="changes-workspace" aria-label="Working changes">
    <div className="changes-sidebar">
      <div className="changes-list-heading"><h2>Changed files <span className="file-count">{changes?.files.length ?? '…'}</span></h2>
        <input type="search" aria-label="Filter changed files" placeholder="Filter files…" value={filter} onChange={event => setFilter(event.target.value)} />
      </div>
      <div className="changes-file-scroll" ref={list} onKeyDown={event => {
        if (!(event.target instanceof HTMLElement) || !event.target.matches('.file-preview') || !['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
        const rows = [...(list.current?.querySelectorAll<HTMLButtonElement>('details[open] .file-preview') ?? [])];
        const current = rows.indexOf(event.target as HTMLButtonElement);
        if (current < 0 || !rows.length) return;
        event.preventDefault();
        const next = event.key === 'Home' ? 0 : event.key === 'End' ? rows.length - 1 : Math.max(0, Math.min(rows.length - 1, current + (event.key === 'ArrowDown' ? 1 : -1)));
        rows[next].focus(); rows[next].click();
      }}>
        {!changes ? <p className="empty">Loading changes…</p> : <>
          {groups.map(group => {
            const all = changes.files.filter(file => belongs(file, group));
            const files = all.filter(matches);
            if (group === 'conflicted' && !all.length) return null;
            if (query && !files.length) return null;
            return <details className="change-group" key={group} open>
              <summary>{labels[group]} <span className="file-count">{query ? `${files.length}/${all.length}` : all.length}</span></summary>
              {files.length > 0 && group !== 'conflicted' && <div className="change-group-actions"><button disabled={disabled || Boolean(blockedReason)} title={blockedReason ?? undefined} onClick={() => mutate(files, group)}>
                {group === 'staged' ? 'Unstage' : 'Stage'} {query ? 'visible' : 'all'} ({files.length})
              </button></div>}
              <ul className="file-list">{files.map(file => {
                const context = group === 'staged' ? 'staged' : 'unstaged';
                const selected = selection?.fileId === file.fileId && selection.context === context;
                const slash = file.displayPath.lastIndexOf('/');
                const [code, meaning] = statusLabel(file, group);
                return <li key={file.fileId} className={selected ? 'file-selected' : undefined}>
                  <button className="file-preview" title={file.displayPath} aria-label={file.displayPath} aria-description={`${meaning}${file.submodule ? ', submodule' : ''}${file.oldDisplayPath ? `, previously ${file.oldDisplayPath}` : ''}`} aria-pressed={selected} onClick={() => onSelect(file, context)}>
                    <span className={`file-status file-status-${code === '!' ? 'conflict' : code}`} title={meaning} aria-hidden="true">{code}</span>
                    <span className="file-name"><span>{file.displayPath.slice(slash + 1)}</span><small>{file.oldDisplayPath ? `${file.oldDisplayPath} → ` : ''}{slash >= 0 ? file.displayPath.slice(0, slash) : meaning}{file.submodule ? ' · submodule' : ''}</small></span>
                  </button>
                  <button className="file-action" title={blockedReason ?? `${group === 'staged' ? 'Unstage' : 'Stage'} ${file.displayPath}`} disabled={disabled || Boolean(blockedReason)} onClick={() => mutate([file], group)}>{group === 'staged' ? 'Unstage' : file.conflicted ? 'Stage resolution' : 'Stage'}</button>
                </li>;
              })}</ul>
              {!files.length && <p className="file-group-empty">No {labels[group].toLowerCase()} files</p>}
            </details>;
          })}
          {!changes.files.length ? <p className="empty">Working tree clean.</p> : query && !filtered.length ? <p className="empty">No files match “{filter}”.</p> : null}
        </>}
      </div>
      <div className="changes-composer">{composer}</div>
    </div>
    <div className="changes-diff" aria-label="Change preview">{diff ?? <div className="diff-empty"><span aria-hidden="true">±</span><h2>{changes?.files.length === 0 ? 'Working tree clean' : 'Select a file to review'}</h2><p>{changes?.files.length === 0 ? 'There are no local file changes to review.' : 'Review its diff here, then stage a file or an individual hunk.'}</p></div>}</div>
  </section>;
}
