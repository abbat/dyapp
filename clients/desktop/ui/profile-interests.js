import { strings } from './locale.js';

// Rust owns canonicalization; keep one tag per line so punctuation remains part of a tag.
const interestInput = document.getElementById('profile-interests');
export const profileInterestTags = () =>
  interestInput.value
    .split('\n')
    .map((tag) => tag.trim())
    .filter(Boolean);
export const showProfileInterests = (tags) => {
  interestInput.value = (tags || []).join('\n');
  interestInput.setCustomValidity('');
};
interestInput.addEventListener('input', () => {
  interestInput.setCustomValidity(
    profileInterestTags().length > 32 ? strings['profile.interestsInvalid'] : '',
  );
});
