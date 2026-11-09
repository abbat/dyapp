import { byId as domById } from './dom.js';
import { locale, strings } from './locale.js';

/** @typedef {{ field: number, key: string, ordered: boolean, choices: { code: number, key: string }[] }} PickerScale */
/** @type {PickerScale[]} */
export let scaleFields = [];
/** @param {PickerScale[]} scales */
export const setScaleFields = (scales) => {
  scaleFields = scales;
};
/** @param {string} key */
export const camel = (key) => key.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase());
/** @param {string} code */
export const countryName = (code) =>
  new Intl.DisplayNames([locale], { type: 'region' }).of(code) ?? code;

// A reversed range is explained at its upper field before anything is sent.
/** @param {string} from @param {string} to */
export const checkRange = (from, to) => {
  const [low, high] = [domById(from, HTMLInputElement).value, domById(to, HTMLInputElement).value];
  domById(to, HTMLInputElement).setCustomValidity(
    low && high && Number(low) > Number(high) ? strings['search.rangeReversed'] : '',
  );
};
