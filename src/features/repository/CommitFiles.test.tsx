import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { ChangedFile } from '../../api/types';
import { CommitFiles } from './CommitFiles';

afterEach(cleanup);
const files: ChangedFile[] = [
  { fileId: 'a', displayPath: 'src/new.ts', oldDisplayPath: 'lib/old.ts', changeKind: 'renamed', indexStatus: null, worktreeStatus: null, untracked: false, conflicted: false, submodule: false },
  { fileId: 'b', displayPath: 'docs/readme.md', oldDisplayPath: null, changeKind: 'added', indexStatus: null, worktreeStatus: null, untracked: false, conflicted: false, submodule: false },
];

it('filters current and previous paths, preserves selection and shows no-match state', () => {
  const onSelect = vi.fn();
  render(<CommitFiles files={files} selection={{ repositoryId: 'r', snapshotId: 's', fileId: 'a', context: 'commit' }} onSelect={onSelect} diff={<p>Selected diff</p>} />);
  expect(screen.getByRole('button', { name: 'src/new.ts' }).getAttribute('aria-pressed')).toBe('true');
  fireEvent.change(screen.getByLabelText('Filter commit files'), { target: { value: 'LIB/OLD' } });
  expect(screen.queryByRole('button', { name: 'docs/readme.md' })).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'src/new.ts' }));
  expect(onSelect).toHaveBeenCalledWith(files[0]);
  fireEvent.change(screen.getByLabelText('Filter commit files'), { target: { value: 'missing' } });
  expect(screen.getByText('No files match this filter.')).toBeTruthy();
  expect(screen.getByText('Selected diff')).toBeTruthy();
});

it('supports bounded arrow/Home/End selection without mutation controls', () => {
  const onSelect = vi.fn();
  render(<CommitFiles files={files} selection={null} onSelect={onSelect} diff={null} />);
  fireEvent.keyDown(screen.getByRole('button', { name: 'src/new.ts' }), { key: 'ArrowDown' });
  expect(onSelect).toHaveBeenLastCalledWith(files[1]);
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'docs/readme.md' }));
  fireEvent.keyDown(document.activeElement!, { key: 'Home' });
  expect(onSelect).toHaveBeenLastCalledWith(files[0]);
  expect(screen.queryByRole('button', { name: /stage/i })).toBeNull();
});

it('clears filters for a new comparison and distinguishes empty comparisons from no matches', () => {
  const { rerender } = render(<CommitFiles key="first" files={files} selection={null} onSelect={vi.fn()} diff={null} />);
  fireEvent.change(screen.getByLabelText('Filter commit files'), { target: { value: 'old' } });
  rerender(<CommitFiles key="second" files={[]} selection={null} onSelect={vi.fn()} diff={null} />);
  expect((screen.getByLabelText('Filter commit files') as HTMLInputElement).value).toBe('');
  expect(screen.getByText('No changed files for this comparison.')).toBeTruthy();
});
