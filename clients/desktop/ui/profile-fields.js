import { locale, strings } from './locale.js';

export let scaleFields = [];
export const setScaleFields = (scales) => {
  scaleFields = scales;
};
export const camel = (key) => key.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase());
export const countryName = (code) => new Intl.DisplayNames([locale], { type: 'region' }).of(code);

// A reversed range is explained at its upper field before anything is sent.
export const checkRange = (from, to) => {
  const [low, high] = [document.getElementById(from).value, document.getElementById(to).value];
  document
    .getElementById(to)
    .setCustomValidity(
      low && high && Number(low) > Number(high) ? strings['search.rangeReversed'] : '',
    );
};
