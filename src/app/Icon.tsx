const paths = {
  chevron: 'm6 9 6 6 6-6',
  refresh: 'M20 7v5h-5 M4 17v-5h5 M6.1 7a7 7 0 0 1 11.6-2L20 8 M4 16l2.3 3A7 7 0 0 0 18 17',
  branch: 'M6 5v10a4 4 0 0 0 4 4h2 M6 11h8a4 4 0 0 0 4-4V5 M4 3h4v4H4z M16 3h4v4h-4z M12 17h4v4h-4z',
  fetch: 'M8 4v15m-4-4 4 4 4-4 M16 20V5m-4 4 4-4 4 4',
  pull: 'M12 3v12m-5-5 5 5 5-5 M5 17v4h14v-4',
  push: 'M12 16V4m-5 5 5-5 5 5 M5 17v4h14v-4',
  more: 'M5 11h1v1H5z M11 11h1v1h-1z M17 11h1v1h-1z',
} as const;
export function Icon({ name }: { name: keyof typeof paths }) {
  return <svg className="ui-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={paths[name]} /></svg>;
}
