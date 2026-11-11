import { all as domAll, byId as domById, find as domFind, query as domQuery } from './dom.js';
import { profileBusy, profileMessage, setProfileBusy } from './app.js';
import { drawPhoto, setAvatarPhoto } from './chat-photos.js';
import { photoPixels } from './chats-photo-send.js';
import { identityState, identityCurrent } from './identity-state.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
export const controller = {};
/** @type {Promise<void> | undefined} */
controller.profilePhotoChanged = undefined;
/** @type {Promise<void> | undefined} */
controller.ownAvatarsShown = undefined;

// Profile photos share the chat decoder; the core fits and encodes them with the profile budget.
// Keep the form busy while a photo changes; editor saves and proof steps transform the latest draft.
const MAX_PROFILE_PHOTOS = 6;
const PROFILE_TILE_SIDE = 320;
const profilePhotoGrid = domById('profile-photos', HTMLDivElement);
const profilePhotoAdd = domById('profile-photo-add', HTMLButtonElement);
const profilePhotoInput = domById('profile-photo-input', HTMLInputElement);
const profilePhotoHint = domById('profile-photo-hint', HTMLParagraphElement);
const profilePhotoLimit = domById('profile-photo-limit', HTMLParagraphElement);
const profilePhotoDialog = domById('photo-remove-dialog', HTMLDialogElement);
const profilePhotoCancel = domById('photo-remove-cancel', HTMLButtonElement);
let profilePhotoCount = 0;
// The core revision of the shown list; removal and moves send it so a stale list changes nothing.
/** @type {number[]} */
let profilePhotoRevision = [];
/** @type {(() => Promise<void>) | null} */
let profilePhotoRemoval = null;
const closePhotoRemoval = () => {
  profilePhotoRemoval = null;
  if (profilePhotoDialog.open) profilePhotoDialog.close();
  profilePhotoDialog.hidden = true;
};

controller.limitProfilePhotos = () => {
  const full = profilePhotoCount >= MAX_PROFILE_PHOTOS;
  profilePhotoAdd.disabled = profileBusy || full;
  profilePhotoHint.hidden = full;
  profilePhotoLimit.hidden = !full;
};
/** @param {unknown} error */
const profilePhotoFailure = (error) => {
  const key = {
    tooLarge: 'profile.photoTooLarge',
    unsupported: 'profile.photoUnsupported',
    corrupted: 'profile.photoUnsupported',
    photoRefused: 'profile.photoRefused',
    photoChanged: 'profile.photoChanged',
  }[String(error)];
  if (key) profileMessage(key);
  else profileMessage('profile.failed', String(error));
};
// Show the list that won; the user reviews it instead of the stale action being retried.
/** @param {import('./ipc.js').Invoke} call @param {number} generation */
const showCurrentPhotos = async (call, generation) => {
  const view = identityCurrent(generation) ? await call('load_profile').catch(() => null) : null;
  if (view && identityCurrent(generation))
    controller.showProfilePhotos(view.draft.photos, call, view.photoRevision);
};
/**
 * @param {() => void} focus
 * @param {() => Promise<import('./profile-fields.js').ProfileView>} work
 * @param {import('./ipc.js').Invoke} call
 */
const changeProfilePhotos = async (work, call, focus) => {
  if (profileBusy) return;
  const generation = identityState.generation;
  setProfileBusy(true);
  try {
    const view = await work();
    if (!identityCurrent(generation)) return;
    controller.showProfilePhotos(view.draft.photos, call, view.photoRevision);
    profileMessage('profile.draft');
  } catch (error) {
    if (String(error) === 'photoChanged') await showCurrentPhotos(call, generation);
    if (identityCurrent(generation)) profilePhotoFailure(error);
  } finally {
    if (identityCurrent(generation)) {
      setProfileBusy(false);
      focus();
    }
  }
};
/** @param {File} file */
const addProfilePhoto = (file, call = invoke) =>
  changeProfilePhotos(
    async () => {
      const { pixels, headers } = await photoPixels(file, 'profile', call);
      return call('add_profile_photo', pixels, { headers });
    },
    call,
    () => profilePhotoAdd.focus(),
  );
/** @param {number} from @param {number} to @param {string} name @param {import('./ipc.js').Invoke} call */
const moveProfilePhoto = (from, to, name, call) =>
  changeProfilePhotos(
    () => call('move_profile_photo', { from, to, revision: profilePhotoRevision }),
    call,
    () => {
      const tile = profilePhotoGrid.children[to];
      (
        domFind(tile, `.${name}`, HTMLElement) || domQuery(tile, 'button', HTMLButtonElement)
      ).focus();
    },
  );
/** @param {number} index @param {import('./ipc.js').Invoke} call */
const askProfilePhotoRemoval = (index, call) => {
  const generation = identityState.generation;
  const revision = profilePhotoRevision;
  profilePhotoRemoval = () =>
    identityCurrent(generation)
      ? changeProfilePhotos(
          () => call('remove_profile_photo', { index, revision }),
          call,
          () => profilePhotoAdd.focus(),
        )
      : Promise.resolve();
  profilePhotoDialog.hidden = false;
  profilePhotoDialog.showModal();
  profilePhotoCancel.focus();
};
/** @param {string} key @param {number} index @param {number} count */
const profilePhotoText = (key, index, count) =>
  strings[key].replace('{n}', String(index + 1)).replace('{count}', String(count));
