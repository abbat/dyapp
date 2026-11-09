import { sizeText } from './formatting.js';
import { identityState, identityCurrent } from './identity-state.js';
import { strings } from './locale.js';
export const controller = {};

// The page's CSP admits no image URLs, so photo bytes are decoded onto a canvas.
const PREVIEW_SIDE = 240;
const ORIGINAL_SIDE = 480;
const MAX_KEPT_ORIGINALS = 16;
// Decrypted originals of the open chat, so a redraw keeps them. Memory only, dropped on leaving it.
const photoOriginals = new Map();
/** @type {string | null} */
let photoOriginalsPeer = null;
// The bubble drawn last for each photo of the open chat, the fetches running and the automatic
// tries already made.
const photoBoxes = new Map();
const photoFetches = new Set();
const photoAutoTried = new Set();
let photoAutoRunning = false;
const forgetPhotos = () => {
  photoOriginals.clear();
  photoBoxes.clear();
  photoAutoTried.clear();
};
/**
 * @param {HTMLCanvasElement} canvas
 * @param {number} width
 * @param {number} height
 * @param {number} side
 */
const fitCanvas = (canvas, width, height, side) => {
  const scale = Math.min(1, side / Math.max(width, height));
  canvas.width = Math.max(1, Math.round(width * scale));
  canvas.height = Math.max(1, Math.round(height * scale));
};
/**
 * @param {number | undefined} [side]
 * @param {HTMLCanvasElement} canvas
 * @param {BlobPart | number[]} bytes
 * @param {string} mime
 */
const drawPhoto = async (canvas, bytes, mime, side) => {
  const bitmap = await createImageBitmap(new Blob([bytes], { type: mime }));
  if (side) fitCanvas(canvas, bitmap.width, bitmap.height, side);
  canvas.getContext('2d').drawImage(bitmap, 0, 0, canvas.width, canvas.height);
  bitmap.close();
};
/** @param {import('./chat-state.js').ChatPhoto} photo */
const photoCaption = (photo) => {
  const parts = [strings['chats.photo'], photo.mime, sizeText(photo.size)];
  if (photo.kept) return parts.join(' · ');
  if (photo.hoursLeft === 0) parts.push(strings['chats.photoExpired']);
  else if (photo.hoursLeft) {
    parts.push(strings['chats.photoExpires'].replace('{hours}', photo.hoursLeft));
  }
  return parts.join(' · ');
};
/** @param {import('./chat-state.js').ChatMessage} message */
const photoKey = (message) => `${message.author}:${message.id}`;
/** @param {string} code */
const photoError = (code) =>
  strings[
    {
      notFound: 'chats.photoGone',
      corrupted: 'chats.photoBroken',
    }[code] || 'chats.photoFailed'
  ];
/** @param {string} key @param {string} name @param {() => void} click */
const photoButton = (key, name, click) => {
  const button = document.createElement('button');
  button.type = 'button';
  button.className = `btn quiet ${name}`;
  button.textContent = strings[key];
  button.addEventListener('click', click);
  return button;
};
// A mail pass redraws the chat under a running fetch, so results go to the bubble of that moment.
/**
 * @param {import('./chat-state.js').ChatMessage} message
 * @param {string} peerId
 * @param {import('./ipc.js').Invoke} call
 */
const downloadOriginal = async (message, peerId, call) => {
  const generation = identityState.generation;
  const key = photoKey(message);
  if (photoFetches.has(key) || photoOriginals.has(key)) return;
  const { mime } = message.photo;
  const latest = () =>
    identityCurrent(generation) && photoOriginalsPeer === peerId ? photoBoxes.get(key) : null;
  photoFetches.add(key);
  latest()?.showFetching();
  try {
    const bytes = await call('chat_photo_original', {
      peerId,
      author: message.author,
      id: message.id,
    });
    const box = latest();
    if (!box) return;
    await drawPhoto(box.canvas, bytes, mime, ORIGINAL_SIDE);
    if (photoOriginals.size >= MAX_KEPT_ORIGINALS) {
      photoOriginals.delete(photoOriginals.keys().next().value);
    }
    photoOriginals.set(key, bytes);
    box.canvas.hidden = false;
    box.showResult('', false);
  } catch (error) {
    // A rejected decode is not a string: the bytes passed the checks but the page cannot show them.
    const code = typeof error === 'string' ? error : 'corrupted';
    latest()?.showResult(photoError(code), code !== 'notFound' && code !== 'corrupted');
  } finally {
    photoFetches.delete(key);
  }
};
// The placeholder always names the photo; a preview that cannot be decoded just leaves it alone.
/**
 * @typedef {HTMLSpanElement & {
 *   canvas: HTMLCanvasElement,
 *   kept: boolean,
 *   ready?: Promise<void>,
 *   showFetching?: () => void,
 *   showResult?: (text: string, retry: boolean) => void,
 *   fetchPhoto?: () => Promise<void>
 * }} PhotoBox
 */
