import { useEffect, useId, useRef, useState, type ReactNode } from 'react';

// A disclosure with ordinary controls: Tab works in both command lists and forms.
// Keeping the contents mounted preserves form drafts while the disclosure closes.
export function ToolbarPopover({ label, name, title = name, children, className = '', align = 'right', disabled = false }: {
  label: ReactNode; name: string; children: (close: () => void) => ReactNode;
  className?: string; align?: 'left' | 'right'; disabled?: boolean; title?: string;
}) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const id = useId();
  const openAtEnd = useRef(false);
  const controls = () => [...(panel.current?.querySelectorAll<HTMLElement>('button, input:not([type="hidden"]), select, textarea, a[href], [tabindex]') ?? [])]
    .filter(node => !node.matches(':disabled') && node.tabIndex >= 0 && !node.closest('[hidden], [inert]') && !node.closest('details:not([open])')
      && getComputedStyle(node).display !== 'none' && getComputedStyle(node).visibility !== 'hidden');
  const focusContent = () => {
    const items = controls();
    (openAtEnd.current ? items.at(-1) : items[0] ?? panel.current)?.focus();
    if (!items.length) panel.current?.focus();
  };
  const close = () => { setOpen(false); trigger.current?.focus(); };
  useEffect(() => {
    if (!open) return;
    focusContent();
    const outside = (event: Event) => {
      if (document.querySelector('dialog[open]')) return;
      if (event.target instanceof Node && !root.current?.contains(event.target)) {
        if (panel.current?.contains(document.activeElement)) trigger.current?.focus();
        setOpen(false);
      }
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || document.querySelector('dialog[open]')) return;
      event.preventDefault(); event.stopPropagation(); setOpen(false); trigger.current?.focus();
    };
    document.addEventListener('pointerdown', outside);
    document.addEventListener('focusin', outside);
    document.addEventListener('keydown', escape, true);
    return () => {
      document.removeEventListener('pointerdown', outside);
      document.removeEventListener('focusin', outside);
      document.removeEventListener('keydown', escape, true);
    };
  }, [open]);
  return <div className={`toolbar-popover ${className}`} ref={root}>
    <button className="toolbar-popover-trigger" ref={trigger} type="button" disabled={disabled} aria-label={name} title={title} aria-expanded={open} aria-controls={id}
      onClick={() => { openAtEnd.current = false; setOpen(value => !value); }} onKeyDown={event => {
        if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey || event.nativeEvent.isComposing) return;
        if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
          event.preventDefault(); openAtEnd.current = event.key === 'ArrowUp';
          if (open) focusContent(); else setOpen(true);
        }
      }}>{label}</button>
    <div id={id} className={`toolbar-popover-panel align-${align}`} ref={panel} hidden={!open} role="region" aria-label={name} tabIndex={-1} onKeyDown={event => {
      if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey || event.nativeEvent.isComposing) return;
      if (!(event.target instanceof HTMLButtonElement) || !['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
      const items = controls().filter(node => node instanceof HTMLButtonElement);
      const current = items.indexOf(event.target);
      if (current < 0) return;
      event.preventDefault();
      const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : (current + (event.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length;
      items[next].focus();
    }}>{children(close)}</div>
  </div>;
}
