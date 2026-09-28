import { useState } from 'react';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { PanelDivider } from './PanelDivider';

function Harness() {
  const [width, setWidth] = useState(350);
  return <><aside>Panel</aside><PanelDivider name="Resize test panel" value={width} min={280} max={480} onChange={setWidth} /></>;
}
function pointer(node: Element, type: string, x: number, id = 1, button = 0) {
  const event = new Event(type, { bubbles: true });
  Object.assign(event, { clientX: x, pointerId: id, button, isPrimary: true });
  fireEvent(node, event);
}
beforeEach(() => {
  HTMLElement.prototype.setPointerCapture = vi.fn();
  HTMLElement.prototype.releasePointerCapture = vi.fn();
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
describe('panel divider', () => {
  it('supports accessible keyboard increments and min/max limits', () => {
    render(<Harness />);
    const divider = screen.getByRole('separator', { name: 'Resize test panel' });
    fireEvent.keyDown(divider, { key: 'ArrowRight' });
    expect(divider.getAttribute('aria-valuenow')).toBe('360');
    fireEvent.keyDown(divider, { key: 'Home' });
    fireEvent.keyDown(divider, { key: 'ArrowLeft' });
    expect(divider.getAttribute('aria-valuenow')).toBe('280');
    fireEvent.keyDown(divider, { key: 'End' });
    fireEvent.keyDown(divider, { key: 'ArrowRight' });
    expect(divider.getAttribute('aria-valuenow')).toBe('480');
  });
  it('drags from rendered size, clamps, captures the pointer, and stops after release', () => {
    render(<Harness />);
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({ width: 300 } as DOMRect);
    const divider = screen.getByRole('separator');
    pointer(divider, 'pointerdown', 300);
    expect(document.activeElement).toBe(divider);
    expect(divider.setPointerCapture).toHaveBeenCalledWith(1);
    pointer(divider, 'pointermove', 330, 2);
    expect(divider.getAttribute('aria-valuenow')).toBe('350');
    pointer(divider, 'pointermove', 380);
    expect(divider.getAttribute('aria-valuenow')).toBe('380');
    pointer(divider, 'pointermove', 1000);
    expect(divider.getAttribute('aria-valuenow')).toBe('480');
    pointer(divider, 'pointerup', 1000);
    pointer(divider, 'pointermove', 300);
    expect(divider.getAttribute('aria-valuenow')).toBe('480');
  });
  it('restores the preferred size on cancellation or Escape and ignores non-primary buttons', () => {
    render(<Harness />);
    const divider = screen.getByRole('separator');
    pointer(divider, 'pointerdown', 100, 1, 2);
    pointer(divider, 'pointermove', 180);
    expect(divider.getAttribute('aria-valuenow')).toBe('350');
    pointer(divider, 'pointerdown', 100);
    pointer(divider, 'pointermove', 180);
    pointer(divider, 'pointercancel', 180);
    expect(divider.getAttribute('aria-valuenow')).toBe('350');
    pointer(divider, 'pointerdown', 100);
    pointer(divider, 'pointermove', 180);
    fireEvent.keyDown(divider, { key: 'Escape' });
    expect(divider.getAttribute('aria-valuenow')).toBe('350');
  });
});