/** @param {import('./chat-state.js').ChatPhoto} photo */
const createPhotoBubble = (photo) => {
  const box = document.createElement('span');
  box.className = 'chat-photo';
  const caption = document.createElement('span');
  caption.className = 'caption';
  caption.textContent = photoCaption(photo);
  const status = document.createElement('span');
  status.className = 'caption photo-status';
  status.setAttribute('role', 'status');
  const canvas = document.createElement('canvas');
  fitCanvas(canvas, photo.width, photo.height, PREVIEW_SIDE);
  canvas.tabIndex = -1;
  canvas.setAttribute('role', 'img');
  canvas.setAttribute('aria-label', strings['chats.photo']);
  box.append(canvas, caption, status);
  /** @type {PhotoBox} */
  const initialized = Object.assign(box, { canvas, kept: photo.kept });
  return { box: initialized, canvas, status };
};
/**
 * @param {import('./chat-state.js').ChatMessage} message
 * @param {string} peerId
 * @param {boolean} blurred
 * @param {import('./ipc.js').Invoke} call
 */
const photoBubble = (message, peerId, blurred, call) => {
  const { photo } = message;
  const key = photoKey(message);
  const { box, canvas, status } = createPhotoBubble(photo);
  photoBoxes.set(key, box);
  const kept = photoOriginals.get(key);
  if (kept) {
    box.ready = drawPhoto(canvas, kept, photo.mime, ORIGINAL_SIDE);
    return box;
  }
  const fetchPhoto = () => downloadOriginal(message, peerId, call);
  const download = photoButton('chats.photoDownload', 'photo-download', fetchPhoto);
  box.showFetching = () => {
    download.disabled = true;
    status.textContent = strings['chats.photoDownloading'];
  };
  box.showResult = (text, retry) => {
    status.textContent = text;
    download.disabled = false;
    if (!retry) download.remove();
  };
  const offerDownload = () => {
    if ((photo.hoursLeft === 0 && !photo.kept) || photoOriginals.has(key)) return;
    box.append(download);
    if (photoFetches.has(key)) box.showFetching();
    else box.fetchPhoto = fetchPhoto;
  };
  const show = photoButton('chats.photoShow', 'photo-show', () => {
    box.classList.remove('blurred');
    show.remove();
    offerDownload();
    canvas.focus();
  });
  if (blurred && photo.preview.length) {
    box.classList.add('blurred');
    box.append(show);
  } else offerDownload();
  canvas.hidden = !photo.preview.length;
  box.ready = photo.preview.length
    ? drawPhoto(canvas, new Uint8Array(photo.preview), 'image/jpeg').catch(() => {
        canvas.hidden = true;
        if (box.classList.contains('blurred')) {
          show.remove();
          box.classList.remove('blurred');
          offerDownload();
        }
      })
    : Promise.resolve();
  return box;
};
// What the policy allows on this network, latest first, one at a time, among as many photos as
// stay in memory. Asked only while something waits. A refusal forgets the tries, so a switch to an
// unmetered network retries a failed fetch. A kept original opens either way: it costs no network.
/** @param {string} peerId @param {import('./ipc.js').Invoke} call */
const fetchAllowedPhotos = async (peerId, call) => {
  const waiting = () =>
    [...photoBoxes]
      .filter(([, box]) => box.isConnected)
      .slice(-MAX_KEPT_ORIGINALS)
      .reverse()
      .filter(([key, box]) => box.fetchPhoto && !photoAutoTried.has(key));
  if (photoAutoRunning || !waiting().length) return;
  photoAutoRunning = true;
  try {
    const allowed = await call('chat_photo_auto', { peerId }).catch(() => false);
    if (!allowed) photoAutoTried.clear();
    for (const [key, box] of waiting().filter(([, item]) => allowed || item.kept)) {
      if (photoOriginalsPeer !== peerId) return;
      photoAutoTried.add(key);
      await box.fetchPhoto();
    }
  } finally {
    photoAutoRunning = false;
  }
};
controller.chatPhotos = {
  photoBubble,
  fetchAllowed: fetchAllowedPhotos,
  /** @param {string | null} peerId */
  resetPeer(peerId, clear = false) {
    if (clear || photoOriginalsPeer !== peerId) forgetPhotos();
    photoOriginalsPeer = peerId;
  },
};

export { drawPhoto, fitCanvas };
