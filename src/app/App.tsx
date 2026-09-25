import { useCallback, useEffect, useRef, useState } from 'react';
import { open, save } from '@tauri-apps/plugin-dialog';
import { api, errorMessage } from '../api/client';
import { navigationBlocked, withNavigationBlock } from '../api/navigation';
import type { Project, RepositoryOpen } from '../api/types';
import { GroupView } from '../features/groups/GroupView';
import { DiscoveryView } from '../features/groups/DiscoveryView';
import { RepositoryView } from '../features/repository/RepositoryView';
import { ToolbarPopover } from './ToolbarPopover';
import { Icon } from './Icon';
import { useRecentWorkspaces, type RecentWorkspace } from './useRecentWorkspaces';
import './app.css';

type DiscoveryRoute = { key: string; root?: string; depth?: number };

export function App() {
  const [project, setProject] = useState<Project | null>(null);
  const [repository, setRepository] = useState<RepositoryOpen | null>(null);
  const [discovery, setDiscovery] = useState<DiscoveryRoute | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [projectName, setProjectName] = useState('');
  const opening = useRef(false);
  const navigationVersion = useRef(0);
  const { items: recent, warning: recentWarning, remember, clear: clearRecent } = useRecentWorkspaces();
  const acceptRepository = useCallback((repo: RepositoryOpen) => {
    navigationVersion.current++;
    setRepository(repo); setDiscovery(null); setError(null);
    remember({ kind: 'repository', path: repo.displayPath, name: repo.displayPath.split(/[\\/]/).filter(Boolean).pop() || repo.displayPath });
  }, [remember]);
  const acceptProject = useCallback((next: Project) => {
    navigationVersion.current++;
    setProject(next); setRepository(null); setDiscovery(null); setError(null);
    remember({ kind: 'project', path: next.displayPath, name: next.name });
  }, [remember]);
  function projectChanged(next: Project) {
    remember({ kind: 'project', path: next.displayPath, name: next.name }, next.displayPath === project?.displayPath);
    setProject(next);
  }
  async function reopen(item: RecentWorkspace) {
    if (busy || navigationBlocked() || document.querySelector('dialog[open]')) return;
    const version = ++navigationVersion.current;
    setBusy(true); setError(null);
    const current = () => navigationVersion.current === version && !navigationBlocked() && !document.querySelector('dialog[open]');
    try {
      if (item.kind === 'repository') {
        const next = await withNavigationBlock(() => api.openRepository(item.path));
        if (current()) acceptRepository(next);
      } else {
        const next = await withNavigationBlock(() => api.openProject(item.path));
        if (current()) acceptProject(next);
      }
    } catch (cause) {
      if (current()) setError(`Could not reopen ${item.name}: ${errorMessage(cause)}. Use Workspace to locate it if it has moved.`);
    } finally { setBusy(false); }
  }
  function back() {
    navigationVersion.current++;
    setRepository(null); setDiscovery(null);
  }
  async function perform(task: () => Promise<void>) {
    navigationVersion.current++;
    setBusy(true); setError(null);
    try { await withNavigationBlock(task); }
    catch (e) { setError(errorMessage(e)); }
    finally { setBusy(false); }
  }
  function discover() {
    void perform(async () => {
      const root = await open({ directory: true, multiple: false, title: 'Discover repositories' });
      if (typeof root === 'string') setDiscovery({ key: crypto.randomUUID(), root, depth: 3 });
    });
  }

  useEffect(() => {
    let disposed = false;
    const blocked = () => navigationBlocked() || document.querySelector('dialog[open]') !== null;
    const intake = async () => {
      if (opening.current || blocked()) return;
      opening.current = true;
      const version = navigationVersion.current;
      const current = () => !disposed && !blocked() && version === navigationVersion.current;
      try {
        const request = await api.getLaunchRequest();
        if (!request || !current()) return;
        try {
          if (request.error) throw request.error;
          if (request.kind === 'repository' && request.path) {
            const next = await api.openRepository(request.path);
            if (!current()) return;
            acceptRepository(next);
          } else if (request.kind === 'project' && request.path) {
            const next = await api.openProject(request.path);
            if (!current()) return;
            acceptProject(next);
          } else if (request.kind === 'discovery' && request.path) {
            setDiscovery({ key: request.requestId, root: request.path, depth: request.scanDepth ?? 3 });
            setError(null);
          } else if (request.kind === 'welcome') {
            setProject(null); setRepository(null); setDiscovery(null); setError(null);
          }
        } catch (e) {
          if (!current()) return;
          setError(errorMessage(e));
        }
        await api.ackLaunchRequest(request.requestId);
      } catch (e) { if (!disposed) setError(errorMessage(e)); }
      finally { opening.current = false; }
    };
    void intake();
    const timer = window.setInterval(() => void intake(), 500);
    return () => { disposed = true; window.clearInterval(timer); };
  }, [acceptProject, acceptRepository]);

  return <div className="application">
    <header className="app-header">
      <a href="#" onClick={e => { e.preventDefault(); back(); }} className="brand">Git View</a>
      <nav aria-label="Open workspace">
        <ToolbarPopover name="Workspace" label={<>Workspace <Icon name="chevron" /></>} align="left" disabled={busy}>{close => <>
        <p className="popover-caption">Open workspace</p>
        <button disabled={busy} onClick={() => { close(); void perform(async () => {
          const path = await open({ directory: true, multiple: false, title: 'Open repository' });
          if (typeof path === 'string') acceptRepository(await api.openRepository(path));
        }); }}>Open repository</button>
        <button disabled={busy} onClick={() => { close(); void perform(async () => {
          const path = await open({ multiple: false, title: 'Open project', filters: [{ name: 'Git View project', extensions: ['json'] }] });
          if (typeof path === 'string') acceptProject(await api.openProject(path));
        }); }}>Open project</button>
        <div className="popover-separator" />
        <button disabled={busy} onClick={() => { close(); discover(); }}>Discover repositories</button>
        </>}</ToolbarPopover>
        <ToolbarPopover name="Recent" label={<>Recent <Icon name="chevron" /></>} align="left" className="recent-workspaces" disabled={busy}>{close => <>
          <p className="popover-caption">Recently opened</p>
          {recentWarning && <p className="recent-notice" role="status">{recentWarning}</p>}
          {!recent.length && <p className="recent-empty">No recent workspaces yet.<br />Open a repository or project to get started.</p>}
          {recent.map(item => <button key={JSON.stringify([item.kind, item.path])} className="recent-item" title={item.path} aria-label={`Open ${item.kind} ${item.name}: ${item.path}`} disabled={busy} onClick={() => { close(); void reopen(item); }}>
            <span className="recent-item-heading"><strong>{item.name}</strong><span className="recent-kind">{item.kind === 'project' ? 'Project' : 'Repository'}</span></span>
            <span className="recent-path">{item.path}</span>
          </button>)}
          {recent.length > 0 && <><div className="popover-separator" /><button title="Forget this list only; no projects or repository files are deleted" onClick={() => { close(); clearRecent(); }}>Clear recent</button></>}
        </>}</ToolbarPopover>
      </nav>
      {project && repository && <button className="project-back" onClick={back}>Back to {project.name}</button>}
    </header>
    {error && <div className="app-error" role="alert">{error}<button aria-label="Dismiss error" onClick={() => setError(null)}>×</button></div>}
    {project && <div className="group-container" hidden={repository !== null || discovery !== null}>
      <GroupView key={project.projectId} project={project} onOpenRepository={acceptRepository} onProjectChange={projectChanged} />
    </div>}
    {repository && !discovery && <RepositoryView repository={repository} />}
    {discovery && <DiscoveryView key={discovery.key} initialRoot={discovery.root} initialDepth={discovery.depth} onProjectCreated={acceptProject}
      onCancel={() => { navigationVersion.current++; setDiscovery(null); }} />}
    {!project && !repository && !discovery && <main className="welcome">
      <div className="welcome-mark" aria-hidden="true">⑂</div><h1>Your repositories, together.</h1>
      <p>Open a repository to inspect history and commit changes, or open a JSON project to check and pull a group.</p>
      <form onSubmit={e => {
        e.preventDefault(); void perform(async () => {
          const path = await save({ title: 'Create project', defaultPath: 'work.git-view.json', filters: [{ name: 'Git View project', extensions: ['json'] }] });
          if (path) acceptProject(await api.createProject(path, { version: 1, name: projectName.trim(), repositories: [] }));
        });
      }}>
        <label>New project name<input value={projectName} onChange={e => setProjectName(e.target.value)} placeholder="Work" /></label>
        <button disabled={busy || !projectName.trim()}>Create project</button>
      </form>
      <p className="hint">CLI: <code>git-view repo .</code> · <code>git-view group work.git-view.json</code> · <code>git-view group --scan ./code</code></p>
    </main>}
  </div>;
}
