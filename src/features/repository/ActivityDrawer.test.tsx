import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it } from 'vitest';
import { ActivityDrawer } from './ActivityDrawer';
import type { OperationResult } from '../../api/types';
import { fixtures } from '../../test/fixtures';

afterEach(cleanup);
const operation = (id: string, state: OperationResult['state'] = 'succeeded'): OperationResult => ({
  ...fixtures.failedAction, operationId: id, action: 'fetch', state,
  reason: null, error: null, outcomeUncertain: false, outputTruncated: false,
});
const open = () => fireEvent.click(screen.getByRole('button', { name: 'Repository activity' }));

it('opens and closes with keyboard focus return without trapping Tab', async () => {
  const user = userEvent.setup();
  render(<><ActivityDrawer operations={[]} /><button>After</button></>);
  await user.tab(); await user.keyboard('{Enter}');
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Close activity' }));
  expect(screen.getByText(/No operations recorded/)).toBeTruthy();
  await user.tab(); expect(document.activeElement).toBe(screen.getByLabelText('Operation results'));
  await user.keyboard('{Escape}');
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Repository activity' }));
  expect(screen.queryByRole('region', { name: 'Repository activity details' })).toBeNull();
  await user.tab(); expect(document.activeElement).toBe(screen.getByRole('button', { name: 'After' }));
});

it('shows newest first and filters all attention states, including uncertain success', () => {
  const operations = ['succeeded', 'failed', 'stopped', 'skipped', 'canceled', 'running', 'queued'].map((state, index) => operation(`op-${index}`, state as OperationResult['state']));
  operations.push({ ...operation('uncertain'), outcomeUncertain: true });
  render(<ActivityDrawer operations={operations} />); open();
  expect(screen.getByText('2 active')).toBeTruthy();
  expect(screen.getByText('5 need attention')).toBeTruthy();
  expect(screen.getAllByRole('listitem')[0].textContent).toContain('uncertain');
  fireEvent.change(screen.getByLabelText('Activity filter'), { target: { value: 'attention' } });
  expect(screen.getAllByRole('listitem')).toHaveLength(5);
  expect(screen.getByText(/Outcome uncertain/)).toBeTruthy();
});

it('renders bounded error diagnostics as text with exit, timestamp and truncation context', () => {
  render(<ActivityDrawer operations={[{ ...operation('failure', 'failed'), action: 'checkoutRemoteBranch', reason: 'Inspect current branch', batchId: 'batch-1', exitCode: null,
    error: { code: 'hook_failed', message: 'Hook rejected checkout', details: '<script>do not execute</script>\nHook output', retryable: false, exitCode: 9, outputTruncated: true } }]} />);
  open(); const item = screen.getByRole('listitem');
  expect(item.textContent).toContain('checkout remote branch');
  expect(item.textContent).toContain('Hook rejected checkout');
  expect(item.textContent).toContain('Operation output truncated');
  fireEvent.click(within(item).getByText('Operation details'));
  expect(screen.getByLabelText('Diagnostic output').textContent).toContain('<script>do not execute</script>');
  expect(item.querySelector('script')).toBeNull();
  expect(within(item).getByText('9')).toBeTruthy();
  expect(within(item).getByText('batch-1')).toBeTruthy();
});

it('bounds each filtered history to 30 while retaining older active operations', () => {
  const operations = [operation('old-running', 'running'), ...Array.from({ length: 40 }, (_, index) => operation(`op-${index}`, index === 0 ? 'failed' : 'succeeded'))];
  render(<ActivityDrawer operations={operations} />); open();
  expect(screen.getAllByRole('listitem')).toHaveLength(31);
  expect(screen.getByText('old-running')).toBeTruthy();
  expect(screen.getByText(/Showing 31 of 41/)).toBeTruthy();
  fireEvent.change(screen.getByLabelText('Activity filter'), { target: { value: 'attention' } });
  expect(screen.getAllByRole('listitem')).toHaveLength(1);
  expect(screen.getByText('op-0')).toBeTruthy();
  expect(operations[0].operationId).toBe('old-running');
});

it('updates results without opening, stealing focus, or collapsing expanded details', () => {
  const { rerender } = render(<ActivityDrawer operations={[operation('op', 'running')]} />);
  rerender(<ActivityDrawer operations={[operation('op')]} />);
  expect(screen.getByRole('button', { name: 'Repository activity' }).getAttribute('aria-expanded')).toBe('false');
  open(); fireEvent.click(screen.getByText('Operation details'));
  const details = screen.getByText('Operation details').closest('details');
  const focus = document.activeElement;
  rerender(<ActivityDrawer operations={[operation('op'), operation('next', 'queued')]} />);
  expect(details?.open).toBe(true); expect(document.activeElement).toBe(focus);
  fireEvent.click(screen.getByRole('button', { name: 'Close activity' })); open();
  expect(details?.open).toBe(true);
});

it('defers Escape to dialogs and reports an empty attention filter', () => {
  render(<><ActivityDrawer operations={[operation('ok')]} /><dialog open>Review</dialog></>); open();
  fireEvent.change(screen.getByLabelText('Activity filter'), { target: { value: 'attention' } });
  expect(screen.getByText('No operations need attention.')).toBeTruthy();
  fireEvent.keyDown(screen.getByRole('button', { name: 'Close activity' }), { key: 'Escape' });
  expect(screen.getByRole('region', { name: 'Repository activity details' })).toBeTruthy();
});
