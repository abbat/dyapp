import { byId as domById } from './dom.js';
import { chatState } from './chat-state.js';
import { openChat, chatSending, chatBlocking } from './chats-ui.js';
import { chatInput, chatSend, chatComposeStatus } from './elements.js';
import { identityState, identityCurrent } from './identity-state.js';
import { strings } from './locale.js';
import { controller as stickerPickerController } from './sticker-picker.js';
import { progress } from './ui-progress.js';

export const updateComposer = () => {
  const waiting = chatState.activeChat?.state === 'requestSent';
  const closed = chatState.activeChat?.state === 'closed';
  const writable =
    Boolean(chatState.activeChat) && !waiting && !closed && !chatSending && !chatBlocking;
  const bytes = new TextEncoder().encode(chatInput.value).length;
  chatInput.disabled = !writable;
  chatSend.disabled = !writable || !chatInput.value.trim() || bytes > 65536;
  chatSend.hidden = !chatInput.value.trim();
  const attach = domById('chat-attach', HTMLButtonElement);
  attach.hidden = false;
  attach.disabled = !writable || chatState.activeChat?.state !== 'open';
  domById('chat-stickers', HTMLButtonElement).disabled = attach.disabled;
  domById('chat-first-notice', HTMLParagraphElement).hidden = !waiting;
  domById('chat-closed-notice', HTMLParagraphElement).hidden = !closed;
  domById('chat-request-notice', HTMLDivElement).hidden =
    chatState.activeChat?.state !== 'requestReceived';
  domById('chat-block', HTMLButtonElement).hidden =
    chatState.activeChat?.state === 'requestReceived';
  for (const id of ['chat-block', 'request-block', 'chat-clear', 'chat-mute']) {
    domById(id, HTMLButtonElement).disabled = chatSending || chatBlocking;
  }
  const mute = domById('chat-mute', HTMLButtonElement);
  mute.dataset.i18n = chatState.activeChat?.muted ? 'chats.unmute' : 'chats.mute';
  mute.textContent = strings[mute.dataset.i18n];
  if (bytes > 65536) chatComposeStatus.textContent = strings['chats.tooLong'];
};
chatInput.addEventListener('input', () => {
  chatComposeStatus.textContent = '';
  updateComposer();
});

// Keep keyboard submission on the same validated send path as the button.
chatInput.addEventListener('keydown', (event) => {
  if (event.key !== 'Enter' || event.shiftKey || event.isComposing) return;
  event.preventDefault();
  if (!chatSend.disabled) domById('chat-composer', HTMLFormElement).requestSubmit();
});
domById('chat-stickers', HTMLButtonElement).addEventListener('click', () => {
  if (chatInput.disabled || chatState.activeChat?.state !== 'open') return;
  const peerId = chatState.activeChat.peerId;
  const generation = identityState.generation;
  progress.stickersShown = stickerPickerController.stickerPicker.open({
    peerId,
    onSent: async () => {
      if (!identityCurrent(generation) || chatState.activeChat?.peerId !== peerId) return;
      await openChat(peerId);
      if (identityCurrent(generation) && chatState.activeChat?.peerId === peerId)
        chatComposeStatus.textContent = strings['stickerPicker.queued'];
    },
  });
});
