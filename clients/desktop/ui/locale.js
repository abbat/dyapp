// Visible strings come from the matching i18n catalogue by data-i18n key.
const accents = {
  a: 'á',
  e: 'é',
  i: 'í',
  o: 'ó',
  u: 'ú',
  y: 'ý',
  A: 'Á',
  E: 'É',
  I: 'Í',
  O: 'Ó',
  U: 'Ú',
};
const pseudo = (text) =>
  `[${[...text].map((c) => accents[c] || c).join('')}${'·'.repeat(Math.ceil(text.length * 0.4))}]`;

export let locale;
export let pseudoLocale;
export let strings;

export const loadLocale = async (invoke) => {
  const options = await invoke('locale_options');
  const [pseudoEnabled, testRussian] = options;
  const systemLanguage = (navigator.languages?.[0] || navigator.language || 'en')
    .split(/[-_]/)[0]
    .toLowerCase();
  // Settings → Language overrides the system unless a test or the pseudo-locale sets it.
  const chosen = localStorage.getItem('language');
  const wanted = chosen === 'en' || chosen === 'ru' ? chosen : systemLanguage;
  const language = !pseudoEnabled && (testRussian || wanted === 'ru') ? 'ru' : 'en';
  const response = await fetch(`i18n/${language}.json`);
  if (!response.ok) throw new Error('Translation catalogue unavailable');
  const catalogue = await response.json();
  locale = language;
  pseudoLocale = pseudoEnabled;
  document.documentElement.lang = language;
  strings = Object.fromEntries(
    Object.entries(catalogue).map(([key, text]) => [key, pseudoEnabled ? pseudo(text) : text]),
  );
  for (const element of document.querySelectorAll('[data-i18n]')) {
    element.textContent = strings[element.dataset.i18n];
  }
  // Icon-only buttons in the rail still have a name and a tooltip.
  for (const element of document.querySelectorAll('[data-i18n-label]')) {
    element.setAttribute('aria-label', strings[element.dataset.i18nLabel]);
    if (element.tagName === 'BUTTON') element.title = strings[element.dataset.i18nLabel];
  }
  for (const element of document.querySelectorAll('[data-i18n-placeholder]')) {
    element.placeholder = strings[element.dataset.i18nPlaceholder];
  }
  return { locale, pseudoLocale, strings };
};
