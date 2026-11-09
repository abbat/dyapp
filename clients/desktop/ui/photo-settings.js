import { all as domAll, byId as domById, eventDetail } from './dom.js';
import { identityState, identityCurrent } from './identity-state.js';
import { invoke } from './ipc.js';
export const controller = {};
/** @type {Promise<void> | undefined} */
controller.photoModeSaved = undefined;
/** @type {Promise<void> | undefined} */
controller.photoModeShown = undefined;

// Auto-download mode is independent of the originals cache budget.
const photoModeError = domById('photo-mode-error', HTMLParagraphElement);
let photoModeRequest = 0;
const showPhotoMode = async (call = invoke) => {
  const generation = identityState.generation;
  const request = ++photoModeRequest;
  try {
    const mode = await call('auto_download_mode');
    if (!identityCurrent(generation) || request !== photoModeRequest) return;
    for (const input of domAll(document, 'input[name="photo-mode"]', HTMLInputElement)) {
      input.checked = input.value === mode;
    }
    photoModeError.hidden = true;
  } catch (_) {
    if (identityCurrent(generation) && request === photoModeRequest) photoModeError.hidden = false;
  }
};
domById('photo-mode', HTMLFormElement).addEventListener('change', (event) => {
  if (!(event.target instanceof HTMLInputElement)) return;
  const generation = identityState.generation;
  const request = ++photoModeRequest;
  controller.photoModeSaved = invoke('set_auto_download_mode', { mode: event.target.value }).then(
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
  if (eventDetail(event) === 'settings') controller.photoModeShown = showPhotoMode();
});
