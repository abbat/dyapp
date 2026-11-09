import { strings } from './locale.js';
import * as fields from './profile-fields.js';
import { seekingFacts } from './profile-seeking.js';

/** @param {fields.ProfileDraft} draft @param {string} place */
export const publicFacts = (draft, place) => {
  const facts = fields.scaleFields.flatMap((scale) => {
    const option = scale.choices.find(
      (choice) => choice.code === draft[fields.scaleKey(scale.key)],
    );
    return option
      ? [
          [
            strings[`profile.field.${scale.key}`],
            strings[`profile.value.${scale.key}.${option.key}`],
          ],
        ]
      : [];
  });
  if (draft.country) facts.push([strings['profile.country'], fields.countryName(draft.country)]);
  if (draft.place) facts.push([strings['profile.place'], place]);
  for (const part of /** @type {const} */ (['Year', 'Month', 'Day'])) {
    const value = draft[`birth${part}`];
    if (value !== null) facts.push([strings[`profile.birth${part}`], String(value)]);
  }
  if (draft.height) facts.push([strings['profile.height'], String(draft.height)]);
  if (draft.about) facts.push([strings['profile.about'], draft.about]);
  if (draft.interests?.length)
    facts.push([strings['profile.interests'], draft.interests.join('; ')]);
  facts.push(...seekingFacts(draft));
  return facts;
};

/** @param {fields.ProfileDraft} draft @param {string} place */
export const publicFields = (draft, place) => {
  const facts = publicFacts(draft, place).map(([label, value]) => `${label}: ${value}`);
  if (draft.name) facts.unshift(`${strings['profile.name']}: ${draft.name}`);
  if (draft.photos?.length) facts.push(`${strings['profile.photos']}: ${draft.photos.length}`);
  return facts.length ? facts : [strings['profile.empty']];
};
