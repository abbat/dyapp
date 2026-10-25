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
const sections = [setup, identity, document.getElementById('picker'),
  document.getElementById('recovery')];

// Shows one section (or none) with its status line.
const show = (section, status) => {
  for (const each of sections) each.hidden = each !== section;
  switchIdentity.hidden = section !== identity && section !== sections[3];
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
  const name = item.status === 'ready' ? item.peerId : reasonText(item.reason);
  const mode = window.strings[`identity.mode.${item.mode}`];
  button.textContent = mode ? `${name} — ${mode}` : name;
  button.addEventListener('click', () => act(invoke('select_identity', { slot: item.slot })));
  const entry = document.createElement('li');
  entry.append(button);
  return entry;
};

async function render(state) {
  if (state.status === 'ready') {
    document.getElementById('peer-id').textContent = state.peerId;
    document.getElementById('file-backup').hidden = state.mode !== 'file';
    document.getElementById('key-path').textContent = state.keyPath || '';
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
window.identityShown = window.stringsShown
  .then(() => invoke('identity_state'))
  .then(render)
  .catch(() => { showError(); return { status: 'error' }; });
