import { navigationBlocked } from '../api/navigation';

const mac = () => /Mac|iPhone|iPad/.test(navigator.platform);
export const shortcutLabel = (key: string) => mac() ? `⌘${key.toUpperCase()}` : `Ctrl+${key.toUpperCase()}`;
export const shortcutAria = (key: string) => `${mac() ? 'Meta' : 'Control'}+${key.toUpperCase()}`;
export function primaryShortcut(event: KeyboardEvent, key: string) {
  return !event.defaultPrevented && !event.isComposing && !event.altKey && !event.shiftKey
    && (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === key;
}
export const shortcutsBlocked = () => navigationBlocked() || Boolean(document.querySelector('dialog[open]'));
