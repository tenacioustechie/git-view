import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { GitRef, RepositoryStatus } from '../../api/types';
import { fixtures } from '../../test/fixtures';
import { OperationContext } from './OperationContext';
import { Rebase } from './Rebase';

afterEach(cleanup);
const target = 'b'.repeat(40);
const refs: GitRef[] = [
  { kind: 'local', fullName: 'refs/heads/main', label: 'main', oid: target },
  { kind: 'local', fullName: 'refs/heads/feature', label: 'feature', oid: 'a'.repeat(40) },
  { kind: 'remote', fullName: 'refs/remotes/origin/topic', label: 'origin/topic', oid: target },
  { kind: 'remote', fullName: 'refs/remotes/origin/HEAD', label: 'origin/HEAD', oid: target },
];
function status(): RepositoryStatus { return { ...fixtures.clean, head: { kind: 'branch', name: 'feature', oid: 'a'.repeat(40) } }; }

describe('rebase controls', () => {
  it('chooses exact local or fetched remote refs without changing history scope', () => {
    const onPlan = vi.fn();
    const current = status();
    render(<Rebase status={current} refs={refs} active={false} onPlan={onPlan} />);
    const selector = screen.getByLabelText('Rebase onto branch') as HTMLSelectElement;
    expect(selector.querySelector('option[value="refs/heads/feature"]')).toBeNull();
    expect(selector.querySelector('option[value="refs/remotes/origin/HEAD"]')).toBeNull();
    fireEvent.change(selector, { target: { value: 'refs/remotes/origin/topic' } });
    fireEvent.click(screen.getByRole('button', { name: 'Preview rebase' }));
    expect(onPlan).toHaveBeenCalledWith({ kind: 'startRebase', statusSnapshotId: current.snapshotId, ontoRef: 'refs/remotes/origin/topic' }, expect.any(Function));
    const fresh = onPlan.mock.calls[0][1] as () => boolean;
    expect(fresh()).toBe(true);
    fireEvent.change(selector, { target: { value: 'refs/heads/main' } });
    expect(fresh()).toBe(false);
  });
  it('requires a clean attached idle branch', () => {
    const onPlan = vi.fn();
    const current = { ...status(), dirty: true };
    render(<Rebase status={current} refs={refs} active={false} onPlan={onPlan} />);
    fireEvent.change(screen.getByLabelText('Rebase onto branch'), { target: { value: 'refs/heads/main' } });
    expect((screen.getByRole('button', { name: 'Preview rebase' }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText(/clean local branch/)).toBeTruthy();
    expect(onPlan).not.toHaveBeenCalled();
  });
  it('shows replay context and reviewed recovery controls at a known conflict', () => {
    const onPlan = vi.fn();
    const current: RepositoryStatus = { ...status(), head: { kind: 'detached', oid: target }, gitState: 'rebase', dirty: true,
      counts: { staged: 0, unstaged: 1, untracked: 0, conflicted: 1 },
      operationContext: { kind: 'rebase', sourceOids: ['c'.repeat(40)], sourceRef: 'refs/heads/main', originalHead: 'a'.repeat(40), targetBranch: 'feature', stopReason: 'conflicts', canContinue: false, canAbort: true, canSkip: true,
        reason: 'Resolve the conflict.', rebase: { ontoRef: 'refs/heads/main', ontoOid: target, currentOid: 'c'.repeat(40), completed: 0, total: 2, appOwned: true } } };
    render(<OperationContext status={current} files={[]} active={false} onPlan={onPlan} onReview={vi.fn()} />);
    expect(screen.getByText('Rebase in progress')).toBeTruthy();
    expect(screen.getByText(/ours is the chosen base/)).toBeTruthy();
    expect(screen.getByText(/0 of 2 completed/)).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Continue rebase' }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: 'Skip commit' }));
    expect(onPlan).toHaveBeenCalledWith({ kind: 'skipRebase', statusSnapshotId: current.snapshotId });
    fireEvent.click(screen.getByRole('button', { name: 'Abort rebase' }));
    expect(onPlan).toHaveBeenCalledWith({ kind: 'abortRebase', statusSnapshotId: current.snapshotId });
  });
});
