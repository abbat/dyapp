import { all as domAll, byId as domById, find as domFind } from './dom.js';
export const controller = {};

// Keep category navigation local to Settings; backend availability owns inner cards.
const root = domById('settings-root', HTMLDivElement);
const back = domById('settings-back', HTMLButtonElement);
const panes = [...domAll(document, '[data-settings-pane]', HTMLDivElement)];
const parent = {
  identities: 'account',
  devices: 'account',
  blocked: 'privacy',
  passed: 'privacy',
};
let selected = null;
const open = (name = null, focus = true) => {
  if (name && !panes.some((pane) => pane.dataset.settingsPane === name)) return;
  selected = name;
  root.hidden = Boolean(name);
  back.hidden = !name;
  for (const pane of panes) pane.hidden = pane.dataset.settingsPane !== name;
  document.dispatchEvent(new CustomEvent('dyapp:settings-pane', { detail: name }));
  if (focus) {
    window.scrollTo(0, 0);
    (name ? back : root.querySelector('button')).focus();
  }
};
for (const button of domAll(document, '[data-settings-open]', HTMLButtonElement)) {
  button.addEventListener('click', () => open(button.dataset.settingsOpen));
}
back.addEventListener('click', () => {
  const previous = selected;
  const destination = parent[previous] || null;
  open(destination, false);
  const container = destination
    ? panes.find((pane) => pane.dataset.settingsPane === destination)
    : root;
  domFind(container, `[data-settings-open="${previous}"]`, HTMLElement)?.focus();
  window.scrollTo(0, 0);
});
document.addEventListener('dyapp:identity-change', () => open(null, false));
document.addEventListener('dyapp:area-change', () => open(null, false));
// Native checkbox checked state supplies the switch's accessibility state.
for (const id of [
  'send-views',
  'send-read-receipts',
  'send-typing',
  'message-notifications',
  'tray-show',
  'tray-close',
]) {
  const input = domById(id, HTMLElement);
  input.setAttribute('role', 'switch');
  input.classList.add('switch');
}
controller.openSettingsPane = open;
