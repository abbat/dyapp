// Who this profile seeks: the form card and its public rows (docs/decisions/0043).
const notLooking = document.getElementById('profile-not-looking');
const seekingFields = document.getElementById('profile-seeking');
const seekingAge = (bound) => document.getElementById(`profile-seeking-age-${bound}`);
const seekingChoices = () => scaleFields.find((scale) => scale.key === 'gender')?.choices ?? [];
const genderName = (choice) => window.strings[`profile.value.gender.${choice.key}`];
// biome-ignore lint/correctness/noUnusedVariables: read by app.js, which loads after this file
const seekingDraft = () => ({
  seekingGenders: [...document.querySelectorAll('#profile-seeking-genders input:checked')].map(
    (box) => Number(box.value),
  ),
  seekingAgeFrom: Number(seekingAge('from').value) || null,
  seekingAgeTo: Number(seekingAge('to').value) || null,
  notLooking: notLooking.checked,
});
// biome-ignore lint/correctness/noUnusedVariables: read by app.js, which loads after this file
const showSeeking = (draft) => {
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
// biome-ignore lint/correctness/noUnusedVariables: read by app.js, which loads after this file
const seekingFacts = (draft) => {
  if (draft.notLooking)
    return [[window.strings['profile.notLooking'], window.strings['profile.notLookingOn']]];
  const genders = seekingChoices()
    .filter((choice) => draft.seekingGenders?.includes(choice.code))
    .map(genderName);
  return [
    ...(genders.length ? [[window.strings['profile.seekingGenders'], genders.join(', ')]] : []),
    ...(draft.seekingAgeFrom
      ? [[window.strings['profile.seekingAgeFrom'], String(draft.seekingAgeFrom)]]
      : []),
    ...(draft.seekingAgeTo
      ? [[window.strings['profile.seekingAgeTo'], String(draft.seekingAgeTo)]]
      : []),
  ];
};
// A reversed age range is explained at its upper field; a disabled one is not checked.
// biome-ignore lint/correctness/noUnusedVariables: read by app.js, which loads after this file
const profileFormValid = () => {
  checkRange('profile-seeking-age-from', 'profile-seeking-age-to');
  return profileForm.reportValidity();
};
notLooking.addEventListener('change', () => {
  seekingFields.disabled = notLooking.checked;
});