/** @param {'left' | 'right' | 'remove'} name */
const profilePhotoIcon = (name) => {
  const icon = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  icon.setAttribute('viewBox', '0 0 24 24');
  icon.setAttribute('width', '1em');
  icon.setAttribute('height', '1em');
  icon.setAttribute('aria-hidden', 'true');
  icon.setAttribute('focusable', 'false');
  const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
  const paths = {
    left: 'M20 12H4m7-7-7 7 7 7',
    right: 'M4 12h16m-7-7 7 7-7 7',
    remove: 'M6 6l12 12M6 18 18 6',
  };
  path.setAttribute('d', paths[name]);
  path.setAttribute('fill', 'none');
  path.setAttribute('stroke', 'currentColor');
  path.setAttribute('stroke-width', '2');
  icon.append(path);
  return icon;
};
/** @param {'left' | 'right' | 'remove'} name @param {string} label @param {() => void} click */
const profilePhotoTool = (name, label, click) => {
  const button = document.createElement('button');
  button.type = 'button';
  button.className = `btn quiet ${name}`;
  button.append(profilePhotoIcon(name));
  button.title = label;
  button.setAttribute('aria-label', label);
  button.addEventListener('click', click);
  return button;
};
/**
 * @param {number} index @param {number} count @param {import('./ipc.js').Invoke} call
 * @param {number[]} hash
 */
const profilePhotoTile = (hash, index, count, call) => {
  /** @type {HTMLElement & { ready?: Promise<void> }} */
  const tile = document.createElement('figure');
  tile.className = 'photo-tile';
  const canvas = document.createElement('canvas');
  canvas.setAttribute('role', 'img');
  canvas.setAttribute('aria-label', profilePhotoText('profile.photoLabel', index, count));
  tile.ready = call('profile_photo', { hash })
    .then((bytes) => drawPhoto(canvas, bytes, 'image/jpeg', PROFILE_TILE_SIDE))
    .catch(() => {
      canvas.hidden = true;
    });
  tile.append(canvas);
  if (index === 0) {
    const main = document.createElement('figcaption');
    main.className = 'caption';
    main.textContent = strings['profile.photoMain'];
    tile.append(main);
  }
  /** @param {string} name @param {number} to */
  const moved = (to, name) => () => {
    controller.profilePhotoChanged = moveProfilePhoto(index, to, name, call);
  };
  const tools = document.createElement('div');
  tools.className = 'photo-tools';
  if (index > 0) {
    tools.append(
      profilePhotoTool(
        'left',
        profilePhotoText('profile.photoLeft', index, count),
        moved(index - 1, 'left'),
      ),
    );
  }
  if (index < count - 1) {
    tools.append(
      profilePhotoTool(
        'right',
        profilePhotoText('profile.photoRight', index, count),
        moved(index + 1, 'right'),
      ),
    );
  }
  tools.append(
    profilePhotoTool('remove', profilePhotoText('profile.photoRemove', index, count), () =>
      askProfilePhotoRemoval(index, call),
    ),
  );
  tile.append(tools);
  return tile;
};
// The open identity's first draft photo on its avatars. It is local, so no node hears of this.
// Other identities keep their letters: their stores stay closed.
/** @type {import('./ipc.js').BinaryResponse | null} */
let ownPhoto = null;
let ownPhotoRun = 0;
/** @param {HTMLElement} avatar */
const paintOwnAvatar = async (avatar) => {
  const photo = ownPhoto;
  const canvas = document.createElement('canvas');
  const drawn =
    !photo ||
    (await drawPhoto(canvas, photo, 'image/jpeg', PROFILE_TILE_SIDE).then(
      () => true,
      () => false,
    ));
  if (photo !== ownPhoto) return;
  setAvatarPhoto(avatar, photo && drawn ? canvas : null);
};
/** @param {HTMLElement} avatar */
controller.ownAvatar = (avatar) => {
  avatar.dataset.own = 'true';
  paintOwnAvatar(avatar);
};
/** @param {import('./ipc.js').Invoke} call @param {number[] | undefined} hash */
const showOwnAvatars = async (hash, call) => {
  const run = ++ownPhotoRun;
  const photo = hash ? await call('profile_photo', { hash }).catch(() => null) : null;
  if (run !== ownPhotoRun) return;
  ownPhoto = photo;
  await Promise.all([...domAll(document, '.avatar[data-own]', HTMLElement)].map(paintOwnAvatar));
};
document.addEventListener('dyapp:identity-change', () => {
  closePhotoRemoval();
  ownPhotoRun += 1;
  ownPhoto = null;
});
/** @param {number[][]} [hashes] @param {number[]} [revision] */
controller.showProfilePhotos = (hashes = [], call = invoke, revision = []) => {
  controller.ownAvatarsShown = showOwnAvatars(hashes[0], call);
  profilePhotoCount = hashes.length;
  profilePhotoRevision = revision;
  profilePhotoGrid.replaceChildren(
    ...hashes.map((hash, index) => profilePhotoTile(hash, index, hashes.length, call)),
  );
  controller.limitProfilePhotos();
};
profilePhotoAdd.addEventListener('click', () => profilePhotoInput.click());
profilePhotoInput.addEventListener('change', () => {
  const [file] = profilePhotoInput.files ?? [];
  if (file) controller.profilePhotoChanged = addProfilePhoto(file);
  profilePhotoInput.value = '';
});
profilePhotoDialog.addEventListener('close', () => {
  if (!profilePhotoDialog.open) closePhotoRemoval();
});
profilePhotoCancel.addEventListener('click', closePhotoRemoval);
profilePhotoDialog.addEventListener('cancel', closePhotoRemoval);
domById('photo-remove-confirm', HTMLButtonElement).addEventListener('click', () => {
  const remove = profilePhotoRemoval;
  if (!profilePhotoDialog.open || !remove) return;
  closePhotoRemoval();
  controller.profilePhotoChanged = remove();
});

export { addProfilePhoto };
