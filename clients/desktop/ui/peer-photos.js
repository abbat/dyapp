import { byId as domById, find as domFind, query as domQuery } from './dom.js';
import { drawPhoto } from './chat-photos.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
import { shortPeer } from './peer-labels.js';
import { viewStatus } from './search.js';
export const controller = {};

// Other people's photos. Lists ask for thumbnails and the profile view for full images, through a
// memory cache in the desktop core. Whatever fails leaves the letters, and the next view retries.
const PEER_PHOTO_SIDE = 1600;
const viewMain = domFind(document, '#view-photos .main', HTMLElement);
const viewThumbs = domFind(document, '#view-photos .thumbs', HTMLElement);
const viewLoad = domById('view-photo-load', HTMLButtonElement);
let viewPhotoRun = 0;
/** @type {() => boolean | undefined | Promise<boolean | undefined>} */
let loadViewPhoto = () => undefined;

/** @param {boolean} full */
const fetchPeerPhotos = async (hashes, full, call) => {
  try {
    await call('fetch_peer_photos', { hashes, full });
  } catch {
    // The drawing below finds nothing and keeps the letters.
  }
};
const peerCanvas = async (hash, call) => {
  const canvas = document.createElement('canvas');
  try {
    await drawPhoto(canvas, await call('peer_photo', { hash }), 'image/jpeg', PEER_PHOTO_SIDE);
    return canvas;
  } catch {
    return null;
  }
};
/** @param {import('./ipc.js').Invoke} call */
const peerPhotoAuto = async (call) => {
  try {
    return (await call('peer_photo_auto')) === true;
  } catch {
    return false;
  }
};
const peerPhotoLabel = (index, count) =>
  strings['profile.photoLabel'].replace('{n}', index + 1).replace('{count}', count);

// ponytail: the liked list redraws on each mail pass with news, so a missing thumbnail connects
// again then; remember misses here if that shows up in traffic.
controller.showCardPhotos = async (found, cards, call = invoke) => {
  const wanted = found
    .map((item, index) => [item.draft.photos?.[0], cards[index]])
    .filter(([hash]) => hash?.length);
  if (!wanted.length) return;
  await fetchPeerPhotos(
    wanted.map(([hash]) => hash),
    false,
    call,
  );
  await Promise.all(
    wanted.map(async ([hash, card]) => {
      const canvas = await peerCanvas(hash, call);
      if (canvas) domQuery(card, '.avatar', HTMLElement).append(canvas);
    }),
  );
};

/** @param {number} index @param {string} letters */
const thumbButton = (canvas, index, count, letters) => {
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'avatar';
  const label = peerPhotoLabel(index, count);
  // The name must hold any text on the button, so the letters stay only in place of a photo.
  button.setAttribute('aria-label', canvas ? label : `${letters}: ${label}`);
  if (canvas) button.append(canvas);
  else button.textContent = letters;
  return button;
};

// Returns undefined when another choice takes over, otherwise whether the full image is showing.
const foundPhotoChoice = ({ thumbnails, originals, buttons, showMain, auto, call }) => {
  /** @param {boolean} load */
  const choose = async (index, load) => {
    const choice = ++viewPhotoRun;
    const current = () => choice === viewPhotoRun;
    buttons.forEach((button, at) => {
      if (at === index) button.setAttribute('aria-current', 'true');
      else button.removeAttribute('aria-current');
    });
    const thumbnail = await peerCanvas(thumbnails[index], call);
    if (!current()) return undefined;
    showMain(thumbnail, index);
    const hash = originals[index] ?? [];
    if (!hash.length) {
      viewLoad.hidden = true;
      return false;
    }
    let full = await peerCanvas(hash, call);
    if (!full && load) {
      await fetchPeerPhotos([hash], true, call);
      full = await peerCanvas(hash, call);
    }
    if (!current()) return undefined;
    if (full) showMain(full, index);
    viewLoad.hidden = Boolean(full) || auto;
    loadViewPhoto = () => choose(index, true);
    return Boolean(full);
  };

  return choose;
};

controller.showFoundPhotos = async (found, call = invoke) => {
  const letters = shortPeer(found.peerId).slice(1, 3);
  const thumbnails = found.draft.photos ?? [];
  const originals = found.fullPhotos ?? [];
  const count = thumbnails.length;
  const showMain = (canvas, index) => {
    if (canvas) {
      viewMain.replaceChildren(canvas);
      viewMain.setAttribute('role', 'img');
      viewMain.setAttribute('aria-label', peerPhotoLabel(index, count));
      viewMain.removeAttribute('aria-hidden');
    } else {
      viewMain.textContent = letters;
      viewMain.removeAttribute('role');
      viewMain.removeAttribute('aria-label');
      viewMain.setAttribute('aria-hidden', 'true');
    }
  };
  // Runs before the first await, so the last profile's photo never shows under this name.
  const first = ++viewPhotoRun;
  showMain(null);
  viewThumbs.replaceChildren();
  viewLoad.hidden = true;
  if (!count) return;
  await fetchPeerPhotos(thumbnails, false, call);
  const [auto, canvases] = await Promise.all([
    peerPhotoAuto(call),
    Promise.all(thumbnails.map((hash) => peerCanvas(hash, call))),
  ]);
  if (first !== viewPhotoRun) return;
  const buttons = canvases.map((canvas, index) => thumbButton(canvas, index, count, letters));
  const choose = foundPhotoChoice({ thumbnails, originals, buttons, showMain, auto, call });
  buttons.forEach((button, index) => {
    button.addEventListener('click', () => {
      controller.peerPhotoChosen = choose(index, auto);
    });
  });
  if (count > 1) viewThumbs.replaceChildren(...buttons);
  await choose(0, auto);
};

viewLoad.addEventListener('click', () => {
  controller.peerPhotoLoaded = (async () => {
    viewStatus.textContent = '';
    viewLoad.disabled = true;
    viewLoad.textContent = strings['view.photoLoading'];
    const shown = await loadViewPhoto();
    viewLoad.disabled = false;
    viewLoad.textContent = strings['view.photoLoad'];
    if (shown) viewMain.focus();
    else if (shown === false) {
      viewStatus.textContent = strings['view.photoFailed'];
      viewLoad.focus();
    }
  })();
});
