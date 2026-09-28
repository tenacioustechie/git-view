import { useRef } from 'react';

export function PanelDivider({ name, value, min, max, onChange }: {
  name: string; value: number; min: number; max: number; onChange: (width: number) => void;
}) {
  const drag = useRef<{ id: number; x: number; width: number; original: number } | null>(null);
  const change = (width: number) => onChange(Math.round(Math.max(min, Math.min(max, width))));
  return <div className="panel-divider" role="separator" aria-label={name} aria-orientation="vertical" tabIndex={0}
    aria-valuemin={min} aria-valuemax={max} aria-valuenow={value} aria-valuetext={`${value} pixels preferred width`}
    title={`${name}: drag or use Left/Right arrows; Home/End sets minimum/maximum`}
    onPointerDown={event => {
      if (event.button !== 0 || !event.isPrimary) return;
      event.preventDefault(); event.currentTarget.focus();
      drag.current = { id: event.pointerId, x: event.clientX, width: event.currentTarget.previousElementSibling?.getBoundingClientRect().width || value, original: value };
      event.currentTarget.setPointerCapture(event.pointerId);
    }}
    onPointerMove={event => {
      if (drag.current?.id === event.pointerId) change(drag.current.width + event.clientX - drag.current.x);
    }}
    onPointerUp={event => { if (drag.current?.id === event.pointerId) { drag.current = null; event.currentTarget.releasePointerCapture(event.pointerId); } }}
    onPointerCancel={() => { if (drag.current) { change(drag.current.original); drag.current = null; } }}
    onLostPointerCapture={() => { drag.current = null; }}
    onKeyDown={event => {
      if (event.key === 'Escape' && drag.current) {
        const previous = drag.current; drag.current = null;
        change(previous.original); event.currentTarget.releasePointerCapture(previous.id); event.preventDefault();
      }
      if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
      event.preventDefault();
      change(event.key === 'Home' ? min : event.key === 'End' ? max : value + (event.key === 'ArrowRight' ? 10 : -10));
    }} />;
}
