import type { CSSProperties } from 'react';

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

export function Patch({ text }: { text: string }) {
  const rows = patchRows(text);
  const digits = rows.reduce((width, row) => Math.max(width, String(row.oldNumber ?? '').length, String(row.newNumber ?? '').length), 3);
  return <div className="patch-view" style={{ '--gutter-width': `${digits + 2}ch` } as CSSProperties}>
    <div className="diff-columns" aria-hidden="true"><span>Old</span><span>New</span><span>Unified diff</span></div>
    <pre className="unified-diff" tabIndex={0} aria-label="Unified diff; old and new line numbers in the left gutters">{rows.map((row, index) => <span key={index} className={`diff-line diff-${row.kind}`}>
      <span className="diff-line-number" aria-hidden="true" data-line={row.oldNumber ?? ''} />
      <span className="diff-line-number" aria-hidden="true" data-line={row.newNumber ?? ''} />
      <span className="diff-source">{row.text}</span>
    </span>)}</pre>
  </div>;
}
