import { fitCanvas } from './chat-photos.js';
import { chatState } from './chat-state.js';
import { chatSending, sendInChat } from './chats-ui.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
export const controller = {};

// Sends a picked photo from the open chat through the imported chat controller.
// A canvas writes a bare JPEG, so EXIF, GPS, colour profiles and anything appended to the file
// never leave the computer. It also turns a rotated photo upright and fits the node limit.
const MAX_PHOTO_FILE_BYTES = 64 * 1024 * 1024;
const MAX_PHOTO_PIXELS = 64_000_000;
// Under the 6 MiB node limit once the encrypted chunks add their tags.
const MAX_PHOTO_BYTES = 6_000_000;
const PHOTO_SIDES = [2560, 1920, 1280];
const PHOTO_QUALITIES = [0.85];
const MAX_PREVIEW_BYTES = 24 * 1024;
const PREVIEW_SIDES = [320, 160];
const PREVIEW_QUALITIES = [0.8, 0.7, 0.6, 0.5, 0.4];
const chatPhotoInput = document.getElementById('chat-photo-input');

const encodePhoto = (bitmap, side, quality) =>
  new Promise((resolve) => {
    const canvas = document.createElement('canvas');
    fitCanvas(canvas, bitmap.width, bitmap.height, side);
    const context = canvas.getContext('2d');
    context.fillStyle = '#fff';
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    canvas.toBlob(
      (blob) => resolve({ blob, width: canvas.width, height: canvas.height }),
      'image/jpeg',
      quality,
    );
  });
const fittedPhoto = async (bitmap, sides, qualities, limit) => {
  for (const side of sides) {
    for (const quality of qualities) {
      const made = await encodePhoto(bitmap, side, quality);
      if (made.blob && made.blob.size <= limit) return made;
    }
  }
  return null;
};
const photoBytes = async (blob) => Array.from(new Uint8Array(await blob.arrayBuffer()));
const preparePhoto = async (
  file,
  sides = PHOTO_SIDES,
  qualities = PHOTO_QUALITIES,
  limit = MAX_PHOTO_BYTES,
) => {
  if (!['image/png', 'image/jpeg'].includes(file.type)) return Promise.reject('unsupported');
  if (file.size > MAX_PHOTO_FILE_BYTES) return Promise.reject('tooLarge');
  const bitmap = await createImageBitmap(file).catch(() => null);
  if (!bitmap) return Promise.reject('unsupported');
  try {
    if (bitmap.width * bitmap.height > MAX_PHOTO_PIXELS) return Promise.reject('tooLarge');
    const photo = await fittedPhoto(bitmap, sides, qualities, limit);
    const preview = await fittedPhoto(bitmap, PREVIEW_SIDES, PREVIEW_QUALITIES, MAX_PREVIEW_BYTES);
    if (!photo || !preview) return Promise.reject('tooLarge');
    return {
      photo: await photoBytes(photo.blob),
      preview: await photoBytes(preview.blob),
      width: photo.width,
      height: photo.height,
    };
  } finally {
    bitmap.close();
  }
};
const photoSendError = (code) => {
  const key = {
    full: 'chats.photoFull',
    tooLarge: 'chats.photoTooLarge',
    unsupported: 'chats.photoUnsupported',
    corrupted: 'chats.photoUnsupported',
    notConnected: 'chats.notConnected',
    peerUnavailable: 'chats.peerUnavailable',
  }[code];
  return key ? strings[key] : `${strings['chats.sendFailed']} ${code}`;
};
const sendChatPhoto = async (file, call = invoke) => {
  if (!chatState.activeChat || chatSending) return;
  await sendInChat(
    async (peerId) => call('send_chat_photo', { peerId, ...(await preparePhoto(file)) }),
    () => {},
    photoSendError,
    call,
  );
};
document.getElementById('chat-attach').addEventListener('click', () => chatPhotoInput.click());
chatPhotoInput.addEventListener('change', () => {
  const [file] = chatPhotoInput.files;
  if (file) controller.chatPhotoSent = sendChatPhoto(file);
  chatPhotoInput.value = '';
});

export { preparePhoto, sendChatPhoto };
