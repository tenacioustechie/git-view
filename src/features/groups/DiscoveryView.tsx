import { useEffect, useRef, useState, type ReactNode } from 'react';
import { open, save } from '@tauri-apps/plugin-dialog';
import { api, errorMessage } from '../../api/client';
import { withNavigationBlock } from '../../api/navigation';
import type { DiscoveryCandidate, DiscoveryResult, Project } from '../../api/types';
import './groups.css';

interface DiscoveryProps {
  initialRoot?: string;
  initialDepth?: number;
  onProjectCreated: (project: Project) => void;
  onCancel: () => void;
}

export function DiscoveryView({ initialRoot, initialDepth, onProjectCreated, onCancel }: DiscoveryProps) {
  const [name, setName] = useState('Discovered repositories');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const saveInFlight = useRef(false);
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  async function create(candidates: DiscoveryCandidate[]) {
    if (saveInFlight.current || !name.trim()) return;
    saveInFlight.current = true; setSaving(true); setError(null);
    const content = { version: 1 as const, name: name.trim(), repositories: candidates.map(candidate => ({ path: candidate.displayPath, label: candidate.label })) };
    try {
      const path = await withNavigationBlock(() => save({ title: 'Save discovered project', defaultPath: 'discovered.git-view.json', filters: [{ name: 'Git View project', extensions: ['json'] }] }));
      if (!path || !mounted.current) return;
      const project = await api.createProject(path, content);
      if (mounted.current) onProjectCreated(project);
    } catch (cause) { if (mounted.current) setError(errorMessage(cause)); }
    finally { saveInFlight.current = false; if (mounted.current) setSaving(false); }
  }
  return <section className="group-view" aria-label="Create project from discovery">
    <h1>Discover repositories</h1>
    <p>Scan a folder, review the results, then choose which repositories to include in a project.</p>
    {error && <p role="alert" className="group-notice">{error}</p>}
    <DiscoveryPicker initialRoot={initialRoot} initialDepth={initialDepth} onConfirm={candidates => void create(candidates)} onCancel={onCancel} confirmLabel="Save selected as project" disabled={saving} confirmDisabled={!name.trim()}
      beforeActions={<label>Project name <input disabled={saving} value={name} onChange={event => setName(event.target.value)} /></label>} />
    {saving && <p role="status">Saving project…</p>}
  </section>;
}

interface PickerProps {
  initialRoot?: string;
  initialDepth?: number;
  onConfirm: (candidates: DiscoveryCandidate[]) => void;
  onCancel: () => void;
  confirmLabel: string;
  excludedRepositoryIds?: ReadonlySet<string>;
  excludedPaths?: ReadonlySet<string>;
  disabled?: boolean;
  confirmDisabled?: boolean;
  beforeActions?: ReactNode;
}

