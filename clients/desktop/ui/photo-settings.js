// Settings → Photos: stored per identity on this device, saved on every change. A lower limit asks
// first: it erases originals at once, and past the nodes' retention they cannot come back.
const photoModeError = document.getElementById('photo-mode-error');
const keepDialog = document.getElementById('photo-keep-dialog');
const keepCancel = document.getElementById('photo-keep-cancel');
let keepLimit = null;
let keepAsked = null;
const showKeep = () => {
  for (const input of document.querySelectorAll('input[name="photo-keep"]')) {
    input.checked = Number(input.value) === keepLimit;
  }
};
const showPhotoMode = async (call = invoke) => {
  const generation = identityGeneration;
  const [mode, limit] = await Promise.all(['auto_download_mode', 'kept_originals_mib']
    .map((command) => call(command).catch(() => null)));
  if (!identityCurrent(generation)) return;
  photoModeError.hidden = mode !== null && limit !== null;
  for (const input of document.querySelectorAll('input[name="photo-mode"]')) {
    input.checked = input.value === mode;
  }
  keepLimit = limit;
  showKeep();
};
const savePhotoChoice = (saving, saved = () => {}) => {
  const generation = identityGeneration;
  window.photoModeSaved = saving.then(
    () => {
      if (!identityCurrent(generation)) return;
      photoModeError.hidden = true;
      saved();
    },
    () => {
      if (!identityCurrent(generation)) return;
      photoModeError.hidden = false;
      showKeep();
    },
  );
};
const saveKeep = (mib) => savePhotoChoice(invoke('set_kept_originals_mib', { mib }), () => {
  keepLimit = mib;
  showKeep();
});
document.getElementById('photo-mode').addEventListener('change', (event) => {
  const { name, value } = event.target;
  if (name !== 'photo-keep') {
    savePhotoChoice(invoke('set_auto_download_mode', { mode: value }));
  } else if (keepLimit !== null && Number(value) >= keepLimit) {
    saveKeep(Number(value));
  } else {
    keepAsked = Number(value);
    keepDialog.hidden = false;
    keepDialog.showModal();
    keepCancel.focus();
  }
});
keepDialog.addEventListener('close', () => {
  keepDialog.hidden = true;
  showKeep();
});
keepCancel.addEventListener('click', () => keepDialog.close());
document.getElementById('photo-keep-confirm').addEventListener('click', () => {
  saveKeep(keepAsked);
  keepDialog.close();
});
document.addEventListener('dyapp:area-change', (event) => {
  if (event.detail === 'settings') window.photoModeShown = showPhotoMode();
});
