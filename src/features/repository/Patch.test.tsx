import { cleanup, render } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import { Patch, patchRows } from './Patch';

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
