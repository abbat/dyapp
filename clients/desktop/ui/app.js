// Practice D1: every visible string comes from the i18n/en.json catalogue, by data-i18n key;
// plurals use Intl.PluralRules. With --pseudo-locale each string is accented and 40% longer.
const { invoke } = window.__TAURI__.core;
const accents = { a: 'á', e: 'é', i: 'í', o: 'ó', u: 'ú', y: 'ý', A: 'Á', E: 'É', I: 'Í', O: 'Ó', U: 'Ú' };
const pseudo = (text) => `[${[...text].map((c) => accents[c] || c).join('')}${'·'.repeat(Math.ceil(text.length * 0.4))}]`;

window.stringsShown = Promise.all([fetch('i18n/en.json').then((r) => r.json()), invoke('pseudo_locale')])
  .then(([catalogue, pseudoLocale]) => {
    window.pseudoLocale = pseudoLocale;
    window.strings = Object.fromEntries(Object.entries(catalogue)
      .map(([key, text]) => [key, pseudoLocale ? pseudo(text) : text]));
    for (const element of document.querySelectorAll('[data-i18n]')) {
      element.textContent = window.strings[element.dataset.i18n];
    }
  });
// Shows the peer ID that the Rust core derives from this run's identity key.
window.peerIdShown = invoke('peer_id').then((id) => {
  document.getElementById('peer-id').textContent = id;
});
