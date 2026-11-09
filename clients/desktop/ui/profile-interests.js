import { byId as domById } from './dom.js';
import { strings } from './locale.js';

// Rust owns canonicalization; keep one tag per line so punctuation remains part of a tag.
const interestInput = domById('profile-interests', HTMLTextAreaElement);
export const profileInterestTags = () =>
  interestInput.value
    .split('\n')
    .map((tag) => tag.trim())
    .filter(Boolean);
/** @param {readonly string[] | null | undefined} tags */
export const showProfileInterests = (tags) => {
  interestInput.value = (tags || []).join('\n');
  interestInput.setCustomValidity('');
};
interestInput.addEventListener('input', () => {
  interestInput.setCustomValidity(
    profileInterestTags().length > 32 ? strings['profile.interestsInvalid'] : '',
  );
});
