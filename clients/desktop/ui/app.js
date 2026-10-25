// Practice D1: visible strings come from the matching i18n catalogue by data-i18n key.
// With --pseudo-locale each English string is accented and 40% longer.
const { invoke } = window.__TAURI__.core;
const accents = { a: 'á', e: 'é', i: 'í', o: 'ó', u: 'ú', y: 'ý', A: 'Á', E: 'É', I: 'Í', O: 'Ó', U: 'Ú' };
const pseudo = (text) => `[${[...text].map((c) => accents[c] || c).join('')}${'·'.repeat(Math.ceil(text.length * 0.4))}]`;

window.stringsShown = Promise.all([invoke('pseudo_locale'), invoke('ui_test_russian')])
  .then(async ([pseudoLocale, testRussian]) => {
    const systemLanguage = (navigator.languages?.[0] || navigator.language || 'en')
      .split(/[-_]/)[0].toLowerCase();
    const language = !pseudoLocale && (testRussian || systemLanguage === 'ru') ? 'ru' : 'en';
    const response = await fetch(`i18n/${language}.json`);
    if (!response.ok) throw new Error('Translation catalogue unavailable');
    const catalogue = await response.json();
    window.locale = language;
    window.pseudoLocale = pseudoLocale;
    document.documentElement.lang = language;
    window.strings = Object.fromEntries(Object.entries(catalogue)
      .map(([key, text]) => [key, pseudoLocale ? pseudo(text) : text]));
    for (const element of document.querySelectorAll('[data-i18n]')) {
      element.textContent = window.strings[element.dataset.i18n];
    }
  });
const ready = document.getElementById('app-ready');
const setup = document.getElementById('setup');
const identity = document.getElementById('identity');
const systemChoice = document.getElementById('system-choice');
const fileChoice = document.getElementById('file-choice');
const storageError = document.getElementById('storage-error');

const switchIdentity = document.getElementById('switch-identity');
const removeIdentity = document.getElementById('remove-identity');
const removeConfirm = document.getElementById('remove-confirm');
const removeAction = document.getElementById('remove-action');
const sections = [setup, identity, document.getElementById('picker'),
  document.getElementById('recovery'), document.getElementById('remove')];
// The opened identity or recovery state the remove screen acts on.
let current = null;

// Shows one section (or none) with its status line.
const show = (section, status) => {
  for (const each of sections) each.hidden = each !== section;
  switchIdentity.hidden = section !== identity && section !== sections[3];
  // The backend refuses to remove a damaged or unreadable slot.
  removeIdentity.hidden = section !== identity && !(section === sections[3]
    && ['missing', 'incomplete', 'deleting'].includes(current?.reason));
  storageError.hidden = true;
  ready.textContent = window.strings[status];
};

const showError = () => {
  show(null, 'status.error');
  switchIdentity.hidden = true;
  storageError.textContent = window.strings['storage.error'];
  storageError.hidden = false;
};

const reasonText = (reason) => window.strings[`recovery.${reason}`]
  || window.strings['recovery.unavailable'];

const act = async (pending) => {
  try {
    return await render(await pending);
  } catch (_) {
    showError();
    return { status: 'error' };
  }
};

const identityButton = (item) => {
  const button = document.createElement('button');
  button.type = 'button';
  Object.assign(button.dataset, { status: item.status, mode: item.mode || '' });
  const name = item.status === 'ready' ? item.peerId : reasonText(item.reason);
  const mode = window.strings[`identity.mode.${item.mode}`];
  button.textContent = mode ? `${name} — ${mode}` : name;
  button.addEventListener('click', () => act(invoke('select_identity', { slot: item.slot })));
  const entry = document.createElement('li');
  entry.append(button);
  return entry;
};

// Bootstrap settings of the open identity: draft rows, a bounded check, then Continue.
const network = document.getElementById('network');
const seeds = document.getElementById('seeds');
const networkName = document.getElementById('network-name');
const checkButton = document.getElementById('check-connection');
const cancelButton = document.getElementById('cancel-check');
const continueButton = document.getElementById('continue-connection');
const seedRows = document.getElementById('seed-rows');
const networkStatus = (key, detail) => {
  const text = window.strings[key];
  document.getElementById('network-status').textContent = detail ? `${text} ${detail}` : text;
};
const draft = () => ({
  seeds: seeds.value.split('\n').map((row) => row.trim()).filter(Boolean),
  name: networkName.value,
});

// Fills the form from the draft, else the active settings; a damaged file is shown, not reset.
const showNetwork = async () => {
  seedRows.replaceChildren();
  continueButton.disabled = true;
  try {
    const active = await invoke('load_network', { file: 'active' });
    const saved = await invoke('load_network', { file: 'draft' }) || active;
    seeds.value = saved ? saved.seeds.join('\n') : '';
    networkName.value = saved ? saved.name : '';
    networkStatus(active ? 'network.active' : 'network.idle');
  } catch (error) {
    networkStatus('network.failed', String(error));
  }
};

const rowItem = (seed, row) => {
  const entry = document.createElement('li');
  entry.dataset.status = row.status;
  const detail = row.peerId || row.reason;
  const text = `${seed}: ${window.strings[`network.row.${row.status}`]}`;
  entry.textContent = detail ? `${text} ${detail}` : text;
  return entry;
};

