// The owner's photos in the profile editor. Loaded after chats-photo-send.js in the shared page
// scope. Every change runs with the form busy: a proof-of-work step saves the draft it read, so a
// change made during one would be lost.
const MAX_PROFILE_PHOTOS = 6;
const PROFILE_PHOTO_SIDES = [1600, 1280];
const PROFILE_PHOTO_QUALITIES = [0.85, 0.7, 0.55];
const MAX_PROFILE_PHOTO_BYTES = 512 * 1024;
const PROFILE_TILE_SIDE = 320;
const profilePhotoGrid = document.getElementById('profile-photos');
const profilePhotoAdd = document.getElementById('profile-photo-add');
const profilePhotoInput = document.getElementById('profile-photo-input');
const profilePhotoHint = document.getElementById('profile-photo-hint');
const profilePhotoLimit = document.getElementById('profile-photo-limit');
const profilePhotoDialog = document.getElementById('photo-remove-dialog');
const profilePhotoCancel = document.getElementById('photo-remove-cancel');
let profilePhotoCount = 0;
let profilePhotoRemoval = null;

window.limitProfilePhotos = () => {
  const full = profilePhotoCount >= MAX_PROFILE_PHOTOS;
  profilePhotoAdd.disabled = profileBusy || full;
  profilePhotoHint.hidden = full;
  profilePhotoLimit.hidden = !full;
};
const profilePhotoFailure = (error) => {
  const key = { tooLarge: 'profile.photoTooLarge', unsupported: 'profile.photoUnsupported',
    corrupted: 'profile.photoUnsupported', photoRefused: 'profile.photoRefused' }[error];
  if (key) profileMessage(key);
  else profileMessage('profile.failed', String(error));
};
const changeProfilePhotos = async (work, call, focus) => {
  if (profileBusy) return;
  const generation = identityGeneration;
  setProfileBusy(true);
  try {
    const view = await work();
    if (!identityCurrent(generation)) return;
    window.showProfilePhotos(view.draft.photos, call);
    profileMessage('profile.draft');
  } catch (error) {
    if (identityCurrent(generation)) profilePhotoFailure(error);
  } finally {
    if (identityCurrent(generation)) {
      setProfileBusy(false);
      focus();
    }
  }
};
const addProfilePhoto = (file, call = invoke) => changeProfilePhotos(async () => {
  const { photo, preview } = await preparePhoto(file, PROFILE_PHOTO_SIDES,
    PROFILE_PHOTO_QUALITIES, MAX_PROFILE_PHOTO_BYTES);
  return call('add_profile_photo', { full: photo, thumbnail: preview });
}, call, () => profilePhotoAdd.focus());
const moveProfilePhoto = (from, to, name, call) => changeProfilePhotos(
  () => call('move_profile_photo', { from, to }), call, () => {
    const tile = profilePhotoGrid.children[to];
    (tile.querySelector(`.${name}`) || tile.querySelector('button')).focus();
  });
const askProfilePhotoRemoval = (index, call) => {
  profilePhotoRemoval = () => changeProfilePhotos(
    () => call('remove_profile_photo', { index }), call, () => profilePhotoAdd.focus());
  profilePhotoDialog.hidden = false;
  profilePhotoDialog.showModal();
  profilePhotoCancel.focus();
};
const profilePhotoText = (key, index, count) => window.strings[key]
  .replace('{n}', index + 1).replace('{count}', count);
const profilePhotoTool = (glyph, name, label, click) => {
  const button = document.createElement('button');
  button.type = 'button';
  button.className = `btn quiet ${name}`;
  button.textContent = glyph;
  button.title = label;
  button.setAttribute('aria-label', label);
  button.addEventListener('click', click);
  return button;
};
const profilePhotoTile = (hash, index, count, call) => {
  const tile = document.createElement('figure');
  tile.className = 'photo-tile';
  const canvas = document.createElement('canvas');
  canvas.setAttribute('role', 'img');
  canvas.setAttribute('aria-label', profilePhotoText('profile.photoLabel', index, count));
  tile.ready = call('profile_photo', { hash })
    .then((bytes) => drawPhoto(canvas, bytes, 'image/jpeg', PROFILE_TILE_SIDE))
    .catch(() => { canvas.hidden = true; });
  tile.append(canvas);
  if (index === 0) {
    const main = document.createElement('figcaption');
    main.className = 'caption';
    main.textContent = window.strings['profile.photoMain'];
    tile.append(main);
  }
  const moved = (to, name) => () => { window.profilePhotoChanged = moveProfilePhoto(index, to, name, call); };
  const tools = document.createElement('div');
  tools.className = 'photo-tools';
  if (index > 0) {
    tools.append(profilePhotoTool('←', 'left',
      profilePhotoText('profile.photoLeft', index, count), moved(index - 1, 'left')));
  }
  if (index < count - 1) {
    tools.append(profilePhotoTool('→', 'right',
      profilePhotoText('profile.photoRight', index, count), moved(index + 1, 'right')));
  }
  tools.append(profilePhotoTool('×', 'remove', profilePhotoText('profile.photoRemove', index, count),
    () => askProfilePhotoRemoval(index, call)));
  tile.append(tools);
  return tile;
};
// The open identity's first draft photo on its avatars. It is local, so no node hears of this.
// Other identities keep their letters: their stores stay closed.
let ownPhoto = null;
let ownPhotoRun = 0;
const paintOwnAvatar = async (avatar) => {
  const photo = ownPhoto;
  const canvas = document.createElement('canvas');
  const drawn = !photo || await drawPhoto(canvas, photo, 'image/jpeg', PROFILE_TILE_SIDE)
    .then(() => true, () => false);
  if (photo !== ownPhoto) return;
  avatar.querySelector('canvas')?.remove();
  if (photo && drawn) avatar.append(canvas);
};
window.ownAvatar = (avatar) => {
  avatar.dataset.own = 'true';
  paintOwnAvatar(avatar);
};
const showOwnAvatars = async (hash, call) => {
  const run = ++ownPhotoRun;
  const photo = hash ? await call('profile_photo', { hash }).catch(() => null) : null;
  if (run !== ownPhotoRun) return;
  ownPhoto = photo;
  await Promise.all([...document.querySelectorAll('.avatar[data-own]')].map(paintOwnAvatar));
};
document.addEventListener('dyapp:identity-change', () => {
  ownPhotoRun += 1;
  ownPhoto = null;
});
window.showProfilePhotos = (hashes = [], call = invoke) => {
  window.ownAvatarsShown = showOwnAvatars(hashes[0], call);
  profilePhotoCount = hashes.length;
  profilePhotoGrid.replaceChildren(...hashes.map((hash, index) => profilePhotoTile(
    hash, index, hashes.length, call)));
  window.limitProfilePhotos();
};
profilePhotoAdd.addEventListener('click', () => profilePhotoInput.click());
profilePhotoInput.addEventListener('change', () => {
  const [file] = profilePhotoInput.files;
  if (file) window.profilePhotoChanged = addProfilePhoto(file);
  profilePhotoInput.value = '';
});
profilePhotoDialog.addEventListener('close', () => { profilePhotoDialog.hidden = true; });
profilePhotoCancel.addEventListener('click', () => profilePhotoDialog.close());
document.getElementById('photo-remove-confirm').addEventListener('click', () => {
  profilePhotoDialog.close();
  window.profilePhotoChanged = profilePhotoRemoval();
});
