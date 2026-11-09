import { identityState, identityCurrent } from './identity-state.js';
import { invoke } from './ipc.js';
import { strings, locale } from './locale.js';

/** @param {import('./ipc.js').Place} place */
const placeLabel = (place) => (place.region ? `${place.name}, ${place.region}` : place.name);
/**
 * @param {HTMLInputElement | HTMLSelectElement} countryInput
 * @param {HTMLInputElement} input
 * @param {HTMLDataListElement} list
 * @param {() => string} country
 * @param {import('./ipc.js').Invoke} [call]
 */
const placePicker = (countryInput, input, list, country, call = invoke) => {
  /** @type {Map<string, string>} */
  let ids = new Map();
  /** @param {import('./ipc.js').Place[]} places */
  const show = (places) => {
    ids = new Map(places.map((place) => [placeLabel(place), String(place.id)]));
    list.replaceChildren(
      ...[...ids.keys()].map((label) => {
        const option = document.createElement('option');
        option.value = label;
        return option;
      }),
    );
  };
  const check = () =>
    input.setCustomValidity(
      input.value && !ids.has(input.value) ? strings['profile.placeUnknown'] : '',
    );
  /** @param {import('./ipc.js').Place | null} place */
  const set = (place) => {
    show(place ? [place] : []);
    input.value = place ? placeLabel(place) : '';
    check();
  };
  countryInput.addEventListener('input', () => set(null));
  input.addEventListener('input', () => {
    const text = input.value;
    if (ids.has(text)) return check();
    const generation = identityState.generation;
    const selectedCountry = country();
    controller.placesShown = call('search_places', {
      country: selectedCountry,
      prefix: text,
      lang: locale,
      limit: 20,
    }).then((places) => {
      if (!identityCurrent(generation)) return;
      if (input.value === text && country() === selectedCountry) show(places);
      check();
    });
  });
  return { set, id: () => ids.get(input.value) || '' };
};
/** @param {string} country @param {string} id */
const placeText = (country, id) =>
  id
    ? invoke('place_name', {
        country,
        id: Number(id),
        lang: locale,
      }).then((place) => (place ? placeLabel(place) : id))
    : Promise.resolve('');

export const controller = {
  placeText,
  /** @type {Promise<void> | undefined} */
  placesShown: undefined,
};

export { placePicker };
