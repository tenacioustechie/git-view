import { useRef, useState, type ReactNode } from 'react';
import type { ChangedFile, DiffRequest } from '../../api/types';

const codes: Record<string, string> = { added: 'A', deleted: 'D', modified: 'M', renamed: 'R', copied: 'C', typeChanged: 'T' };

export function CommitFiles({ files, selection, onSelect, diff }: {
  files: ChangedFile[]; selection: DiffRequest | null; onSelect: (file: ChangedFile) => void; diff: ReactNode;
}) {
  const [filter, setFilter] = useState('');
  const list = useRef<HTMLUListElement>(null);
  const query = filter.trim().toLocaleLowerCase();
  const visible = files.filter(file => `${file.displayPath}\n${file.oldDisplayPath ?? ''}`.toLocaleLowerCase().includes(query));
  return <section className="commit-file-review" aria-label="Commit file review">
    <div className="commit-files">
      <div className="commit-files-heading"><h3>Changed files <span className="file-count">{query ? `${visible.length}/${files.length}` : files.length}</span></h3>
        <input type="search" aria-label="Filter commit files" placeholder="Filter files…" value={filter} onChange={event => setFilter(event.target.value)} />
      </div>
      <ul className="file-list" ref={list} onKeyDown={event => {
        if (!(event.target instanceof HTMLButtonElement) || !['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
        const rows = [...(list.current?.querySelectorAll<HTMLButtonElement>('button') ?? [])];
        const current = rows.indexOf(event.target);
        if (current < 0) return;
        event.preventDefault();
        const next = event.key === 'Home' ? 0 : event.key === 'End' ? rows.length - 1 : Math.max(0, Math.min(rows.length - 1, current + (event.key === 'ArrowDown' ? 1 : -1)));
        rows[next].focus(); rows[next].click();
      }}>{visible.map(file => {
        const selected = selection?.context === 'commit' && selection.fileId === file.fileId;
        const slash = file.displayPath.lastIndexOf('/');
        const code = codes[file.changeKind ?? ''] ?? '•';
        return <li key={file.fileId} className={selected ? 'file-selected' : undefined}>
          <button className="file-preview" title={file.displayPath} aria-label={file.displayPath} aria-description={`${file.changeKind ?? 'changed'}${file.oldDisplayPath ? `, previously ${file.oldDisplayPath}` : ''}${file.submodule ? ', submodule' : ''}`} aria-pressed={selected} onClick={() => onSelect(file)}>
            <span className={`file-status file-status-${code}`} aria-hidden="true">{code}</span>
            <span className="file-name"><span>{file.displayPath.slice(slash + 1)}</span><small>{file.oldDisplayPath ? `${file.oldDisplayPath} → ` : ''}{slash >= 0 ? file.displayPath.slice(0, slash) : file.changeKind}{file.submodule ? ' · submodule' : ''}</small></span>
          </button>
        </li>;
      })}</ul>
      {!visible.length && <p className="file-group-empty">{files.length ? 'No files match this filter.' : 'No changed files for this comparison.'}</p>}
    </div>
    <div className="commit-diff" key={selection?.fileId ?? ''} aria-label="Commit diff preview">{diff ?? <div className="diff-empty"><h2>Select a changed file</h2><p>Review this commit’s changes. Historical diffs are read-only.</p></div>}</div>
  </section>;
}
