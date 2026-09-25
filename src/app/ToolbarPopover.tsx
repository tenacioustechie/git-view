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
  const close = () => { setOpen(false); trigger.current?.focus(); };
  useEffect(() => {
    if (!open) return;
    panel.current?.querySelector<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled), a[href]')?.focus();
    const outside = (event: Event) => {
      if (event.target instanceof Node && !root.current?.contains(event.target)) setOpen(false);
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
      onClick={() => setOpen(value => !value)} onKeyDown={event => {
        if (event.key === 'ArrowDown') { event.preventDefault(); setOpen(true); }
      }}>{label}</button>
    <div id={id} className={`toolbar-popover-panel align-${align}`} ref={panel} hidden={!open} role="region" aria-label={name} onKeyDown={event => {
      if (!(event.target instanceof HTMLButtonElement) || !['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
      const items = [...(panel.current?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])];
      const current = items.indexOf(event.target);
      if (current < 0) return;
      event.preventDefault();
      const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : (current + (event.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length;
      items[next].focus();
    }}>{children(close)}</div>
  </div>;
}
