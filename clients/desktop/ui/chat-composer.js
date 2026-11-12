import { byId as domById } from './dom.js';
import { chatState } from './chat-state.js';
import { openChat, chatSending, chatBlocking } from './chats-ui.js';
import { chatInput, chatSend, chatComposeStatus } from './elements.js';
import { identityState, identityCurrent } from './identity-state.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
import { controller as stickerPickerController } from './sticker-picker.js';
import { progress } from './ui-progress.js';

let composerRun = 0;
/**
 * Text and phase rules come from the core so every client agrees on what can be sent.
 * @param {import('./chat-state.js').ConversationState | null} state
 * @param {string} text
 */
export const assess = (state, text) =>
  invoke('chat_composer', { state, text }).catch(() => ({
    tooLong: false,
    blank: true,
    sendable: false,
  }));

export const updateComposer = () => {
  const waiting = chatState.activeChat?.state === 'requestSent';
  const closed = chatState.activeChat?.state === 'closed';
  const writable =
    Boolean(chatState.activeChat) && !waiting && !closed && !chatSending && !chatBlocking;
  chatInput.disabled = !writable;
  if (!writable) chatSend.disabled = true;
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
  const run = ++composerRun;
  const shown = assess(chatState.activeChat?.state ?? null, chatInput.value).then((policy) => {
    if (run !== composerRun) return;
    chatSend.disabled = !writable || !policy.sendable;
    chatSend.hidden = policy.blank;
    if (policy.tooLong) chatComposeStatus.textContent = strings['chats.tooLong'];
  });
  progress.composerShown = shown;
  return shown;
};
chatInput.addEventListener('input', () => {
  chatComposeStatus.textContent = '';
  updateComposer();
});

// Keep keyboard submission on the same validated send path as the button.
chatInput.addEventListener('keydown', (event) => {
  if (event.key !== 'Enter' || event.shiftKey || event.isComposing) return;
  event.preventDefault();
  // The keystroke may arrive before the core assessed the latest text.
  void progress.composerShown?.then(() => {
    if (!chatSend.disabled) domById('chat-composer', HTMLFormElement).requestSubmit();
  });
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
