import { useMemo, type CSSProperties } from 'react';

export type DiffLayout = 'unified' | 'split';
export const MAX_PATCH_LINES = 10000;
export function limitPatch(text: string) {
  const lines = text.split('\n', MAX_PATCH_LINES + 2);
  if (lines.at(-1) === '') lines.pop();
  const limited = lines.length > MAX_PATCH_LINES;
  return { text: limited ? lines.slice(0, MAX_PATCH_LINES).join('\n') + '\n' : text, limited };
}

export function DiffLayoutControl({ value, onChange }: { value: DiffLayout; onChange: (value: DiffLayout) => void }) {
  return <label className="diff-layout-control">Diff layout<select aria-label="Diff layout" value={value} onChange={event => onChange(event.target.value as DiffLayout)}>
    <option value="unified">Unified</option><option value="split">Side-by-side</option>
  </select></label>;
}

// Presentation only: mutation capabilities continue to come from opaque backend
// hunk tokens. Never reconstruct an actionable patch from these display rows.
export function patchRows(text: string) {
  let oldLine = 0, newLine = 0, oldRemaining = 0, newRemaining = 0;
  const lines = text.split('\n');
  if (lines.at(-1) === '') lines.pop();
  return lines.map((line, index) => {
    let oldNumber: number | null = null, newNumber: number | null = null;
    let kind = 'meta';
    const header = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/.exec(line);
    if (header) {
      oldLine = Number(header[1]); newLine = Number(header[3]);
      oldRemaining = Number(header[2] ?? 1); newRemaining = Number(header[4] ?? 1);
      kind = 'hunk';
    } else if (line.startsWith('-') && oldRemaining > 0) {
      oldNumber = oldLine++; oldRemaining--; kind = 'remove';
    } else if (line.startsWith('+') && newRemaining > 0) {
      newNumber = newLine++; newRemaining--; kind = 'add';
    } else if (line.startsWith(' ') && oldRemaining > 0 && newRemaining > 0) {
      oldNumber = oldLine++; newNumber = newLine++; oldRemaining--; newRemaining--; kind = 'context';
    } else if (!line.startsWith('\\')) {
      // Unknown/combined headers have no trustworthy two-column numbering.
      oldRemaining = 0; newRemaining = 0;
    }
    return { kind, oldNumber, newNumber, text: line + (index < lines.length - 1 || text.endsWith('\n') ? '\n' : '') };
  });
}

type PatchRow = ReturnType<typeof patchRows>[number];
type SplitRow = { kind: 'pair'; old: PatchRow | null; next: PatchRow | null } | { kind: 'meta' | 'hunk'; text: string };

// Pair adjacent removal/addition blocks by position, not by inferred similarity.
// No-newline markers stay with their side and never consume a source line.
export function splitPatchRows(rows: PatchRow[]): SplitRow[] {
  const result: SplitRow[] = [];
  let removed: PatchRow[] = [], added: PatchRow[] = [];
  let last: PatchRow | null = null;
  const flush = () => {
    for (let index = 0; index < Math.max(removed.length, added.length); index++) result.push({ kind: 'pair', old: removed[index] ?? null, next: added[index] ?? null });
    removed = []; added = []; last = null;
  };
  for (const row of rows) {
    if (row.kind === 'remove' || row.kind === 'add') {
      last = { ...row };
      (row.kind === 'remove' ? removed : added).push(last);
    } else if (row.text.startsWith('\\') && last) {
      last.text += row.text;
    } else {
      flush();
      if (row.kind === 'context') {
        last = { ...row };
        result.push({ kind: 'pair', old: last, next: last });
      } else result.push({ kind: row.kind === 'hunk' ? 'hunk' : 'meta', text: row.text });
    }
  }
  flush();
  return result;
}

export function Patch({ text, layout = 'unified' }: { text: string; layout?: DiffLayout }) {
  const preview = useMemo(() => limitPatch(text), [text]);
  const rows = useMemo(() => patchRows(preview.text), [preview.text]);
  const combined = /^@@@/m.test(preview.text);
  const split = useMemo(() => layout === 'split' && !combined ? splitPatchRows(rows) : null, [rows, layout, combined]);
  const digits = rows.reduce((width, row) => Math.max(width, String(row.oldNumber ?? '').length, String(row.newNumber ?? '').length), 3);
  return <div className="patch-view" style={{ '--gutter-width': `${digits + 2}ch` } as CSSProperties}>
    {preview.limited && <p role="status" className="diff-limit">Display limited to the first {MAX_PATCH_LINES.toLocaleString('en-US')} patch lines. Use Git or an external editor to inspect the remainder.</p>}
    {layout === 'split' && combined && <p role="status" className="diff-limit">Combined diff shown as unified text; two-side line alignment is unavailable.</p>}
    {split ? <div className="split-scroll" tabIndex={0} role="region" aria-label="Side-by-side diff; old on the left, new on the right">
      <div className="split-diff">
        <div className="split-heading">Old</div><div className="split-heading">New</div>
        {split.map((row, index) => row.kind !== 'pair' ? <div key={index} className={`split-meta diff-${row.kind}`} tabIndex={row.kind === 'hunk' ? -1 : undefined} data-diff-hunk={row.kind === 'hunk' ? '' : undefined}>{row.text}</div> : <div className="split-row" key={index}>
          <div className={`split-cell diff-${row.old?.kind ?? 'gap'}`}><span className="diff-line-number" aria-hidden="true" data-line={row.old?.oldNumber ?? ''} /><span className="diff-source">{row.old?.text ?? ''}</span></div>
          <div className={`split-cell diff-${row.next?.kind ?? 'gap'}`}><span className="diff-line-number" aria-hidden="true" data-line={row.next?.newNumber ?? ''} /><span className="diff-source">{row.next?.text ?? ''}</span></div>
        </div>)}
      </div>
    </div> : <>
    <div className="diff-columns" aria-hidden="true"><span>Old</span><span>New</span><span>Unified diff</span></div>
    <pre className="unified-diff" tabIndex={0} aria-label="Unified diff; old and new line numbers in the left gutters">{rows.map((row, index) => <span key={index} className={`diff-line diff-${row.kind}`} tabIndex={row.kind === 'hunk' ? -1 : undefined} data-diff-hunk={row.kind === 'hunk' ? '' : undefined}>
      <span className="diff-line-number" aria-hidden="true" data-line={row.oldNumber ?? ''} />
      <span className="diff-line-number" aria-hidden="true" data-line={row.newNumber ?? ''} />
      <span className="diff-source">{row.text}</span>
    </span>)}</pre></>}
  </div>;
}
