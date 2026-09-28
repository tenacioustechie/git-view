import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { ToolbarPopover } from './ToolbarPopover';
import userEvent from '@testing-library/user-event';

afterEach(cleanup);

it('opens at the last eligible control with ArrowUp and excludes hidden commands', async () => {
  const user = userEvent.setup();
  render(<ToolbarPopover name="Commands" label="Commands">{() => <><button>First</button><div hidden><button>Hidden</button></div><button disabled>Disabled</button><button>Last</button></>}</ToolbarPopover>);
  await user.tab(); await user.keyboard('{ArrowUp}');
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Last' }));
  await user.keyboard('{Home}{ArrowDown}');
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Last' }));
  await user.keyboard('{Escape}');
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Commands' }));
});

it('uses real Tab and Shift+Tab without trapping focus or changing text-field arrows', async () => {
  const user = userEvent.setup();
  render(<><button>Before</button><ToolbarPopover name="Form" label="Form">{() => <><input aria-label="Text" defaultValue="draft" /><button>Last</button></>}</ToolbarPopover><button>After</button></>);
  await user.tab(); await user.tab(); await user.keyboard('{Enter}');
  const text = screen.getByLabelText('Text');
  expect(document.activeElement).toBe(text);
  await user.keyboard('{Home}{ArrowRight}');
  expect(document.activeElement).toBe(text);
  expect((text as HTMLInputElement).selectionStart).toBe(1);
  await user.tab({ shift: true });
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Form' }));
  await user.tab({ shift: true });
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Before' }));
  expect(screen.queryByRole('region', { name: 'Form' })).toBeNull();
  await user.tab(); await user.keyboard('{ArrowDown}'); await user.tab(); await user.tab();
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'After' }));
  expect(screen.queryByRole('region', { name: 'Form' })).toBeNull();
});

it('focuses an empty disclosure and keeps focus visible when clicking non-focusable content outside', () => {
  render(<><ToolbarPopover name="Help" label="Help">{() => <p>Reference text</p>}</ToolbarPopover><p>Outside text</p></>);
  const trigger = screen.getByRole('button', { name: 'Help' });
  fireEvent.click(trigger);
  expect(document.activeElement).toBe(screen.getByRole('region', { name: 'Help' }));
  fireEvent.pointerDown(screen.getByText('Outside text'));
  expect(document.activeElement).toBe(trigger);
  expect(screen.queryByRole('region', { name: 'Help' })).toBeNull();
});

it('defers Escape and focus movement to an open confirmation dialog', () => {
  render(<><ToolbarPopover name="Help" label="Help">{() => <p>Reference</p>}</ToolbarPopover><dialog open><button>Confirm</button></dialog></>);
  fireEvent.click(screen.getByRole('button', { name: 'Help' }));
  act(() => screen.getByRole('button', { name: 'Confirm' }).focus());
  fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
  expect(screen.getByRole('region', { name: 'Help' })).toBeTruthy();
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Confirm' }));
});

it('opens by keyboard, skips disabled controls, and restores trigger focus on Escape', () => {
  render(<ToolbarPopover name="Commands" label="Commands">{() => <><button disabled>Unavailable</button><button>Available</button><button>Last</button></>}</ToolbarPopover>);
  const trigger = screen.getByRole('button', { name: 'Commands' });
  expect(screen.queryByRole('region', { name: 'Commands' })).toBeNull();
  fireEvent.keyDown(trigger, { key: 'ArrowDown' });
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Available' }));
  fireEvent.keyDown(document.activeElement!, { key: 'ArrowDown' });
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Last' }));
  fireEvent.keyDown(document.activeElement!, { key: 'ArrowDown' });
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Available' }));
  fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
  expect(trigger.getAttribute('aria-expanded')).toBe('false');
  expect(document.activeElement).toBe(trigger);
});

it('closes on outside click or leaving with Tab, while retaining form contents', () => {
  render(<><ToolbarPopover name="Push" label="Push">{() => <input aria-label="Destination" defaultValue="main" />}</ToolbarPopover><button>Outside</button></>);
  const trigger = screen.getByRole('button', { name: 'Push' });
  fireEvent.click(trigger);
  fireEvent.change(screen.getByLabelText('Destination'), { target: { value: 'topic' } });
  fireEvent.pointerDown(screen.getByRole('button', { name: 'Outside' }));
  expect(screen.queryByRole('region', { name: 'Push' })).toBeNull();
  fireEvent.click(trigger);
  expect((within(screen.getByRole('region', { name: 'Push' })).getByLabelText('Destination') as HTMLInputElement).value).toBe('topic');
  act(() => screen.getByRole('button', { name: 'Outside' }).focus());
  expect(trigger.getAttribute('aria-expanded')).toBe('false');
});

it('closes a command before invoking it and keeps only the focused disclosure open', () => {
  const action = vi.fn();
  render(<><ToolbarPopover name="First" label="First">{close => <button onClick={() => { close(); action(); }}>Run</button>}</ToolbarPopover>
    <ToolbarPopover name="Second" label="Second">{() => <button>Other</button>}</ToolbarPopover></>);
  fireEvent.click(screen.getByRole('button', { name: 'First' }));
  fireEvent.click(screen.getByRole('button', { name: 'Second' }));
  expect(screen.queryByRole('region', { name: 'First' })).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'First' }));
  expect(screen.queryByRole('region', { name: 'Second' })).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'Run' }));
  expect(action).toHaveBeenCalledTimes(1);
  expect(screen.queryByRole('region', { name: 'First' })).toBeNull();
});
