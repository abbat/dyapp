import { all as domAll, byId as domById } from './dom.js';
import { strings } from './locale.js';
import { checkRange, scaleFields } from './profile-fields.js';

const profileForm = domById('profile', HTMLFormElement);

// Who this profile seeks: the form card and its public rows (docs/decisions/0043).
const notLooking = domById('profile-not-looking', HTMLInputElement);
const seekingFields = domById('profile-seeking', HTMLFieldSetElement);

const seekingChoices = () => scaleFields.find((scale) => scale.key === 'gender')?.choices ?? [];
/** @param {{ key: string }} choice */
const genderName = (choice) => strings[`profile.value.gender.${choice.key}`];
export const seekingDraft = () => ({
  seekingGenders: [
    ...domAll(document, '#profile-seeking-genders input:checked', HTMLInputElement),
  ].map((box) => Number(box.value)),
  seekingAgeFrom: Number(domById('profile-seeking-age-from', HTMLInputElement).value) || null,
  seekingAgeTo: Number(domById('profile-seeking-age-to', HTMLInputElement).value) || null,
  notLooking: notLooking.checked,
});
/** @typedef {Pick<import('./profile-fields.js').ProfileDraft, 'seekingGenders' | 'seekingAgeFrom' | 'seekingAgeTo' | 'notLooking'>} SeekingDraft */
/** @param {SeekingDraft} draft */
export const showSeeking = (draft) => {
  domById('profile-seeking-genders', HTMLDivElement).replaceChildren(
    ...seekingChoices().map((choice) => {
      const box = document.createElement('input');
      box.id = `profile-seeking-gender-${choice.code}`;
      box.type = 'checkbox';
      box.value = String(choice.code);
      box.checked = Boolean(draft.seekingGenders?.includes(choice.code));
      const name = document.createElement('span');
      name.textContent = genderName(choice);
      const label = document.createElement('label');
      label.className = 'choice';
      label.append(box, name);
      return label;
    }),
  );
  domById('profile-seeking-age-from', HTMLInputElement).value = String(draft.seekingAgeFrom || '');
  domById('profile-seeking-age-to', HTMLInputElement).value = String(draft.seekingAgeTo || '');
  notLooking.checked = draft.notLooking;
  seekingFields.disabled = draft.notLooking;
};
/** @param {SeekingDraft} draft */
export const seekingFacts = (draft) => {
  if (draft.notLooking) return [[strings['profile.notLooking'], strings['profile.notLookingOn']]];
  const genders = seekingChoices()
    .filter((choice) => draft.seekingGenders?.includes(choice.code))
    .map(genderName);
  return [
    ...(genders.length ? [[strings['profile.seekingGenders'], genders.join(', ')]] : []),
    ...(draft.seekingAgeFrom
      ? [[strings['profile.seekingAgeFrom'], String(draft.seekingAgeFrom)]]
      : []),
    ...(draft.seekingAgeTo ? [[strings['profile.seekingAgeTo'], String(draft.seekingAgeTo)]] : []),
  ];
};
// A reversed age range is explained at its upper field; a disabled one is not checked.
export const profileFormValid = () => {
  checkRange('profile-seeking-age-from', 'profile-seeking-age-to');
  return profileForm.reportValidity();
};
notLooking.addEventListener('change', () => {
  seekingFields.disabled = notLooking.checked;
});
