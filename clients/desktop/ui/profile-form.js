import { locale, strings } from './locale.js';
import { countryName, scaleKey } from './profile-fields.js';
import { byId as domById } from './dom.js';

// Profile form elements are built only after app.js validates the identity generation.
export const profileForm = {
  /** @param {import('./profile-fields.js').PickerScale[]} scales */
  readScales(scales) {
    /** @type {Record<string, number>} */
    const values = Object.fromEntries(
      scales.map((scale) => [
        scaleKey(scale.key),
        Number(
          domById(
            `profile-${scale.key === 'relationship_goal' ? 'goal' : scale.key}`,
            HTMLSelectElement,
          ).value,
        ),
      ]),
    );
    const required = [
      'gender',
      'orientation',
      'relationshipGoal',
      'rolePreference',
      'careerAmbition',
      'financialPhilosophy',
      'employmentStatus',
      'fitnessLevel',
      'zodiac',
    ];
    if (required.some((key) => !Number.isInteger(values[key])))
      throw new Error('Missing required profile scales');
    return {
      ...values,
      gender: values.gender,
      orientation: values.orientation,
      relationshipGoal: values.relationshipGoal,
      rolePreference: values.rolePreference,
      careerAmbition: values.careerAmbition,
      financialPhilosophy: values.financialPhilosophy,
      employmentStatus: values.employmentStatus,
      fitnessLevel: values.fitnessLevel,
      zodiac: values.zodiac,
    };
  },
  /** @param {import('./profile-fields.js').PickerScale} scale @param {Record<string, unknown>} draft */
  scaleRow(scale, draft) {
    const label = document.createElement('label');
    const select = document.createElement('select');
    select.id = `profile-${scale.key === 'relationship_goal' ? 'goal' : scale.key}`;
    label.htmlFor = select.id;
    label.textContent = strings[`profile.field.${scale.key}`];
    const unset = document.createElement('option');
    unset.value = '0';
    unset.textContent = strings['profile.unset'];
    select.append(
      unset,
      ...scale.choices.map((choice) => {
        const option = document.createElement('option');
        option.value = String(choice.code);
        option.textContent = strings[`profile.value.${scale.key}.${choice.key}`];
        return option;
      }),
    );
    select.value = String(draft[scaleKey(scale.key)] || 0);
    const row = document.createElement('div');
    row.className = 'field';
    row.append(label, select);
    return row;
  },
  /** @param {readonly string[]} countries */
  countryOptions(countries) {
    const unset = document.createElement('option');
    unset.value = '';
    unset.textContent = strings['profile.unset'];
    return [
      unset,
      ...countries
        .map((code) => ({ code, name: countryName(code) }))
        .sort((a, b) => a.name.localeCompare(b.name, locale))
        .map(({ code, name }) => {
          const option = document.createElement('option');
          option.value = code;
          option.textContent = name;
          return option;
        }),
    ];
  },
};
