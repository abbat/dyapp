import { byId as domById } from './dom.js';
import { chatState } from './chat-state.js';
import { chatSending, sendInChat } from './chats-ui.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
const controller = {};
/** @type {Promise<void> | undefined} */
controller.chatPhotoSent = undefined;

// Sends a picked photo from the open chat through the imported chat controller.
// The page only decodes the file upright at the size the core picks; the core fits and encodes
// a bare JPEG, so EXIF, GPS, colour profiles and anything appended to the file never leave.
const chatPhotoInput = domById('chat-photo-input', HTMLInputElement);

/**
 * Decodes a picked file to straight RGBA at the core's decode size.
 * @param {File} file
 * @param {'profile' | 'chat'} purpose
 * @param {import('./ipc.js').Invoke} call
 */
const photoPixels = async (file, purpose, call) => {
  if (file.size > (await call('photo_picked_byte_limit'))) return Promise.reject('tooLarge');
  const bitmap = await createImageBitmap(file).catch(() => null);
  if (!bitmap) return Promise.reject('unsupported');
  try {
    const { width, height } = await call('photo_decode_plan', {
      mime: file.type,
      width: bitmap.width,
      height: bitmap.height,
      bytes: file.size,
      purpose,
    });
    const canvas = document.createElement('canvas');
    canvas.width = width;
    canvas.height = height;
    const context = canvas.getContext('2d');
    if (!context) throw new Error('Photo canvas unavailable');
    context.imageSmoothingQuality = 'high';
    context.drawImage(bitmap, 0, 0, width, height);
    const pixels = new Uint8Array(context.getImageData(0, 0, width, height).data.buffer);
    return { pixels, headers: { 'dyapp-width': String(width), 'dyapp-height': String(height) } };
  } finally {
    bitmap.close();
  }
};
/** @param {unknown} code */
const photoSendError = (code) => {
  const key = {
    full: 'chats.photoFull',
    tooLarge: 'chats.photoTooLarge',
    unsupported: 'chats.photoUnsupported',
    corrupted: 'chats.photoUnsupported',
    notConnected: 'chats.notConnected',
    peerUnavailable: 'chats.peerUnavailable',
  }[String(code)];
  return key ? strings[key] : `${strings['chats.sendFailed']} ${code}`;
};
/** @param {File} file */
const sendChatPhoto = async (file, call = invoke) => {
  if (!chatState.activeChat || chatSending) return;
  await sendInChat(
    async (peerId) => {
      const { pixels, headers } = await photoPixels(file, 'chat', call);
      return call('send_chat_photo', pixels, { headers: { ...headers, 'dyapp-peer-id': peerId } });
    },
    () => {},
    photoSendError,
    call,
  );
};
domById('chat-attach', HTMLButtonElement).addEventListener('click', () => chatPhotoInput.click());
chatPhotoInput.addEventListener('change', () => {
  const [file] = chatPhotoInput.files ?? [];
  if (file) controller.chatPhotoSent = sendChatPhoto(file);
  chatPhotoInput.value = '';
});

export { photoPixels, sendChatPhoto };
