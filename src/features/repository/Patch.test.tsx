import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import { limitPatch, MAX_PATCH_LINES, Patch, patchRows, splitPatchRows } from './Patch';

afterEach(cleanup);

it('numbers context, additions and removals independently across multiple hunks', () => {
  const rows = patchRows('--- a/file\n+++ b/file\n@@ -2,2 +4,3 @@ function\n same\n-old\n+new\n+extra\n@@ -20 +40 @@\n-last\n+next\n');
  expect(rows.map(({ kind, oldNumber, newNumber }) => [kind, oldNumber, newNumber])).toEqual([
    ['meta', null, null], ['meta', null, null], ['hunk', null, null],
    ['context', 2, 4], ['remove', 3, null], ['add', null, 5], ['add', null, 6],
    ['hunk', null, null], ['remove', 20, null], ['add', null, 40],
  ]);
});

it('handles zero-length ranges, header-looking code and no-newline markers', () => {
  const rows = patchRows('@@ -0,0 +1,2 @@\n+++ not a file header\n\\ No newline at end of file\n+second\n@@ -9,1 +0,0 @@\n--- removed code\n\\ No newline at end of file');
  expect(rows[1]).toMatchObject({ kind: 'add', newNumber: 1, oldNumber: null });
  expect(rows[2]).toMatchObject({ kind: 'meta', newNumber: null });
  expect(rows[3].newNumber).toBe(2);
  expect(rows[5]).toMatchObject({ kind: 'remove', oldNumber: 9, newNumber: null });
});

it('does not invent line numbers for metadata, combined diffs or text beyond a hunk range', () => {
  expect(patchRows('diff --git a/a b/a\n@@@ -1 -1 +1 @@@\n++combined\n').every(row => row.oldNumber === null && row.newNumber === null)).toBe(true);
  expect(patchRows('@@ -1 +1 @@\n-a\n+b\n+outside').at(-1)).toMatchObject({ kind: 'meta', newNumber: null });
  expect(patchRows('')).toEqual([]);
});

it('renders source as literal selectable text without gutter numbers contaminating the patch', () => {
  const text = '@@ -2 +2 @@\n-<script>old</script>\n+<img src=x>\n';
  const { container } = render(<Patch text={text} />);
  expect(container.querySelector('pre')?.textContent).toBe(text);
  expect(container.querySelector('script,img')).toBeNull();
  expect(container.querySelector('.diff-add [data-line="2"]')).toBeTruthy();
  expect(container.querySelector('pre')?.tabIndex).toBe(0);
});

it('pairs unequal replacement blocks and keeps following context aligned', () => {
  const rows = splitPatchRows(patchRows('@@ -2,3 +4,4 @@\n-old1\n-old2\n+new1\n+new2\n+extra\n same\n'));
  expect(rows.slice(1).map(row => row.kind === 'pair' && [row.old?.oldNumber ?? null, row.next?.newNumber ?? null])).toEqual([[2, 4], [3, 5], [null, 6], [4, 7]]);
});

it.each([
  ['@@ -0,0 +1,2 @@\n+one\n+two\n', [[null, 1], [null, 2]]],
  ['@@ -5,2 +0,0 @@\n-one\n-two\n', [[5, null], [6, null]]],
  ['@@ -1 +1 @@\n-old\n+new\n@@ -9 +12 @@\n next\n', [[1, 1], [9, 12]]],
])('aligns additions/deletions and multiple hunks: %s', (text, expected) => {
  const rows = splitPatchRows(patchRows(text));
  expect(rows.flatMap(row => row.kind === 'pair' ? [[row.old?.oldNumber ?? null, row.next?.newNumber ?? null]] : [])).toEqual(expected);
});

it('attaches no-newline markers to the correct side without splitting replacement pairs or mutating source rows', () => {
  const rows = patchRows('@@ -1 +1 @@\n-old\n\\ No newline at end of file\n+new\n\\ No newline at end of file');
  const original = JSON.stringify(rows);
  const aligned = splitPatchRows(rows);
  expect(aligned).toHaveLength(2);
  const row = aligned[1];
  expect(row.kind === 'pair' && row.old?.text).toBe('-old\n\\ No newline at end of file\n');
  expect(row.kind === 'pair' && row.next?.text).toBe('+new\n\\ No newline at end of file');
  expect(JSON.stringify(rows)).toBe(original);
});

it('renders split literal content, line numbers, blank counterparts and focusable hunk navigation targets', () => {
  const { container } = render(<Patch layout="split" text={'diff --git a/β b/β\n@@ -1 +1,2 @@\n-<script>old</script>\n+<img src=x>\n+extra\r\n'} />);
  expect(screen.getByRole('region', { name: /Side-by-side diff/ }).tabIndex).toBe(0);
  expect(container.querySelector('script,img')).toBeNull();
  expect(container.querySelectorAll('.split-cell.diff-gap')).toHaveLength(1);
  expect(container.querySelector('.diff-add [data-line="2"]')).toBeTruthy();
  expect(container.querySelector<HTMLElement>('[data-diff-hunk]')?.tabIndex).toBe(-1);
  expect(container.querySelector('.split-diff')?.textContent).toContain('+extra\r\n');
});

it('falls back to unnumbered unified text for combined diff headers', () => {
  const text = '@@@ -1 -1 +1 @@@\n++combined\n';
  const { container } = render(<Patch layout="split" text={text} />);
  expect(screen.getByRole('status').textContent).toContain('two-side line alignment is unavailable');
  expect(container.querySelector('pre')?.textContent).toBe(text);
  expect(container.querySelector('.split-diff')).toBeNull();
});

it.each(['unified', 'split'] as const)('bounds large %s displays and retains an explicit limit warning', layout => {
  const text = `@@ -0,0 +1,${MAX_PATCH_LINES + 5} @@\n` + '+line\n'.repeat(MAX_PATCH_LINES + 5);
  const { container } = render(<Patch layout={layout} text={text} />);
  expect(limitPatch(text).limited).toBe(true);
  expect(limitPatch('+line\n'.repeat(MAX_PATCH_LINES)).limited).toBe(false);
  expect(screen.getByRole('status').textContent).toContain('first 10,000 patch lines');
  expect(container.querySelectorAll(layout === 'split' ? '.split-row,.split-meta' : '.diff-line')).toHaveLength(MAX_PATCH_LINES);
});
