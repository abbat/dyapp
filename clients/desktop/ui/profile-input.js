import { byId } from './dom.js';
import { identityState, identityCurrent } from './identity-state.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
import { seekingDraft } from './profile-seeking.js';

const form = byId('profile', HTMLFormElement);
/** @param {string} id */
const read = (id) => byId(id, HTMLInputElement).value;
export const profileTextInput = () => ({
  birthYear: read('birth-year'),
  birthMonth: read('birth-month'),
  birthDay: read('birth-day'),
  height: read('profile-height'),
  interests: byId('profile-interests', HTMLTextAreaElement).value,
  ...seekingDraft(),
});

/** @type {Record<string, [HTMLInputElement | HTMLTextAreaElement, string]>} */
const errors = {
  'invalid birth date': [byId('birth-day', HTMLInputElement), 'profile.problem.invalidBirthDate'],
  invalidHeight: [byId('profile-height', HTMLInputElement), 'profile.heightInvalid'],
  invalidInterests: [byId('profile-interests', HTMLTextAreaElement), 'profile.interestsInvalid'],
  'invalid profile seeking': [
    byId('profile-seeking-age-to', HTMLInputElement),
    'search.rangeReversed',
  ],
};
let revision = 0;
let pending = Promise.resolve();
export const profileInputReady = () => pending;
export const resetProfileInput = () => {
  revision += 1;
  pending = Promise.resolve();
  for (const [field] of Object.values(errors)) field.setCustomValidity('');
};

/** Check pure input only; stale responses cannot attach errors to a replacement editor. */
export const assessProfileInput = (call = invoke) => {
  resetProfileInput();
  const current = revision;
  const owner = identityState.generation;
  const input = profileTextInput();
  const snapshot = JSON.stringify(input);
  pending = call('profile_editor_input', { input }).then(
    () => {},
    (error) => {
      if (
        current !== revision ||
        !identityCurrent(owner) ||
        JSON.stringify(profileTextInput()) !== snapshot
      )
        return;
      const entry = errors[String(error)];
      if (!entry) return;
      const [field, message] = entry;
      // Preserve the browser's explanation for its own malformed/out-of-range input.
      if (field.validity.badInput || field.validity.rangeUnderflow || field.validity.rangeOverflow)
        return;
      field.setCustomValidity(strings[message]);
    },
  );
  return pending;
};

form.addEventListener('input', () => assessProfileInput(), true);
form.addEventListener('change', () => assessProfileInput(), true);
