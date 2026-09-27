import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ChangedFile, Changes } from '../../api/types';
import { ChangesWorkspace } from './ChangesWorkspace';

const file: ChangedFile = { fileId: 'both', displayPath: 'src/a.ts', oldDisplayPath: null, changeKind: 'modified', indexStatus: 'M', worktreeStatus: 'M', untracked: false, conflicted: false, submodule: false };
const changes: Changes = { snapshotId: 'snapshot-1', files: [file,
  { ...file, fileId: 'new', displayPath: 'new.txt', indexStatus: '?', worktreeStatus: '?', untracked: true },
  { ...file, fileId: 'other', displayPath: 'src/b.ts', indexStatus: '.' },
  { ...file, fileId: 'conflict', displayPath: 'conflict.txt', conflicted: true },
] };
function setup(overrides: Partial<Parameters<typeof ChangesWorkspace>[0]> = {}) {
  const props = { changes, selection: null, disabled: false, blockedReason: null, onSelect: vi.fn(), onAction: vi.fn(), composer: null, diff: null, ...overrides };
  const view = render(<ChangesWorkspace {...props} />);
  return { ...view, props };
}
afterEach(cleanup);

describe('working-file bulk selection', () => {
  it('stages checked unstaged and untracked files with exact snapshot tokens, independently of preview', () => {
    const { props } = setup();
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select unstaged file src/a.ts' }));
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select untracked file new.txt' }));
    expect(props.onAction).not.toHaveBeenCalled();
    expect(props.onSelect).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'src/b.ts' }));
    expect(props.onSelect).toHaveBeenCalledWith(changes.files[2], 'unstaged');
    fireEvent.click(screen.getByRole('button', { name: 'Stage selected (2)' }));
    expect(props.onAction).toHaveBeenCalledExactlyOnceWith({ kind: 'stage', snapshotId: 'snapshot-1', fileIds: ['both', 'new'] });
  });
  it('keeps the staged and unstaged versions of the same file separate', () => {
    const { props } = setup();
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select staged file src/a.ts' }));
    expect((screen.getByRole('checkbox', { name: 'Select unstaged file src/a.ts' }) as HTMLInputElement).checked).toBe(false);
    expect(screen.queryByRole('button', { name: /Stage selected/ })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Unstage selected (1)' }));
    expect(props.onAction).toHaveBeenCalledExactlyOnceWith({ kind: 'unstage', snapshotId: 'snapshot-1', fileIds: ['both'] });
  });
  it('clears checked items on filter edits, including when the old filter returns', () => {
    const { props } = setup();
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select untracked file new.txt' }));
    fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'src/' } });
    expect(screen.queryByRole('region', { name: 'Selected file actions' })).toBeNull();
    fireEvent.change(screen.getByRole('searchbox'), { target: { value: '' } });
    expect((screen.getByRole('checkbox', { name: 'Select untracked file new.txt' }) as HTMLInputElement).checked).toBe(false);
    expect(props.onAction).not.toHaveBeenCalled();
  });
  it('clears on refreshed snapshots even when file tokens are reused; selection does not reappear', () => {
    const { props, rerender } = setup();
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select unstaged file src/a.ts' }));
    rerender(<ChangesWorkspace {...props} changes={{ ...changes, snapshotId: 'snapshot-2' }} />);
    expect(screen.queryByRole('region', { name: 'Selected file actions' })).toBeNull();
    rerender(<ChangesWorkspace {...props} />);
    expect((screen.getByRole('checkbox', { name: 'Select unstaged file src/a.ts' }) as HTMLInputElement).checked).toBe(false);
    expect(props.onAction).not.toHaveBeenCalled();
  });
  it('supports Tab/Space checkbox selection and clearing without staging', async () => {
    const user = userEvent.setup();
    const { props } = setup();
    const checkbox = screen.getByRole('checkbox', { name: 'Select staged file src/a.ts' });
    checkbox.focus();
    await user.keyboard(' ');
    expect((checkbox as HTMLInputElement).checked).toBe(true);
    await user.keyboard(' ');
    expect((checkbox as HTMLInputElement).checked).toBe(false);
    await user.keyboard(' ');
    fireEvent.click(screen.getByRole('button', { name: 'Clear selection' }));
    expect(screen.queryByRole('region', { name: 'Selected file actions' })).toBeNull();
    expect(props.onAction).not.toHaveBeenCalled();
  });
  it.each([{ disabled: true }, { blockedReason: 'Finish recovery first' }])('disables bulk mutations while blocked: %j', override => {
    const { props, rerender } = setup();
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select unstaged file src/a.ts' }));
    rerender(<ChangesWorkspace {...props} {...override} />);
    expect((screen.getByRole('button', { name: 'Stage selected (1)' }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole('checkbox', { name: 'Select unstaged file src/a.ts' }) as HTMLInputElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: 'Stage selected (1)' }));
    expect(props.onAction).not.toHaveBeenCalled();
  });
  it('excludes conflicted files from bulk selection and preserves individual Stage resolution', () => {
    const { props } = setup();
    expect(screen.queryByRole('checkbox', { name: /conflict.txt/ })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Stage resolution' }));
    expect(props.onAction).toHaveBeenCalledExactlyOnceWith({ kind: 'stage', snapshotId: 'snapshot-1', fileIds: ['conflict'] });
  });
});
