import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { ToolbarPopover } from './ToolbarPopover';

afterEach(cleanup);

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
