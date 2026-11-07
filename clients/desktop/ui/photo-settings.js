// Auto-download mode is independent of the originals cache budget.
const photoModeError = document.getElementById('photo-mode-error');
let photoModeRequest = 0;
const showPhotoMode = async (call = invoke) => {
  const generation = identityGeneration;
  const request = ++photoModeRequest;
  try {
    const mode = await call('auto_download_mode');
    if (!identityCurrent(generation) || request !== photoModeRequest) return;
    for (const input of document.querySelectorAll('input[name="photo-mode"]')) {
      input.checked = input.value === mode;
    }
    photoModeError.hidden = true;
  } catch (_) {
    if (identityCurrent(generation) && request === photoModeRequest) photoModeError.hidden = false;
  }
};
document.getElementById('photo-mode').addEventListener('change', (event) => {
  const generation = identityGeneration;
  const request = ++photoModeRequest;
  window.photoModeSaved = invoke('set_auto_download_mode', { mode: event.target.value }).then(
    () => {
      if (identityCurrent(generation) && request === photoModeRequest) photoModeError.hidden = true;
    },
    () => {
      if (identityCurrent(generation) && request === photoModeRequest)
        photoModeError.hidden = false;
    },
  );
});
document.addEventListener('dyapp:area-change', (event) => {
  if (event.detail === 'settings') window.photoModeShown = showPhotoMode();
});
