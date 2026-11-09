import { strings } from './locale.js';
import { checkRange, scaleFields } from './profile-fields.js';

const profileForm = document.getElementById('profile');

// Who this profile seeks: the form card and its public rows (docs/decisions/0043).
const notLooking = document.getElementById('profile-not-looking');
const seekingFields = document.getElementById('profile-seeking');
const seekingAge = (bound) => document.getElementById(`profile-seeking-age-${bound}`);
const seekingChoices = () => scaleFields.find((scale) => scale.key === 'gender')?.choices ?? [];
const genderName = (choice) => strings[`profile.value.gender.${choice.key}`];
export const seekingDraft = () => ({
  seekingGenders: [...document.querySelectorAll('#profile-seeking-genders input:checked')].map(
    (box) => Number(box.value),
  ),
  seekingAgeFrom: Number(seekingAge('from').value) || null,
  seekingAgeTo: Number(seekingAge('to').value) || null,
  notLooking: notLooking.checked,
});
export const showSeeking = (draft) => {
  document.getElementById('profile-seeking-genders').replaceChildren(
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
  seekingAge('from').value = draft.seekingAgeFrom || '';
  seekingAge('to').value = draft.seekingAgeTo || '';
  notLooking.checked = draft.notLooking;
  seekingFields.disabled = draft.notLooking;
};
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
