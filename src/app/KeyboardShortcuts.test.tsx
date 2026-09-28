import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { KeyboardShortcuts } from './KeyboardShortcuts';
import { primaryShortcut, shortcutAria, shortcutLabel } from './shortcuts';

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

it.each([['MacIntel', '⌘F', 'Meta+F'], ['Win32', 'Ctrl+F', 'Control+F']])('shows platform hints for %s', (platform, label, aria) => {
  vi.spyOn(navigator, 'platform', 'get').mockReturnValue(platform);
  expect(shortcutLabel('f')).toBe(label);
  expect(shortcutAria('f')).toBe(aria);
});

it('opens, reads and exits the shortcut reference using keyboard only', async () => {
  const user = userEvent.setup();
  render(<><KeyboardShortcuts /><button>Next</button></>);
  await user.tab(); await user.keyboard('{Enter}');
  expect(document.activeElement).toBe(screen.getByRole('region', { name: 'Keyboard shortcuts' }));
  expect(screen.getByText(/Does not fetch or pull/)).toBeTruthy();
  await user.keyboard('{Escape}');
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Keyboard shortcuts' }));
  await user.keyboard('{ArrowDown}'); await user.tab();
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Next' }));
  expect(screen.queryByRole('region', { name: 'Keyboard shortcuts' })).toBeNull();
});

it('matches only plain primary shortcuts and respects IME and handled events', () => {
  expect(primaryShortcut(new KeyboardEvent('keydown', { key: 'f', ctrlKey: true }), 'f')).toBe(true);
  expect(primaryShortcut(new KeyboardEvent('keydown', { key: 'F', metaKey: true }), 'f')).toBe(true);
  for (const extra of [{ shiftKey: true }, { altKey: true }, { isComposing: true }]) {
    expect(primaryShortcut(new KeyboardEvent('keydown', { key: 'f', ctrlKey: true, ...extra }), 'f')).toBe(false);
  }
  const handled = new KeyboardEvent('keydown', { key: 'f', ctrlKey: true, cancelable: true });
  handled.preventDefault(); expect(primaryShortcut(handled, 'f')).toBe(false);
});