export function DiscoveryPicker({ initialRoot = '', initialDepth = 3, onConfirm, onCancel, confirmLabel, excludedRepositoryIds, excludedPaths, disabled = false, confirmDisabled = false, beforeActions }: PickerProps) {
  const [root, setRoot] = useState(initialRoot);
  const [depth, setDepth] = useState(String(initialDepth));
  const [result, setResult] = useState<DiscoveryResult | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [scanning, setScanning] = useState(false);
  const [picking, setPicking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const generation = useRef(0);
  const inFlight = useRef(false);
  const pickerInFlight = useRef(false);
  const mounted = useRef(true);
  const lastRoot = useRef<string | null>(null);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; generation.current++; }; }, []);
  useEffect(() => { generation.current++; setRoot(initialRoot); setDepth(String(initialDepth)); setResult(null); setSelected(new Set()); lastRoot.current = null; }, [initialRoot, initialDepth]);
  const validDepth = /^\d+$/.test(depth) && Number(depth) <= 10;
  const excluded = (candidate: DiscoveryCandidate) => Boolean(excludedRepositoryIds?.has(candidate.repositoryId) || excludedPaths?.has(candidate.displayPath));
  const available = result?.candidates.filter(candidate => !excluded(candidate)) ?? [];
  const chosen = available.filter(candidate => selected.has(candidate.repositoryId));
  function changeRoot(value: string) {
    generation.current++; setRoot(value); setResult(null); setSelected(new Set()); lastRoot.current = null; setError(null);
  }
  function changeDepth(value: string) { generation.current++; setDepth(value); setResult(null); setError(null); }
  async function pickRoot() {
    if (pickerInFlight.current || disabled) return;
    pickerInFlight.current = true; setPicking(true);
    const request = generation.current;
    try {
      const path = await withNavigationBlock(() => open({ directory: true, multiple: false, title: 'Choose discovery root' }));
      if (mounted.current && request === generation.current && typeof path === 'string') changeRoot(path);
    } catch (cause) { if (mounted.current && request === generation.current) setError(errorMessage(cause)); }
    finally { pickerInFlight.current = false; if (mounted.current) setPicking(false); }
  }
  async function scan() {
    if (inFlight.current || disabled || !root.trim() || !validDepth) return;
    inFlight.current = true; setScanning(true); setError(null);
    const request = ++generation.current;
    try {
      const next = await api.discoverRepositories(root.trim(), Number(depth));
      if (!mounted.current || request !== generation.current) return;
      const sameRoot = lastRoot.current === null || lastRoot.current === next.rootPath;
      setSelected(current => sameRoot ? current : new Set());
      lastRoot.current = next.rootPath; setResult(next);
    } catch (cause) {
      if (mounted.current && request === generation.current) { setResult(null); setError(errorMessage(cause)); }
    } finally { inFlight.current = false; if (mounted.current) setScanning(false); }
  }
  return <section className="group-discovery" aria-label="Repository discovery">
    <div className="group-filters">
      <label>Root folder <input value={root} disabled={disabled} onChange={event => changeRoot(event.target.value)} placeholder="Folder to search" /></label>
      <button disabled={disabled || picking} onClick={() => void pickRoot()}>Choose folder</button>
      <label>Search depth <input type="number" min="0" max="10" step="1" value={depth} disabled={disabled} onChange={event => changeDepth(event.target.value)} /></label>
      <button disabled={disabled || scanning || picking || !root.trim() || !validDepth} onClick={() => void scan()}>{result ? 'Rescan' : 'Scan repositories'}</button>
    </div>
    <p className="group-hint">Depth 0 checks the root folder only. The maximum depth is 10. Newly found repositories are unchecked so you can review additions.</p>
    {!validDepth && <p role="alert">Enter a whole-number depth from 0 to 10.</p>}
    {scanning && <p role="status">Scanning repositories… You can cancel this review; a running scan may finish in the background.</p>}
    {error && <p role="alert" className="group-notice">Scan failed: {error}</p>}
    {result && <>
      <p>{result.candidates.length} repositories found in {result.rootPath}; {result.visitedDirectories} folders visited, depth {result.depth}.</p>
      {result.truncated && <p role="alert" className="group-notice">Partial results: the scan reached its limit. Scan a smaller folder to find the remaining repositories.</p>}
      {result.issues.length > 0 && <div className="group-notice" role="status"><p>Some folders could not be scanned. These results may be incomplete.</p><ul>{result.issues.map((issue, index) => <li key={`${issue.path}:${index}`}><span>{issue.path}</span>: {issue.reason}</li>)}</ul></div>}
      {result.candidates.length ? <div className="group-table-scroll"><table className="group-table"><caption>Discovered repositories</caption>
        <thead><tr><th><input type="checkbox" aria-label="Select all discovered repositories" disabled={disabled || scanning || !available.length} checked={available.length > 0 && chosen.length === available.length} onChange={event => { const checked = event.target.checked; setSelected(current => { const next = new Set(current); for (const candidate of available) { if (checked) next.add(candidate.repositoryId); else next.delete(candidate.repositoryId); } return next; }); }} /></th><th>Repository</th><th>Path</th><th>Review</th></tr></thead>
        <tbody>{result.candidates.map(candidate => <tr key={candidate.repositoryId}><td><input type="checkbox" aria-label={`Include ${candidate.label}`} aria-describedby={`discovered-path-${candidate.repositoryId}`} disabled={disabled || scanning || excluded(candidate)} checked={!excluded(candidate) && selected.has(candidate.repositoryId)} onChange={event => { const checked = event.target.checked; setSelected(current => { const next = new Set(current); if (checked) next.add(candidate.repositoryId); else next.delete(candidate.repositoryId); return next; }); }} /></td><td>{candidate.label}</td><td className="discovery-path" id={`discovered-path-${candidate.repositoryId}`}>{candidate.displayPath}</td><td>{excluded(candidate) ? 'Already in project or draft' : 'Available'}</td></tr>)}</tbody>
      </table></div> : <p>No repositories were found in this scan.</p>}
      <p>{chosen.length} selected for {confirmLabel === 'Add selected to draft' ? 'addition' : 'the project'}.</p>
    </>}
    <div className="group-toolbar">{beforeActions}<button disabled={disabled || scanning || picking || confirmDisabled || !chosen.length} onClick={() => onConfirm(chosen)}>{confirmLabel} ({chosen.length})</button><button disabled={disabled} onClick={onCancel}>Cancel discovery</button></div>
  </section>;
}
