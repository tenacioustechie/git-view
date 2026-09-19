import { useRef } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import type { GraphRow } from './layout';

const rowHeight = 44;
const colors = ['#68aaf9', '#a994f5', '#58bfa7', '#e6ad69', '#e584a2'];
export function HistoryGraph({ rows, selected, onSelect, loading, more, onMore }: {
  rows: GraphRow[]; selected: string | null; onSelect: (oid: string) => void;
  loading: boolean; more: boolean; onMore: () => void;
}) {
  const scroll = useRef<HTMLDivElement>(null);
  const virtual = useVirtualizer({ count: rows.length, getScrollElement: () => scroll.current, estimateSize: () => rowHeight, overscan: 12, initialRect: { width: 640, height: 600 } });
  const width = Math.max(60, ...rows.map(row => row.width * 18 + 20));
  return <div className="history-list" ref={scroll} aria-label="Commit history" tabIndex={0}>
    <div style={{ height: virtual.getTotalSize(), position: 'relative', minWidth: width + 240 }}>
      {virtual.getVirtualItems().map(item => {
        const row = rows[item.index];
        return <button type="button" key={row.commit.oid} className={`history-row ${selected === row.commit.oid ? 'selected' : ''}`}
          style={{ position: 'absolute', top: item.start, height: rowHeight }} onClick={() => onSelect(row.commit.oid)}
          aria-pressed={selected === row.commit.oid} title={`${row.commit.oid}\n${row.commit.subject}\n${row.commit.authorName}`}>
          <svg width={width} height={rowHeight} aria-hidden="true" style={{ flexShrink: 0 }}>
            {row.segments.map((segment, i) => {
              const x1 = 14 + segment.from * 18, x2 = 14 + segment.to * 18;
              const y1 = segment.part === 'parent' ? 22 : 0, y2 = segment.part === 'incoming' ? 22 : 44;
              return <path key={i} d={`M ${x1} ${y1} C ${x1} ${(y1 + y2) / 2}, ${x2} ${(y1 + y2) / 2}, ${x2} ${y2}`}
                fill="none" stroke={colors[segment.to % colors.length]} strokeWidth="2" strokeDasharray={row.commit.boundary && segment.part === 'parent' ? '3 3' : undefined} />;
            })}
            <circle cx={14 + row.lane * 18} cy={22} r={4.5} fill={row.commit.boundary ? 'var(--panel)' : colors[row.lane % colors.length]} stroke={colors[row.lane % colors.length]} strokeWidth="2" />
          </svg>
          <span className="history-description"><span>{row.commit.refs.map(ref => <span className="ref-badge" key={ref.fullName}>{ref.label}</span>)}{row.commit.subject || '(no subject)'}</span>
            <small>{row.commit.authorName} · {row.commit.oid.slice(0, 8)}{row.commit.boundary ? ' · shallow boundary' : ''}</small></span>
        </button>;
      })}
    </div>
    {more && <button className="load-more" disabled={loading} onClick={onMore}>{loading ? 'Loading…' : 'Load older commits'}</button>}
    {!rows.length && !loading && <p className="empty">No commits in this scope.</p>}
  </div>;
}
