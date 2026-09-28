import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { fixtures } from '../test/fixtures';
import { layoutPage } from './layout';
import { HistoryGraph } from './HistoryGraph';

// Isolate event/focus semantics; native acceptance covers real virtualization.
vi.mock('@tanstack/react-virtual', () => ({ useVirtualizer: ({ count }: { count: number }) => ({
  getTotalSize: () => count * 44, scrollToIndex: vi.fn(),
  getVirtualItems: () => Array.from({ length: count }, (_, index) => ({ index, start: index * 44 })),
}) }));
afterEach(cleanup);
const rows = layoutPage(fixtures.history.commits).rows;

it('keeps arrow-key focus on the stable history viewport and announces selection', () => {
  const onSelect = vi.fn();
  render(<HistoryGraph rows={rows} selected={rows[0].commit.oid} onSelect={onSelect} loading={false} more onMore={vi.fn()} />);
  const row = screen.getByRole('button', { name: /Merge feature/ });
  act(() => row.focus()); fireEvent.keyDown(row, { key: 'ArrowDown' });
  expect(document.activeElement).toBe(screen.getByLabelText('Commit history'));
  expect(onSelect).toHaveBeenCalledWith(rows[1].commit.oid);
  expect(screen.getByRole('status').textContent).toContain(rows[0].commit.subject);
});
it('does not hijack Load older commits or modified/composing navigation keys', () => {
  const onSelect = vi.fn();
  render(<HistoryGraph rows={rows} selected={null} onSelect={onSelect} loading={false} more onMore={vi.fn()} />);
  fireEvent.keyDown(screen.getByRole('button', { name: 'Load older commits' }), { key: 'Home' });
  for (const extra of [{ ctrlKey: true }, { altKey: true }, { metaKey: true }, { shiftKey: true }, { isComposing: true }]) {
    fireEvent.keyDown(screen.getByLabelText('Commit history'), { key: 'ArrowDown', ...extra });
  }
  expect(onSelect).not.toHaveBeenCalled();
});
