import { ToolbarPopover } from './ToolbarPopover';
import { shortcutLabel } from './shortcuts';

export function KeyboardShortcuts() {
  return <ToolbarPopover name="Keyboard shortcuts" label="Shortcuts" className="keyboard-shortcuts">
    {() => <>
      <h2>Keyboard shortcuts</h2>
      <p>Shortcuts apply to the visible workspace. Git actions still require their normal review.</p>
      <h3>Repository or project</h3>
      <dl>
        <dt><kbd>{shortcutLabel('f')}</kbd></dt><dd>Find in repository history, or focus the project name/path filter.</dd>
        <dt><kbd>{shortcutLabel('r')}</kbd></dt><dd>Refresh local status. Does not fetch or pull.</dd>
      </dl>
      <h3>Navigation</h3>
      <dl>
        <dt><kbd>Tab</kbd> / <kbd>Shift+Tab</kbd></dt><dd>Move between controls.</dd>
        <dt><kbd>Enter</kbd> / <kbd>Space</kbd></dt><dd>Activate a focused button; Space toggles a checkbox.</dd>
        <dt><kbd>↑</kbd> / <kbd>↓</kbd></dt><dd>Open a menu at its last/first command. Within command menus, file lists or history, move through items.</dd>
        <dt><kbd>Home</kbd> / <kbd>End</kbd></dt><dd>First/last item in command menus, file lists or loaded history.</dd>
        <dt><kbd>Escape</kbd></dt><dd>Close a menu or compact scope panel and return focus; in a search field, clear the current search.</dd>
      </dl>
      <h3>Panel dividers</h3>
      <dl><dt><kbd>←</kbd> / <kbd>→</kbd></dt><dd>Adjust preferred width by 10 pixels.</dd>
        <dt><kbd>Home</kbd> / <kbd>End</kbd></dt><dd>Minimum/maximum preferred width.</dd>
        <dt><kbd>Escape</kbd></dt><dd>Cancel an active drag.</dd></dl>
      <p>Text fields and sliders keep their normal editing keys. Shortcuts pause for confirmation and native file dialogs. Escape closes this reference.</p>
    </>}
  </ToolbarPopover>;
}
