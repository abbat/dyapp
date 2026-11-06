// Rust owns canonicalization; keep one tag per line so punctuation remains part of a tag.
const interestInput = document.getElementById('profile-interests');
window.profileInterestTags = () => interestInput.value.split('\n').map((tag) => tag.trim()).filter(Boolean);
window.showProfileInterests = (tags) => {
  interestInput.value = (tags || []).join('\n');
  interestInput.setCustomValidity('');
};
interestInput.addEventListener('input', () => {
  interestInput.setCustomValidity(window.profileInterestTags().length > 32
    ? window.strings['profile.interestsInvalid'] : '');
});
