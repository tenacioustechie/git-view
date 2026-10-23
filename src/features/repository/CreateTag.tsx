import type { CommitDetails, RepositoryAction, RepositoryStatus } from '../../api/types';

export type TagDraft = {
  commitSnapshotId: string;
  targetOid: string;
  name: string;
  message: string;
};

export function CreateTag({ details, status, active, draft, onDraft, onPlan }: {
  details: CommitDetails;
  status: RepositoryStatus;
  active: boolean;
  draft: TagDraft | null;
  onDraft: (draft: TagDraft | null) => void;
  onPlan: (action: Extract<RepositoryAction, { kind: 'createTag' }>) => void;
}) {
  const current = draft?.targetOid === details.oid ? draft : null;
  const start = () => onDraft({ commitSnapshotId: details.snapshotId, targetOid: details.oid, name: '', message: '' });
  const update = (change: Partial<Pick<TagDraft, 'name' | 'message'>>) => {
    if (current) onDraft({ ...current, ...change });
  };
  const unavailable = status.gitState !== 'idle'
    ? 'Finish the current Git operation before creating a tag.'
    : active
      ? 'Wait for the current repository operation to finish.'
      : null;

  return <section className="create-tag" aria-label="Create tag on selected commit">
    {!current ? <>
      <p>Create an annotated local tag that points to this exact commit.</p>
      <button disabled={Boolean(unavailable)} title={unavailable ?? undefined} onClick={start}>Create tag…</button>
    </> : <form onSubmit={event => {
      event.preventDefault();
      onPlan({ kind: 'createTag', commitSnapshotId: current.commitSnapshotId, targetOid: current.targetOid, name: current.name, message: current.message });
    }}>
      <small>Selected commit: {details.message.split(/\r?\n/, 1)[0] || '(no subject)'} · {details.oid}</small>
      <label>Tag name<input autoFocus maxLength={255} value={current.name} onChange={event => update({ name: event.target.value })} placeholder="v1.0.0" /></label>
      <label>Annotation<textarea aria-label="Tag annotation" maxLength={65536} rows={3} value={current.message} onChange={event => update({ message: event.target.value })} placeholder="Describe this version" /></label>
      <small>The tag stays local until you explicitly push it outside this workflow.</small>
      {unavailable && <small>{unavailable}</small>}
      <div className="tag-actions"><button type="button" onClick={() => onDraft(null)}>Cancel</button><button type="submit" disabled={Boolean(unavailable) || !current.name || !current.message.trim()}>Preview tag</button></div>
    </form>}
  </section>;
}
