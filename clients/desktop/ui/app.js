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

const showError = () => {
  setup.hidden = true;
  identity.hidden = true;
  ready.textContent = window.strings['status.error'];
  storageError.textContent = window.strings['storage.error'];
  storageError.hidden = false;
};

const render = (state) => {
  if (state.status === 'ready') {
    setup.hidden = true;
    identity.hidden = false;
    ready.textContent = window.strings['status.ready'];
    document.getElementById('peer-id').textContent = state.peerId;
    return state;
  }
  if (state.status !== 'setup') throw new Error('Unknown identity state');
  setup.hidden = false;
  identity.hidden = true;
  ready.textContent = window.strings['status.choose'];
  systemChoice.disabled = state.systemAvailable !== 'true';
  document.getElementById('system-reason').textContent = systemChoice.disabled
    ? window.strings[state.systemReason] : '';
  return state;
};

const choose = async (mode) => {
  systemChoice.disabled = true;
  fileChoice.disabled = true;
  try {
    const peerId = await invoke('create_identity', { mode });
    render({ status: 'ready', peerId });
  } catch (_) {
    showError();
  }
};

systemChoice.addEventListener('click', () => choose('system'));
fileChoice.addEventListener('click', () => choose('file'));
window.identityShown = window.stringsShown
  .then(() => invoke('identity_state'))
  .then(render)
  .catch(() => { showError(); return { status: 'error' }; });
