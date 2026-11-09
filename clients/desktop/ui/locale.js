import { all as domAll } from './dom.js';
import { invoke } from './ipc.js';

// Visible strings come from the matching i18n catalogue by data-i18n key.
/** @type {Record<string, string>} */
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
/** @param {string} text */
const pseudo = (text) =>
  `[${[...text].map((c) => accents[c] || c).join('')}${'·'.repeat(Math.ceil(text.length * 0.4))}]`;

/** @type {string} */
export let locale;
/** @type {boolean} */
export let pseudoLocale;
/** @type {Record<string, string>} */
export let strings;

/** @param {unknown} catalogue @param {boolean} pseudoEnabled */
const translateCatalogue = (catalogue, pseudoEnabled) => {
  if (!catalogue || typeof catalogue !== 'object' || Array.isArray(catalogue))
    throw new Error('Invalid translation catalogue');
  return Object.fromEntries(
    Object.entries(catalogue).map(([key, text]) => {
      if (typeof text !== 'string') throw new Error('Invalid translation string');
      return [key, pseudoEnabled ? pseudo(text) : text];
    }),
  );
};

/** @param {Element} element @param {string} attribute */
const translationFor = (element, attribute) => {
  const key = element.getAttribute(attribute);
  if (key === null) throw new Error('Missing translation attribute');
  return strings[key];
};

/** @param {(command: 'locale_options') => Promise<readonly [boolean, boolean]>} invoke */
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
  const translated = translateCatalogue(await response.json(), pseudoEnabled);
  locale = language;
  pseudoLocale = pseudoEnabled;
  document.documentElement.lang = language;
  strings = translated;
  for (const element of domAll(document, '[data-i18n]', HTMLElement)) {
    element.textContent = translationFor(element, 'data-i18n');
  }
  // Icon-only buttons in the rail still have a name and a tooltip.
  for (const element of domAll(document, '[data-i18n-label]', HTMLElement)) {
    const label = translationFor(element, 'data-i18n-label');
    element.setAttribute('aria-label', label);
    if (element.tagName === 'BUTTON') element.title = label;
  }
  for (const element of domAll(
    document,
    '[data-i18n-placeholder]',
    HTMLInputElement,
    HTMLTextAreaElement,
  )) {
    element.placeholder = translationFor(element, 'data-i18n-placeholder');
  }
  return { locale, pseudoLocale, strings };
};

export const stringsShown = loadLocale(invoke);
