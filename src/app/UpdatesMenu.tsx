import { ToolbarPopover } from './ToolbarPopover';
import type { UpdateState } from './useUpdates';

const time = (iso: string) => new Date(iso).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
const day = (iso: string) => new Date(iso).toLocaleDateString(undefined, { dateStyle: 'medium' });

export function UpdatesMenu({ updates }: { updates: UpdateState }) {
  const { preferences, result, checking, error, highlighted } = updates;
  const latest = result?.available ? result.latestVersion : null;
  const skipped = latest !== null && latest === preferences.skippedVersion;
  return <ToolbarPopover name={highlighted ? 'Updates: new version available' : 'Updates'} title={highlighted ? `Git View ${latest} is available` : 'Updates'}
    className={`app-updates${highlighted ? ' has-update' : ''}`}
    label={highlighted ? <><span className="update-dot" aria-hidden="true" />Update available</> : 'Updates'}>
    {() => <>
      <p className="popover-caption">Git View updates</p>
      <div className="update-status" role="status">
        {checking ? <p>Checking for updates…</p>
          : latest && result ? <>
            <p><strong>Git View {latest} is available.</strong> You have {result.currentVersion}.</p>
            {(result.releaseName || result.publishedAt) && <p className="update-meta">{[result.releaseName, result.publishedAt && `Published ${day(result.publishedAt)}`].filter(Boolean).join(' · ')}</p>}
            {skipped && <p className="update-meta">You skipped this version. Automatic checks will not highlight it.</p>}
          </>
          : result ? <p>{result.latestVersion ? `Git View ${result.currentVersion} is up to date.` : 'No published release is available yet.'}
            <span className="update-meta"> Checked {time(result.checkedAt)}.</span></p>
          : !error && <p>{preferences.automatic ? 'Git View checks for updates shortly after it starts.' : 'Automatic checks are off.'}</p>}
        {error && <p className="update-error">{error}</p>}
      </div>
      {latest && result && <>
        {result.notes && <details className="update-notes"><summary>Release notes</summary>
          <pre>{result.notes}{result.notesTruncated ? '\n…' : ''}</pre>
        </details>}
        <button className="update-primary" title="Opens the release page in your browser. Download and install the package for your system." onClick={() => void updates.openReleasePage()}>Update Git View…</button>
        {skipped ? <button onClick={() => updates.skip(null)}>Stop skipping {latest}</button>
          : <button title="Hide this version's update badge. You can still update from this menu." onClick={() => updates.skip(latest)}>Skip this version</button>}
      </>}
      <div className="popover-separator" />
      <button disabled={checking} onClick={() => void updates.check()}>Check for updates</button>
      <label className="update-automatic">
        <input type="checkbox" checked={preferences.automatic} onChange={event => updates.setAutomatic(event.target.checked)} />
        <span>Check automatically<span className="update-meta">At startup and once a day. Contacts GitHub only; nothing is downloaded or installed.</span></span>
      </label>
    </>}
  </ToolbarPopover>;
}
