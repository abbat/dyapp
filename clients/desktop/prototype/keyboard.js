// Interaction-only prototype support: no data writes or backend actions.
const visible = (element) => {
  if (!element.getClientRects().length || getComputedStyle(element).visibility !== 'visible')
    return false;
  for (const details of document.querySelectorAll('details:not([open])')) {
    if (details.contains(element) && !details.querySelector(':scope > summary')?.contains(element))
      return false;
  }
  return true;
};
export const selector = (element) => {
  if (element.id) return `#${CSS.escape(element.id)}`;
  const parent = element.parentElement;
  if (!parent) return element.tagName.toLowerCase();
  return `${selector(parent)}>${element.tagName.toLowerCase()}:nth-child(${[...parent.children].indexOf(element) + 1})`;
};
const controls =
  'a[href], button, input:not([type="hidden"]), select, textarea, summary, [tabindex], [contenteditable="true"]';
export const activeDialog = () =>
  [...document.querySelectorAll('[role="dialog"]')].filter(visible).at(-1);
export const enabledControls = () =>
  [...(activeDialog() ?? document).querySelectorAll(controls)].filter(
    (element) => visible(element) && !element.matches(':disabled') && !element.closest('[inert]'),
  );
const radioPeers = (element) =>
  enabledControls().filter(
    (peer) => peer.type === 'radio' && peer.name === element.name && peer.form === element.form,
  );
export const tabStops = (reverse = false) =>
  enabledControls().filter((element) => {
    if (element.type !== 'radio' || !element.name) return element.tabIndex >= 0;
    const peers = radioPeers(element);
    return element === (peers.find((peer) => peer.checked) ?? (reverse ? peers.at(-1) : peers[0]));
  });
const invokers = new Map();
const focusHints = new Map(
  [...document.querySelectorAll('[role="dialog"]')].map((dialog) => [
    dialog,
    dialog.querySelector('.focus'),
  ]),
);
const contracts = {
  'chat-forward-dialog': ['chat', '#msg-forward'],
  'chat-forward-loading': ['chat', '#msg-forward'],
  'chat-forward-empty': ['chat', '#msg-forward'],
  'chat-forward-error': ['chat', '#msg-forward'],
  'chat-forward-send-error': ['chat', '#msg-forward'],
  'chat-forward-pending': ['chat', '#msg-forward'],
  'publish-dialog': ['profile', '#profile-publish'],
  'publish-progress': ['profile', '#profile-publish'],
  'photo-remove-dialog': ['profile', '#profile-photos .photo-tools button:last-child'],
  'chat-rename-dialog': ['chat-menu', '#chat-rename'],
  'chat-clear-dialog': ['chat-menu', '#chat-clear'],
  'verify-dialog': ['profile-view-safety', '#view-confirm'],
  'unlike-dialog': ['profile-view-match', '#view-unlike'],
  'delete-dialog': ['chat', '#msg-delete'],
  'remove-dialog': ['settings-identities-screen', '#settings-remove'],
  'remove-offline': ['settings-identities-screen', '#settings-remove'],
  'stickers-remove-dialog': ['stickers-ready', '#sticker-remove-1'],
  'stickers-remove-unreadable': ['stickers-ready', '#sticker-remove-2'],
  'device-code-dialog': ['settings-devices', '#device-link'],
  'device-code-error': ['settings-devices', '#device-link'],
  'device-send-error': ['settings-devices', '#device-link'],
  'device-code-bad': ['settings-devices', '#device-link'],
  'device-confirm-dialog': ['device-code-dialog', '#device-code-next'],
  'disconnect-dialog': ['settings-devices', '#device-disconnect'],
  'disconnect-offline': ['settings-devices', '#device-disconnect'],
  'revoked-dialog': ['wizard-identity', 'main[data-on~="wizard-identity"] h1', true],
};
for (const state of ['empty', 'ready', 'recent', 'loading', 'error', 'sending'])
  contracts[`sticker-picker-${state}`] = ['chat', '#chat-stickers'];
