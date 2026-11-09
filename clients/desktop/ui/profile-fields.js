import { byId as domById } from './dom.js';
import { locale, strings } from './locale.js';

/** @typedef {{ field: number, key: string, ordered: boolean, choices: { code: number, key: string }[] }} PickerScale */
/**
 * @typedef {{ name: string, gender: number, orientation: number, relationshipGoal: number,
 *   rolePreference: number, careerAmbition: number, financialPhilosophy: number,
 *   employmentStatus: number, fitnessLevel: number, zodiac: number,
 *   birthYear: number | null, birthMonth: number | null, birthDay: number | null,
 *   height: number | null, about: string, interests: string[], smoking: number,
 *   drinking: number, tattoos: number, maritalStatus: number, pets: number,
 *   country: string, place: string, seekingGenders: number[], seekingAgeFrom: number | null,
 *   seekingAgeTo: number | null, notLooking: boolean, photos: number[][] }} ProfileDraft
 * @typedef {'name' | 'height' | 'about' | 'interests' | 'smoking' | 'drinking' | 'tattoos' |
 *   'maritalStatus' | 'pets' | 'country' | 'place' | 'seekingGenders' | 'seekingAgeFrom' |
 *   'seekingAgeTo' | 'notLooking'} DefaultedDraftFields
 * @typedef {Omit<ProfileDraft, DefaultedDraftFields | 'photos'> &
 *   Partial<Pick<ProfileDraft, DefaultedDraftFields>>} ProfileDraftInput
 * @typedef {{ draft: ProfileDraft, published: number, ages: [number, number] | null,
 *   problem: 'notAttested' | 'invalidBirthDate' | 'underage' | null,
 *   searchMinutes: number | null, hidden: boolean }} ProfileView
 */
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