// A row-level problem the user must correct outranks nodes that are merely down.
const summary = (rows) => {
  const has = (...codes) => rows.some((row) => codes.includes(row.status));
  if (has('connected')) return 'network.connected';
  if (has('invalidAddress', 'invalidPeerId', 'duplicate')) return 'network.invalidAddress';
  if (has('wrongPeer', 'wrongNetwork')) return 'network.wrong';
  return 'network.unreachable';
};

const check = async () => {
  const rows = draft();
  checkButton.disabled = true;
  continueButton.disabled = true;
  cancelButton.hidden = false;
  seedRows.replaceChildren();
  networkStatus('network.checking');
  try {
    const result = await invoke('check_connection', { draft: rows, waitMs: 15000 });
    seedRows.replaceChildren(...result.map((row, at) => rowItem(rows.seeds[at], row)));
    continueButton.disabled = !result.some((row) => row.status === 'connected');
    networkStatus(summary(result));
  } catch (error) {
    const [code, ...reason] = String(error).split(': ');
    if (['empty', 'cancelled', 'invalidNetwork'].includes(code)) {
      networkStatus(`network.${code}`, reason.join(': '));
    } else {
      networkStatus('network.failed', String(error));
    }
  } finally {
    checkButton.disabled = false;
    cancelButton.hidden = true;
  }
};

network.addEventListener('submit', (event) => {
  event.preventDefault();
  window.connectionChecked = check();
});
// Edits invalidate the last success and are kept even if the app closes before a check.
network.addEventListener('input', () => { continueButton.disabled = true; });
network.addEventListener('change', () => invoke('save_network_draft', { draft: draft() })
  .catch((error) => networkStatus('network.failed', String(error))));
cancelButton.addEventListener('click', () => invoke('cancel_connection_check'));
continueButton.addEventListener('click', () => {
  continueButton.disabled = true;
  invoke('activate_connection')
    .then(() => networkStatus('network.active'))
    .catch((error) => networkStatus('network.failed', String(error)));
});

async function render(state) {
  if (state.status === 'ready' || state.status === 'recovery') current = state;
  if (state.status === 'ready') {
    document.getElementById('peer-id').textContent = state.peerId;
    document.getElementById('file-backup').hidden = state.mode !== 'file';
    document.getElementById('key-path').textContent = state.keyPath || '';
    await showNetwork();
    show(identity, 'status.ready');
    return state;
  }
  if (state.status === 'picker') {
    const items = await invoke('list_identities');
    document.getElementById('identities').replaceChildren(...items.map(identityButton));
    show(sections[2], 'status.pick');
    return state;
  }
  if (state.status === 'recovery') {
    document.getElementById('recovery-reason').textContent = reasonText(state.reason);
    document.getElementById('recovery-peer').textContent = state.peerId || '';
    show(sections[3], 'status.recovery');
    return state;
  }
  if (state.status !== 'setup') throw new Error('Unknown identity state');
  systemChoice.disabled = state.systemAvailable !== 'true';
  fileChoice.disabled = false;
  document.getElementById('system-reason').textContent = systemChoice.disabled
    ? window.strings[state.systemReason] : '';
  show(setup, 'status.choose');
  return state;
}

const choose = (mode) => {
  systemChoice.disabled = true;
  fileChoice.disabled = true;
  return act(invoke('create_identity', { mode }));
};

systemChoice.addEventListener('click', () => choose('system'));
fileChoice.addEventListener('click', () => choose('file'));
switchIdentity.addEventListener('click', () => act({ status: 'picker' }));
document.getElementById('add-identity')
  .addEventListener('click', () => act(invoke('storage_options')));
document.getElementById('retry')
  .addEventListener('click', () => act(invoke('identity_state')));

// Second confirmation: Remove stays disabled until the checkbox is ticked.
const showRemove = async () => {
  const items = await invoke('list_identities');
  const copies = current.peerId && items.some((item) => item.slot !== current.slot
    && item.status === 'ready' && item.peerId === current.peerId);
  document.getElementById('remove-peer').textContent = current.peerId
    || reasonText(current.reason);
  document.getElementById('remove-mode').textContent = window.strings[
    `identity.mode.${current.mode}`] || '';
  const path = document.getElementById('remove-path');
  path.hidden = !current.keyPath;
  path.querySelector('code').textContent = current.keyPath || '';
  document.getElementById('remove-copies').textContent = !current.peerId ? ''
    : window.strings[copies ? 'remove.otherCopy' : 'remove.lastCopy'];
  // A missing key or another copy means nothing irreplaceable is lost.
  document.getElementById('remove-warning').hidden = !current.peerId || copies;
  removeConfirm.checked = false;
  removeAction.disabled = true;
  show(sections[4], 'remove.title');
};
removeIdentity.addEventListener('click', () => showRemove().catch(showError));
removeConfirm.addEventListener('change', () => { removeAction.disabled = !removeConfirm.checked; });
removeAction.addEventListener('click', () => {
  removeAction.disabled = true;
  act(invoke('remove_identity', { slot: current.slot, peerId: current.peerId || null }));
});
document.getElementById('remove-cancel').addEventListener('click', () => act(current));
window.identityShown = window.stringsShown
  .then(() => invoke('identity_state'))
  .then(render)
  .catch(() => { showError(); return { status: 'error' }; });
