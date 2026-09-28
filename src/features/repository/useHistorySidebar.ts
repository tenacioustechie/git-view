import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState, useSyncExternalStore } from 'react';

const query = '(max-width: 1000px)';
const snapshot = () => window.matchMedia?.(query).matches ?? false;
function subscribe(notify: () => void) {
  const media = window.matchMedia?.(query);
  media?.addEventListener('change', notify);
  return () => media?.removeEventListener('change', notify);
}

// Keep the same sidebar mounted at both widths so form inputs never reset.
export function useHistorySidebar(active: boolean) {
  const beforeResize = useRef<Element | null>(null);
  const subscribeWithFocus = useCallback((notify: () => void) => subscribe(() => {
    beforeResize.current = document.activeElement;
    notify();
  }), []);
  const compact = useSyncExternalStore(subscribeWithFocus, snapshot);
  const [open, setOpen] = useState(false);
  const panel = useRef<HTMLElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const id = useId();
  const expanded = compact && open && active;
  const close = useCallback(() => {
    setOpen(false);
    if (compact && active) trigger.current?.focus();
  }, [compact, active]);

  useLayoutEffect(() => {
    const focused = beforeResize.current ?? document.activeElement;
    beforeResize.current = null;
    if (!active) return;
    if (compact && !expanded && (panel.current?.contains(focused) || focused?.matches('[role="separator"][aria-label="Resize branches panel"]'))) trigger.current?.focus();
    if (!compact && (focused === closeButton.current || focused === trigger.current)) panel.current?.querySelector<HTMLButtonElement>('button:not([hidden])')?.focus();
  }, [compact, expanded, active]);

  useEffect(() => {
    if (!expanded) return;
    panel.current?.querySelector<HTMLButtonElement>('button')?.focus();
    const outside = (event: Event) => {
      if (document.querySelector('dialog[open]') || !(event.target instanceof Node)
        || panel.current?.contains(event.target) || trigger.current?.contains(event.target)) return;
      setOpen(false);
      if (panel.current?.contains(document.activeElement)) trigger.current?.focus();
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || document.querySelector('dialog[open]')) return;
      event.preventDefault(); close();
    };
    document.addEventListener('pointerdown', outside);
    document.addEventListener('focusin', outside);
    document.addEventListener('keydown', escape);
    return () => {
      document.removeEventListener('pointerdown', outside);
      document.removeEventListener('focusin', outside);
      document.removeEventListener('keydown', escape);
    };
  }, [expanded, close]);

  return { compact, expanded, panel, trigger, closeButton, id, close, reveal: () => setOpen(true), toggle: () => setOpen(value => !value) };
}
