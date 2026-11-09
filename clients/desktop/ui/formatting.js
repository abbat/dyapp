import { locale, strings } from './locale.js';

const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
/** @param {number} bytes */
const sizeText = (bytes) => {
  let unit = 0;
  let value = bytes;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit ? 1 : 0;
  const number = new Intl.NumberFormat(locale, {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }).format(value);
  return `${number} ${strings[`usage.unit.${units[unit]}`]}`;
};

/** @param {{ sentAtMs?: number | null } | null | undefined} message */
const chatTime = (message) =>
  message?.sentAtMs == null
    ? ''
    : new Intl.DateTimeFormat(document.documentElement.lang, {
        dateStyle: 'short',
        timeStyle: 'short',
      }).format(new Date(message.sentAtMs));
/** @param {string | null | undefined} text */
const chatLinkHosts = (text) => {
  const hosts = new Set();
  for (const match of (text || '').matchAll(/https?:\/\/[^\s<>"']+/gi)) {
    try {
      const url = new URL(match[0].replace(/[),.!?]+$/u, ''));
      if (url.protocol === 'http:' || url.protocol === 'https:') hosts.add(url.host);
    } catch {
      /* A malformed URL remains plain text. */
    }
  }
  return [...hosts];
};

export { sizeText, chatTime, chatLinkHosts };
