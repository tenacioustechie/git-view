import { useCallback, useEffect, useRef, useState } from 'react';
import { open, save } from '@tauri-apps/plugin-dialog';
import { api, errorMessage } from '../api/client';
import type { Project, RepositoryOpen } from '../api/types';
import { GroupView } from '../features/groups/GroupView';
import { RepositoryView } from '../features/repository/RepositoryView';
import './app.css';

export function App() {
  const [project,setProject]=useState<Project|null>(null);
  const [repository,setRepository]=useState<RepositoryOpen|null>(null);
  const [error,setError]=useState<string|null>(null);
  const [busy,setBusy]=useState(false);
  const [projectName,setProjectName]=useState('');
  const opening=useRef(false);
  const dialogPending=useRef(false);
  const acceptRepository=useCallback((repo:RepositoryOpen)=>{setRepository(repo);setError(null);},[]);
  async function perform(task:()=>Promise<void>) {dialogPending.current=true;setBusy(true);setError(null);try {await task();} catch(e) {setError(errorMessage(e));} finally {dialogPending.current=false;setBusy(false);}}
  useEffect(()=>{
    let disposed=false;
    const intake=async()=>{
      if(opening.current || dialogPending.current || document.querySelector('dialog[open]')) return;
      opening.current=true;
      try {
        const request=await api.getLaunchRequest();
        if(!request||disposed||document.querySelector('dialog[open]'))return;
        try {
          if(request.error)throw request.error;
          if(request.kind==='repository'&&request.path) {const next=await api.openRepository(request.path);if(document.querySelector('dialog[open]'))return;acceptRepository(next);}
          else if(request.kind==='project'&&request.path) {const next=await api.openProject(request.path);if(document.querySelector('dialog[open]'))return;setProject(next);setRepository(null);setError(null);}
          else if(request.kind==='welcome'){setProject(null);setRepository(null);}
        } catch(e) {setError(errorMessage(e));}
        await api.ackLaunchRequest(request.requestId);
      } catch(e) {if(!disposed)setError(errorMessage(e));}
      finally {opening.current=false;}
    };
    void intake();const timer=window.setInterval(()=>void intake(),500);
    return()=>{disposed=true;window.clearInterval(timer);};
  },[acceptRepository]);
  return <div className="application">
    <header className="app-header"><a href="#" onClick={e=>{e.preventDefault();setRepository(null);}} className="brand">Git View</a><nav aria-label="Open workspace">
      <button disabled={busy} onClick={()=>void perform(async()=>{const path=await open({directory:true,multiple:false,title:'Open repository'});if(typeof path==='string')acceptRepository(await api.openRepository(path));})}>Open repository</button>
      <button disabled={busy} onClick={()=>void perform(async()=>{const path=await open({multiple:false,title:'Open project',filters:[{name:'Git View project',extensions:['json']}]});if(typeof path==='string'){setProject(await api.openProject(path));setRepository(null);}})}>Open project</button>
      {project&&repository&&<button onClick={()=>setRepository(null)}>Back to {project.name}</button>}
    </nav></header>
    {error&&<div className="app-error" role="alert">{error}<button aria-label="Dismiss error" onClick={()=>setError(null)}>×</button></div>}
    {project&&<div className="group-container" hidden={repository!==null}><GroupView key={project.projectId} project={project} onOpenRepository={acceptRepository} onProjectChange={setProject}/></div>}
    {repository&&<RepositoryView repository={repository} onBack={project?()=>setRepository(null):undefined}/>}
    {!project&&!repository&&<main className="welcome"><div className="welcome-mark" aria-hidden="true">⑂</div><h1>Your repositories, together.</h1><p>Open a repository to inspect history and commit changes, or open a JSON project to check and pull a group.</p><form onSubmit={e=>{e.preventDefault();void perform(async()=>{const path=await save({title:'Create project',defaultPath:'work.git-view.json',filters:[{name:'Git View project',extensions:['json']}]});if(path)setProject(await api.createProject(path,{version:1,name:projectName.trim(),repositories:[]}));});}}><label>New project name<input value={projectName} onChange={e=>setProjectName(e.target.value)} placeholder="Work"/></label><button disabled={busy||!projectName.trim()}>Create project</button></form><p className="hint">CLI: <code>git-view repo .</code> · <code>git-view group work.git-view.json</code></p></main>}
  </div>;
}