export const dialogContract = () => {
  const dialog = activeDialog();
  if (!dialog) return null;
  if (dialog.matches('dialog') && dialog.dataset.opener)
    return {
      dialog: selector(dialog),
      state: dialog.dataset.returnState || document.documentElement.dataset.screen,
      opener: `#${CSS.escape(dialog.dataset.opener)}`,
      heading: false,
    };
  const state = document.documentElement.dataset.screen;
  const contract = contracts[state];
  if (!contract) throw new Error(`Dialog state has no opener contract: ${state}`);
  return {
    dialog: selector(dialog),
    state:
      state.startsWith('chat-forward-') &&
      new URLSearchParams(location.search).get('forwardSource') === 'received'
        ? 'chat-forward-received-menu'
        : contract[0],
    opener: invokers.get(state)?.isConnected ? selector(invokers.get(state)) : contract[1],
    heading: Boolean(contract[2]),
  };
};
const modalInertness = () => {
  const scrim = activeDialog()?.closest('.modal-scrim');
  for (const child of document.body.children) child.inert = Boolean(scrim && child !== scrim);
  const menuOpen =
    ['menu', 'switcher'].includes(document.documentElement.dataset.screen) &&
    matchMedia('(max-width: 640px)').matches;
  for (const child of document.querySelectorAll('.shell > .content, .shell > .topbar'))
    child.inert = menuOpen;
};
const transientScopes = new Set();
// Counterexamples change presentation, not the browser's navigation history.
export const suspendUrlUpdates = () => {
  const scope = {};
  transientScopes.add(scope);
  return () => transientScopes.delete(scope);
};
export const showState = (state) => {
  const shown =
    state === 'chat-draft' ||
    state.startsWith('sticker-picker-') ||
    state.startsWith('chat-sticker-')
      ? 'chat'
      : state === 'profile-goal-dialog'
        ? 'profile'
        : state;
  const matches = (element, key) =>
    element.dataset[key].split(' ').some((value) => value === state || value === shown);
  for (const element of document.querySelectorAll('[data-on]'))
    element.hidden = !matches(element, 'on');
  for (const element of document.querySelectorAll('[data-off]'))
    element.hidden = matches(element, 'off');
  for (const element of document.querySelectorAll('[data-disabled]'))
    element.disabled = matches(element, 'disabled');
  document.documentElement.dataset.screen = state;
  const url = new URL(location.href);
  url.hash = state;
  if (!transientScopes.size) history.replaceState(null, '', url);
  document.title = `${window.strings['app.name']} — ${state}`;
  modalInertness();
  focusHints.get(activeDialog())?.focus();
};
export const closeDialog = () => {
  const source = document.documentElement.dataset.screen;
  const contract = dialogContract();
  if (!contract) return;
  showState(contract.state);
  invokers.delete(source);
  const target = document.querySelector(contract.opener);
  if (!target || !visible(target) || target.matches(':disabled'))
    throw new Error(`Dialog cannot restore ${contract.opener}`);
  if (contract.heading) target.tabIndex = -1;
  target.focus();
};
const keydown = (event) => {
  if (!activeDialog() || activeDialog().matches('dialog')) return;
  if (event.key === 'Escape') {
    event.preventDefault();
    if (document.documentElement.dataset.screen !== 'chat-forward-pending') closeDialog();
  } else if (event.key === 'Tab') {
    const stops = tabStops(event.shiftKey);
    const first = stops[0],
      last = stops.at(-1);
    const edge = event.shiftKey ? first : last;
    if (!activeDialog().contains(document.activeElement) || document.activeElement === edge) {
      event.preventDefault();
      (event.shiftKey ? last : first)?.focus();
    }
  }
};
document.addEventListener('keydown', keydown);
const boundOpeners = new Set();
for (const [state, contract] of Object.entries(contracts)) {
  if (contract[2] || ['#chat-stickers', '#msg-forward'].includes(contract[1])) continue;
  for (const opener of document.querySelectorAll(contract[1])) {
    if (boundOpeners.has(opener)) continue;
    boundOpeners.add(opener);
    opener.addEventListener('click', () => {
      invokers.set(state, opener);
      showState(state);
    });
  }
}
for (const button of document.querySelectorAll('[role="dialog"] button')) {
  if (!/(?:^|-)cancel(?:-|$)/.test(button.id) && button.id !== 'revoked-ok') continue;
  button.addEventListener('click', () => {
    if (!button.disabled && button.closest('[role="dialog"]') === activeDialog()) closeDialog();
  });
}
const initial = [...document.querySelectorAll('.focus')].find(visible);
for (const element of document.querySelectorAll('.focus')) element.classList.remove('focus');
modalInertness();
initial?.focus();
