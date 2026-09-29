import { ToolbarPopover } from '../../app/ToolbarPopover';
import { Icon } from '../../app/Icon';
import { PANEL_LIMITS, type PanelWidth, type ViewPreferences } from './useViewPreferences';

const panelLabels: Record<PanelWidth, string> = { changesWidth: 'Changed files width', branchesWidth: 'Branches width', historyWidth: 'History width' };
export function ViewPreferencesMenu({ preferences, update, reset, warning }: {
  preferences: ViewPreferences; update: (patch: Partial<ViewPreferences>) => void; reset: () => void; warning: string;
}) {
  return <ToolbarPopover name="View preferences" className="view-preferences" label={<>View <Icon name="chevron" /></>}>
    {() => <>
      <p className="popover-caption">View and panels</p>
      <p className="view-preferences-help">Saved on this device for all repositories.</p>
      {warning && <p role="status" className="view-preferences-warning">{warning}</p>}
      <label>Opening view<select aria-label="Opening view" value={preferences.openingView} onChange={event => update({ openingView: event.target.value as ViewPreferences['openingView'] })}>
        <option value="changes">Changes</option><option value="history">History</option>
      </select></label>
      <small>Used the next time you open a repository.</small>
      <label>Diff layout<select aria-label="Preferred diff layout" value={preferences.diffLayout} onChange={event => update({ diffLayout: event.target.value as ViewPreferences['diffLayout'] })}>
        <option value="unified">Unified</option><option value="split">Side-by-side</option>
      </select></label>
      <label><input type="checkbox" checked={preferences.showCommitDetails} onChange={event => update({ showCommitDetails: event.target.checked })} /> Show commit details</label>
      <div className="popover-separator" />
      {(Object.keys(panelLabels) as PanelWidth[]).map(key => <label className="panel-width-control" key={key}>
        <span>{panelLabels[key]} <output>{preferences[key]} px</output></span>
        <input type="range" aria-label={panelLabels[key]} min={PANEL_LIMITS[key][0]} max={PANEL_LIMITS[key][1]} step={1} value={preferences[key]} onChange={event => update({ [key]: Number(event.target.value) })} />
      </label>)}
      <small>Panels fit the available space without changing saved widths. You can also drag their dividers or use the arrow keys.</small>
      <div className="popover-separator" />
      <button onClick={reset}>Reset view preferences</button>
    </>}
  </ToolbarPopover>;
}
