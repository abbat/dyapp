import { byId as domById } from './dom.js';

// Rust interprets the raw lines; this module renders a saved canonical list.
const interestInput = domById('profile-interests', HTMLTextAreaElement);
/** @param {readonly string[] | null | undefined} tags */
export const showProfileInterests = (tags) => {
  interestInput.value = (tags || []).join('\n');
  interestInput.setCustomValidity('');
};
