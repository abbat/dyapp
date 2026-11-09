import { chatRow as buildChatRow } from './chat-row.js';
import { byId as domById, all as domAll, eventDetail } from './dom.js';
import { chatTime, chatLinkHosts } from './formatting.js';
import { updateComposer } from './chat-composer.js';
import { statusMessage, peerLabel, showArea } from './app.js';
import { controller as chatMessageController } from './chat-message.js';
import { controller as chatPhotosController } from './chat-photos.js';
import { chatState } from './chat-state.js';
import { showForwardDialog, controller as chatsShareController } from './chats-share.js';
import {
  chatInput,
  chatSend,
  chatComposeStatus,
  editDialog,
  editCancel,
  editSave,
  editInput,
  editError,
  deleteDialog,
  deleteCancel,
  deleteConfirm,
  deleteError,
  forwardDialog,
  forwardCancel,
  forwardConfirm,
  forwardSelect,
  clearDialog,
  clearCancel,
  clearConfirm,
  clearError,
  chatRequests,
  chatList,
  newDeviceNotice,
  chatDetail,
  chatOverview,
  chatMessages,
  likedList,
  forwardError,
  likedButton,
} from './elements.js';
import { identityState, identityCurrent } from './identity-state.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
import { addKnownPeers } from './peer-labels.js';
import { closeRename, loadPeerNames, controller as peerNamesController } from './peer-names.js';
import { controller as peerPhotosController } from './peer-photos.js';
import { viewed, withPlaces, resultCard } from './search.js';
import { controller as stickerMessagesController } from './sticker-messages.js';
import { controller as stickerPickerController } from './sticker-picker.js';
import { controller as stickerRequestsController } from './sticker-requests.js';
import { progress } from './ui-progress.js';
export const controller = {};
/** @type {Promise<void> | undefined} */
controller.chatOpened = undefined;
/** @type {Promise<void> | undefined} */
controller.chatPhotosShown = undefined;
/** @type {Promise<void> | undefined} */
controller.chatRead = undefined;
/** @type {Promise<void> | undefined} */
controller.chatBlocked = undefined;
/** @type {Promise<void> | undefined} */
controller.chatMuted = undefined;
/** @type {Promise<void> | undefined} */
controller.chatCleared = undefined;
/** @type {Promise<void> | undefined} */
controller.chatSent = undefined;
/** @type {Promise<void> | undefined} */
controller.chatEdited = undefined;
/** @type {Promise<void> | undefined} */
controller.chatDeleted = undefined;
/** @type {Promise<void> | undefined} */
controller.profileShared = undefined;
/** @type {Promise<void> | undefined} */
controller.chatForwarded = undefined;

// Chats: local projection, requests, text, clearing and Liked you.
// Liked you: peers whose like this profile has not answered, fetched again on each visit.

/** @param {string} key */
const chatsMessage = (key, detail = '') =>
  statusMessage(domById('chats-status', HTMLParagraphElement), 'chats-open-network', key, detail);

/** @type {string | null} */
let chatRenderKey = null;
/** @type {import('./ipc.js').Invoke | null} */
let chatRenderCall = null;
/** @type {string | null} */
let newDevicePeer = null;
let chatSending = false;
let chatBlocking = false;
let chatsRequest = 0;
/** @type {import('./chat-state.js').ChatRow[]} */
let visibleChatRows = [];
let chatOpenRequest = 0;

/** @typedef {{ peerId: string, generation: number, request: number }} ChatTarget */
/** @typedef {ChatTarget & { groupId: number[], messageId: number[] }} MessageTarget */
/** @type {ChatTarget | null} */
let clearTarget = null;
let clearBusy = false;
/** @type {string | null} */
let clearedPeer = null;

/** @type {(MessageTarget & { text: string | null }) | null} */
let editTarget = null;
let editBusy = false;
const closeEdit = () => {
  if (editDialog.open) editDialog.close();
  editDialog.hidden = true;
  editTarget = null;
  editBusy = false;
  editCancel.disabled = false;
  editSave.disabled = false;
};
editDialog.addEventListener('close', () => {
  if (!editDialog.open) closeEdit();
});
editDialog.addEventListener('cancel', (event) => {
  if (editBusy) event.preventDefault();
});
editCancel.addEventListener('click', closeEdit);
const updateEdit = () => {
  const bytes = new TextEncoder().encode(editInput.value).length;
  editSave.disabled =
    editBusy ||
    !editTarget ||
    !editInput.value.trim() ||
    editInput.value === editTarget.text ||
    bytes > 65536;
  if (bytes > 65536) {
    editError.textContent = strings['chats.tooLong'];
    editError.hidden = false;
  }
};
editInput.addEventListener('input', () => {
  editError.hidden = true;
  editError.textContent = '';
  updateEdit();
});
/** @param {import('./chat-state.js').ChatMessage} message */
const openEdit = (message) => {
  if (
    !chatState.activeChat?.messages.includes(message) ||
    message.author !== identityState.current.peerId ||
    message.deleted
  )
    return;
  editTarget = {
    peerId: chatState.activeChat.peerId,
    groupId: message.groupId,
    messageId: message.id,
    text: message.text,
    generation: identityState.generation,
    request: chatOpenRequest,
  };
  editInput.value = message.text;
  editError.hidden = true;
  editError.textContent = '';
  updateEdit();
  editDialog.hidden = false;
  editDialog.showModal();
  editInput.focus();
};

/** @type {MessageTarget | null} */
let deleteTarget = null;
let deleteBusy = false;
const closeDelete = () => {
  if (deleteDialog.open) deleteDialog.close();
  deleteDialog.hidden = true;
  deleteTarget = null;
  deleteBusy = false;
  deleteCancel.disabled = false;
  deleteConfirm.disabled = false;
};
deleteDialog.addEventListener('close', () => {
  if (!deleteDialog.open) closeDelete();
});
deleteDialog.addEventListener('cancel', (event) => {
  if (deleteBusy) event.preventDefault();
});
deleteCancel.addEventListener('click', closeDelete);
/** @param {import('./chat-state.js').ChatMessage} message */
const openDelete = (message) => {
  if (
    !chatState.activeChat?.messages.includes(message) ||
    message.author !== identityState.current.peerId ||
    message.deleted
  )
    return;
  deleteTarget = {
    peerId: chatState.activeChat.peerId,
    groupId: message.groupId,
    messageId: message.id,
    generation: identityState.generation,
    request: chatOpenRequest,
  };
  deleteError.hidden = true;
  deleteError.textContent = '';
  deleteDialog.hidden = false;
  deleteDialog.showModal();
  deleteCancel.focus();
};

const closeForward = () => {
  if (forwardDialog.open) forwardDialog.close();
  forwardDialog.hidden = true;
  chatState.forwardTarget = null;
  chatState.forwardBusy = false;
  forwardCancel.disabled = false;
  forwardConfirm.disabled = false;
  forwardSelect.disabled = false;
  forwardSelect.replaceChildren();
};
forwardDialog.addEventListener('close', () => {
  if (!forwardDialog.open) closeForward();
});
forwardDialog.addEventListener('cancel', (event) => {
  if (chatState.forwardBusy) event.preventDefault();
});
forwardCancel.addEventListener('click', closeForward);
// showForwardDialog is in chats-share.js.
/** @param {import('./chat-state.js').ChatMessage} message */
const openForward = (message) => {
  if (!chatState.activeChat?.messages.includes(message) || message.deleted || !message.text) return;
  chatState.forwardTarget = {
    sourcePeerId: chatState.activeChat.peerId,
    sourceGroupId: message.groupId,
    sourceAuthor: message.author,
    sourceMessageId: message.id,
    text: message.text,
    generation: identityState.generation,
    request: chatOpenRequest,
  };
  showForwardDialog(
    'forward',
    visibleChatRows.filter(
      (row) => row.state === 'open' && row.peerId !== chatState.activeChat.peerId,
    ),
  );
};
const closeClear = () => {
  if (clearDialog.open) clearDialog.close();
  clearDialog.hidden = true;
  clearTarget = null;
  domById('chat-clear-name', HTMLParagraphElement).textContent = '';
  domById('chat-clear-peer', HTMLElement).textContent = '';
  clearBusy = false;
  clearCancel.disabled = false;
  clearConfirm.disabled = false;
};
clearDialog.addEventListener('close', () => {
  if (!clearDialog.open) closeClear();
});
clearDialog.addEventListener('cancel', (event) => {
  if (clearBusy) event.preventDefault();
});
clearCancel.addEventListener('click', closeClear);
domById('chat-clear', HTMLButtonElement).addEventListener('click', () => {
  if (!chatState.activeChat) return;
  clearTarget = {
    peerId: chatState.activeChat.peerId,
    generation: identityState.generation,
    request: chatOpenRequest,
  };
  domById('chat-clear-name', HTMLParagraphElement).textContent = peerNamesController.peerName(
    chatState.activeChat.peerId,
  );
  domById('chat-clear-peer', HTMLElement).textContent = chatState.activeChat.peerId;
  clearError.hidden = true;
  clearError.textContent = '';
  clearDialog.hidden = false;
  clearDialog.showModal();
  clearCancel.focus();
});
/** @param {import('./chat-state.js').ChatRow} row */
const chatRow = (row) =>
  buildChatRow(row, () => {
    controller.chatOpened = openChat(row.peerId);
  });
/** @param {import('./chat-state.js').ChatRow[]} rows */
const renderChatRows = (rows, call = invoke) => {
  visibleChatRows = rows;
  addKnownPeers(rows);
  const requests = rows.filter((row) => row.state === 'requestReceived');
  const conversations = rows.filter((row) => row.state !== 'requestReceived');
  chatRequests.replaceChildren(...requests.map(chatRow));
  domById('chats-requests-count', HTMLSpanElement).textContent = String(requests.length);
  chatList.replaceChildren(...conversations.map(chatRow));
  const cards = [
    ...domAll(chatRequests, ':scope > *', HTMLElement),
    ...domAll(chatList, ':scope > *', HTMLElement),
  ];
  controller.chatPhotosShown = peerPhotosController.showCardPhotos(
    cards.map((card) => ({
      draft: { photos: [peerNamesController.peerPhoto(card.dataset.peerId)] },
    })),
    cards,
    call,
  );
  if (!requests.length) chatRequests.textContent = strings['chats.noRequests'];
  if (!conversations.length) chatList.textContent = strings['chats.noConversations'];
};
/** @param {string | null} peerId */
const showNewDevice = (peerId) => {
  newDevicePeer = peerId;
  newDeviceNotice.hidden = peerId === null;
};
// A failed read keeps what is shown; the next mail pass asks again.
/** @param {import('./ipc.js').Invoke} call */
const linkedDevice = async (call) => {
  try {
    return (await call('linked_device')) === true;
  } catch {
    return false;
  }
};
/** @param {import('./ipc.js').Invoke} call @param {string} peerId */
const newDeviceFlag = async (peerId, call) => {
  try {
    return await call('contact_new_device', { peerId });
  } catch {
    return null;
  }
};
const acknowledgeDevice = async (call = invoke) => {
  const peerId = newDevicePeer;
  if (!peerId) return;
  const generation = identityState.generation;
  const shown = () => identityCurrent(generation) && newDevicePeer === peerId;
  const acknowledge = domById('chat-new-device-ack', HTMLButtonElement);
  acknowledge.disabled = true;
  try {
    await call('acknowledge_new_device', { peerId });
    if (!shown()) return;
    showNewDevice(null);
    // Reopening supersedes a load that read the flag before the acknowledgement.
    await openChat(peerId, call);
    (chatInput.disabled ? domById('chat-back', HTMLButtonElement) : chatInput).focus();
  } catch (error) {
    if (shown()) chatsMessage('chats.newDevice.failed', String(error));
  } finally {
    acknowledge.disabled = false;
  }
};
domById('chat-new-device-ack', HTMLButtonElement).addEventListener('click', () =>
  acknowledgeDevice(),
);
const closeChat = () => {
  chatRenderKey = null;
  chatRenderCall = null;
  stickerPickerController.stickerPicker?.close();
  stickerMessagesController.stickerMessages?.clear();
  stickerRequestsController.stickerRequests?.clear();
  chatOpenRequest += 1;
  chatPhotosController.chatPhotos.resetPeer(null, true);
  showNewDevice(null);
  closeClear();
  closeEdit();
  closeRename();
  closeDelete();
  closeForward();
  chatState.activeChat = null;
  chatSending = false;
  chatBlocking = false;
  chatInput.value = '';
  chatComposeStatus.textContent = '';
  updateComposer();
  chatDetail.hidden = true;
  chatOverview.hidden = false;
};
domById('chat-back', HTMLButtonElement).addEventListener('click', closeChat);
document.addEventListener('dyapp:area-change', (event) => {
  if (eventDetail(event) === 'chats-area') closeChat();
});
document.addEventListener('dyapp:identity-change', () => {
  chatsRequest += 1;
  visibleChatRows = [];
  clearedPeer = null;
  closeChat();
  chatRequests.replaceChildren();
  domById('chats-requests-count', HTMLSpanElement).textContent = '0';
  chatList.replaceChildren();
  chatMessages.replaceChildren();
  domById('chat-cleared', HTMLParagraphElement).hidden = true;
  likedList.replaceChildren();
  domById('chats-liked-count', HTMLSpanElement).textContent = '0';
});
// The next view or mail pass retries a receipt that did not go out.
/** @param {import('./ipc.js').Invoke} call @param {string} peerId */
const sendRead = async (peerId, call) => {
  try {
    await call('send_read_receipts', { peerId });
  } catch {
    /* the page shows nothing for a background receipt */
  }
};
/**
 * @param {(import('./chat-state.js').ChatHistory & { peerId: string; }) | null} previous
 * @param {string} peerId
 */
const resetChatSnapshot = (peerId, previous) => {
  if (newDevicePeer !== peerId) showNewDevice(null);
  chatPhotosController.chatPhotos.resetPeer(peerId);
  // A transparent refresh keeps the draft writable until the new snapshot changes its policy.
  if (previous?.peerId !== peerId) {
    if (previous) chatInput.value = '';
    chatState.activeChat = null;
    updateComposer();
    chatsMessage('chats.loading');
  }
};
/**
 * @param {import('./chat-state.js').ChatHistory} history @param {(import('./chat-state.js').ChatHistory & { peerId: string; }) | null} previous @param {number} generation @param {import('./ipc.js').Invoke} call
 * @param {string} peerId
 */
const renderOpenedChat = (peerId, history, previous, generation, call) => {
  const renderKey = JSON.stringify([
    generation,
    peerId,
    peerNamesController.peerName(peerId),
    history,
  ]);
  if (!previous || renderKey !== chatRenderKey || chatRenderCall !== call) {
    stickerMessagesController.stickerMessages?.clear();
    stickerRequestsController.stickerRequests?.clear();
    chatMessages.replaceChildren(
      ...history.messages.map((message) =>
        chatMessageController.chatMessage(message, peerId, history, call),
      ),
    );
  } else {
    // Mounted action callbacks retain these objects; their reference guards must still match.
    history.messages = previous.messages;
  }
  chatRenderKey = renderKey;
  chatRenderCall = call;
  chatPhotosController.chatPhotos.fetchAllowed(peerId, call);
  const cleared = clearedPeer === peerId && !history.messages.length;
  domById('chat-cleared', HTMLParagraphElement).hidden = !cleared;
  if (!history.messages.length && !cleared) chatMessages.textContent = strings['chats.noMessages'];
  const peer = domById('chat-peer', HTMLHeadingElement);
  peer.textContent = peerLabel(peer, peerId);
  domById('chat-state', HTMLParagraphElement).textContent =
    strings[`chats.state.${history.state}`] || '';
  domById('chat-presence', HTMLParagraphElement).hidden = !history.activeRecently;
  chatState.activeChat = { peerId, ...history };
  if (history.state !== 'open') stickerPickerController.stickerPicker?.leave(null);
  domById('chat-clear', HTMLButtonElement).hidden = history.state === 'none';
  domById('chat-rename', HTMLButtonElement).hidden = history.state === 'none';
  domById('chat-mute', HTMLButtonElement).hidden = history.state === 'none';
  updateComposer();
  chatOverview.hidden = true;
  chatDetail.hidden = false;
  chatsMessage('chats.ready');
};
/** @param {string} peerId */
const openChat = async (peerId, call = invoke) => {
  const previous = chatState.activeChat;
  stickerPickerController.stickerPicker?.leave(peerId);
  if (previous?.peerId !== peerId) {
    stickerMessagesController.stickerMessages?.clear();
    stickerRequestsController.stickerRequests?.clear();
  }
  const generation = identityState.generation;
  const request = ++chatOpenRequest;
  const currentChat = () => identityCurrent(generation) && request === chatOpenRequest;
  resetChatSnapshot(peerId, previous);
  try {
    const history = await call('chat_history', { peerId });
    if (!currentChat()) return;
    await loadPeerNames([peerId], call);
    if (!currentChat()) return;
    const newDevice = await newDeviceFlag(peerId, call);
    if (!currentChat()) return;
    if (newDevice !== null) showNewDevice(newDevice === true ? peerId : null);
    const linked = await linkedDevice(call);
    if (!currentChat()) return;
    domById('chat-linked-notice', HTMLParagraphElement).hidden = !linked;
    renderOpenedChat(peerId, history, previous, generation, call);
    // A mail pass behind a hidden window is not reading; visibilitychange re-runs this.
    if (history.state === 'none' || document.hidden) return;
    await call('mark_chat_read', { peerId });
    if (!currentChat()) return;
    controller.chatRead = sendRead(peerId, call);
    const rows = await call('chat_rows');
    if (!currentChat()) return;
    renderChatRows(rows);
  } catch (error) {
    if (currentChat()) chatsMessage('chats.failed', String(error));
  }
};
domById('view-message', HTMLButtonElement).addEventListener('click', () => {
  const found = viewed?.found;
  if (!found) return;
  showArea('chats-area');
  controller.chatOpened = openChat(found.peerId);
});
const blockChat = async (call = invoke) => {
  if (!chatState.activeChat || chatBlocking) return;
  const { peerId } = chatState.activeChat;
  const generation = identityState.generation;
  const request = chatOpenRequest;
  chatBlocking = true;
  updateComposer();
  let result;
  try {
    result = await call('block_peer', { peerId });
  } catch (error) {
    if (
      identityCurrent(generation) &&
      request === chatOpenRequest &&
      chatState.activeChat?.peerId === peerId
    ) {
      chatBlocking = false;
      updateComposer();
      chatsMessage('chats.blockFailed', String(error));
    }
    return;
  }
  if (!identityCurrent(generation)) return;
  // Invalidate a mailbox refresh that started before the local block was committed.
  const listRequest = ++chatsRequest;
  renderChatRows(visibleChatRows.filter((row) => row.peerId !== peerId));
  if (request === chatOpenRequest && chatState.activeChat?.peerId === peerId) closeChat();
  else {
    chatBlocking = false;
    updateComposer();
  }
  chatsMessage(`chats.blocked.${result.revocation}`);
  const rows = await call('chat_rows').catch(() => null);
  if (identityCurrent(generation) && listRequest === chatsRequest && rows) renderChatRows(rows);
};
for (const id of ['chat-block', 'request-block']) {
  domById(id, HTMLElement).addEventListener('click', () => {
    controller.chatBlocked = blockChat();
  });
}
const muteChat = async (call = invoke) => {
  if (!chatState.activeChat) return;
  const { peerId } = chatState.activeChat;
  const muted = !chatState.activeChat.muted;
  const generation = identityState.generation;
  try {
    await call('set_muted', { peerId, muted });
  } catch (error) {
    if (identityCurrent(generation) && chatState.activeChat?.peerId === peerId) {
      chatComposeStatus.textContent = `${strings['chats.muteFailed']} ${String(error)}`;
    }
    return;
  }
  if (!identityCurrent(generation)) return;
  // Invalidate a mailbox refresh that started before the flag was written.
  chatsRequest += 1;
  renderChatRows(
    visibleChatRows.map((row) => (row.peerId === peerId ? { ...row, muted } : row)),
    call,
  );
  if (chatState.activeChat?.peerId !== peerId) return;
  chatState.activeChat.muted = muted;
  chatComposeStatus.textContent = '';
  updateComposer();
};
domById('chat-mute', HTMLButtonElement).addEventListener('click', () => {
  controller.chatMuted = muteChat();
});
const clearChat = async (call = invoke) => {
  const target = clearTarget;
  if (!target || clearBusy) return;
  const currentClear = () =>
    clearTarget === target &&
    identityCurrent(target.generation) &&
    target.request === chatOpenRequest &&
    chatState.activeChat?.peerId === target.peerId;
  clearBusy = true;
  clearCancel.disabled = true;
  clearConfirm.disabled = true;
  try {
    await call('clear_chat', { peerId: target.peerId });
  } catch (error) {
    if (!currentClear()) return;
    clearBusy = false;
    clearCancel.disabled = false;
    clearConfirm.disabled = false;
    clearError.textContent = `${strings['chats.clearFailed']} ${String(error)}`;
    clearError.hidden = false;
    return;
  }
  if (!currentClear()) return;
  clearedPeer = target.peerId;
  chatInput.value = '';
  chatMessages.replaceChildren();
  domById('chat-cleared', HTMLParagraphElement).hidden = false;
  closeClear();
  await openChat(target.peerId, call);
};
clearConfirm.addEventListener('click', () => {
  controller.chatCleared = clearChat();
});
// One send of the open chat: holds the composer, tells a queued message from a failed one and
// redraws. `sent` runs before the redraw; `failure` words an unstaged error.
/**
 * @param {() => void} sent @param {import('./ipc.js').Invoke} call
 * @param {(peerId: string) => Promise<unknown>} send
 * @param {(error: string) => string} failure
 */
const sendInChat = async (send, sent, failure, call) => {
  const chat = chatState.activeChat;
  if (!chat) return;
  const { peerId, messages } = chat;
  const generation = identityState.generation;
  const request = chatOpenRequest;
  const currentSend = () =>
    identityCurrent(generation) &&
    request === chatOpenRequest &&
    chatState.activeChat?.peerId === peerId;
  const existing = new Set(messages.map((message) => JSON.stringify(message.id)));
  chatSending = true;
  chatComposeStatus.textContent = strings['chats.sending'];
  updateComposer();
  let result = 'sent';
  let errorText = '';
  try {
    await send(peerId);
  } catch (error) {
    errorText = String(error);
    // A failed node delivery may already have committed the text and its retryable outbox.
    // Show that local text instead of allowing a second copy from the same draft.
    const history = await call('chat_history', { peerId }).catch(() => null);
    if (!currentSend()) return;
    result = history?.messages.some(
      (message) =>
        message.author === identityState.current.peerId &&
        !existing.has(JSON.stringify(message.id)),
    )
      ? 'queued'
      : 'failed';
  }
  if (!currentSend()) return;
  chatSending = false;
  if (result === 'failed') {
    chatComposeStatus.textContent = failure(errorText);
    updateComposer();
    return;
  }
  sent();
  updateComposer();
  await openChat(peerId, call);
  if (identityCurrent(generation) && chatState.activeChat?.peerId === peerId) {
    chatComposeStatus.textContent = strings[`chats.${result}`];
  }
};
const sendChatText = async (call = invoke) => {
  if (!chatState.activeChat || chatSending || chatSend.disabled) return;
  const text = chatInput.value;
  await sendInChat(
    (peerId) => call('send_chat_text', { peerId, text }),
    () => {
      chatInput.value = '';
    },
    (error) => {
      if (error === 'peerUnavailable') return strings['chats.peerUnavailable'];
      // The peer's unlike landed after this chat was drawn.
      if (String(error).endsWith('chat is closed')) return strings['chats.closedNotice'];
      return `${strings['chats.sendFailed']} ${error}`;
    },
    call,
  );
};
domById('chat-composer', HTMLFormElement).addEventListener('submit', (event) => {
  event.preventDefault();
  controller.chatSent = sendChatText();
});
const editChatText = async (call = invoke) => {
  const target = editTarget;
  if (!target || editSave.disabled || editBusy) return;
  const text = editInput.value;
  const currentEdit = () =>
    editTarget === target &&
    identityCurrent(target.generation) &&
    target.request === chatOpenRequest &&
    chatState.activeChat?.peerId === target.peerId;
  editBusy = true;
  editCancel.disabled = true;
  updateEdit();
  let result = 'editSent';
  let errorText = '';
  try {
    await call('edit_chat_text', {
      peerId: target.peerId,
      groupId: target.groupId,
      messageId: target.messageId,
      text,
    });
  } catch (error) {
    errorText = String(error);
    // The edit may be committed locally even when network delivery failed.
    const history = await call('chat_history', { peerId: target.peerId }).catch(() => null);
    if (!currentEdit()) return;
    result = history?.messages.some(
      (message) =>
        message.author === identityState.current.peerId &&
        JSON.stringify(message.id) === JSON.stringify(target.messageId) &&
        JSON.stringify(message.groupId) === JSON.stringify(target.groupId) &&
        message.edited &&
        !message.deleted &&
        message.text === text,
    )
      ? 'editQueued'
      : 'failed';
  }
  if (!currentEdit()) return;
  editBusy = false;
  if (result === 'failed') {
    editCancel.disabled = false;
    editError.textContent = `${strings['chats.editFailed']} ${errorText}`;
    editError.hidden = false;
    updateEdit();
    return;
  }
  closeEdit();
  await openChat(target.peerId, call);
  if (identityCurrent(target.generation) && chatState.activeChat?.peerId === target.peerId) {
    chatComposeStatus.textContent = strings[`chats.${result}`];
  }
};
domById('chat-edit-form', HTMLFormElement).addEventListener('submit', (event) => {
  event.preventDefault();
  controller.chatEdited = editChatText();
});
const deleteChatText = async (call = invoke) => {
  const target = deleteTarget;
  if (!target || deleteBusy) return;
  const currentDelete = () =>
    deleteTarget === target &&
    identityCurrent(target.generation) &&
    target.request === chatOpenRequest &&
    chatState.activeChat?.peerId === target.peerId;
  deleteBusy = true;
  deleteCancel.disabled = true;
  deleteConfirm.disabled = true;
  let result = 'deleteSent';
  let errorText = '';
  try {
    await call('delete_chat_text', {
      peerId: target.peerId,
      groupId: target.groupId,
      messageId: target.messageId,
    });
  } catch (error) {
    errorText = String(error);
    // A failed delivery can follow a durable local delete and terminal placeholder.
    const history = await call('chat_history', { peerId: target.peerId }).catch(() => null);
    if (!currentDelete()) return;
    result = history?.messages.some(
      (message) =>
        message.author === identityState.current.peerId &&
        JSON.stringify(message.id) === JSON.stringify(target.messageId) &&
        JSON.stringify(message.groupId) === JSON.stringify(target.groupId) &&
        message.deleted &&
        message.text == null,
    )
      ? 'deleteQueued'
      : 'failed';
  }
  if (!currentDelete()) return;
  deleteBusy = false;
  if (result === 'failed') {
    deleteCancel.disabled = false;
    deleteConfirm.disabled = false;
    deleteError.textContent = `${strings['chats.deleteFailed']} ${errorText}`;
    deleteError.hidden = false;
    return;
  }
  closeDelete();
  await openChat(target.peerId, call);
  if (identityCurrent(target.generation) && chatState.activeChat?.peerId === target.peerId) {
    chatComposeStatus.textContent = strings[`chats.${result}`];
  }
};
deleteConfirm.addEventListener('click', () => {
  controller.chatDeleted = deleteChatText();
});
/** @param {string} errorText */
const showForwardFailure = (errorText) => {
  forwardCancel.disabled = false;
  forwardConfirm.disabled = false;
  forwardSelect.disabled = false;
  forwardError.textContent = `${strings['chats.forwardFailed']} ${errorText}`;
  forwardError.hidden = false;
};
const forwardChatText = async (call = invoke) => {
  const source = chatState.forwardTarget;
  const targetPeerId = forwardSelect.value;
  if (!source || chatState.forwardBusy || forwardConfirm.disabled || !targetPeerId) return;
  const currentForward = () =>
    chatState.forwardTarget === source &&
    identityCurrent(source.generation) &&
    source.request === chatOpenRequest &&
    chatState.activeChat?.peerId === source.sourcePeerId;
  chatState.forwardBusy = true;
  forwardCancel.disabled = true;
  forwardConfirm.disabled = true;
  forwardSelect.disabled = true;
  const before = await call('chat_history', { peerId: targetPeerId }).catch(() => null);
  if (!currentForward()) return;
  const oldIds = new Set(
    before?.messages.map((message) => JSON.stringify([message.groupId, message.id])) || [],
  );
  let result = 'forwardSent';
  let errorText = '';
  try {
    await call('forward_chat_text', {
      sourcePeerId: source.sourcePeerId,
      sourceGroupId: source.sourceGroupId,
      sourceAuthor: source.sourceAuthor,
      sourceMessageId: source.sourceMessageId,
      targetPeerId,
    });
  } catch (error) {
    errorText = String(error);
    const after = await call('chat_history', { peerId: targetPeerId }).catch(() => null);
    if (!currentForward()) return;
    result =
      before &&
      after?.messages.some(
        (message) =>
          message.author === identityState.current.peerId &&
          message.forwarded &&
          !message.deleted &&
          message.text === source.text &&
          !oldIds.has(JSON.stringify([message.groupId, message.id])),
      )
        ? 'forwardQueued'
        : 'failed';
  }
  if (!currentForward()) return;
  chatState.forwardBusy = false;
  if (result === 'failed') {
    showForwardFailure(errorText);
    return;
  }
  closeForward();
  await openChat(targetPeerId, call);
  if (identityCurrent(source.generation) && chatState.activeChat?.peerId === targetPeerId) {
    chatComposeStatus.textContent = strings[`chats.${result}`];
  }
};
domById('chat-forward-form', HTMLFormElement).addEventListener('submit', (event) => {
  event.preventDefault();
  if (chatState.forwardTarget?.share)
    controller.profileShared = chatsShareController.shareProfile();
  else controller.chatForwarded = forwardChatText();
});
/** @param {string | null} receiveError @param {number} foundCount @param {number} rowCount */
const showChatsStatus = (receiveError, foundCount, rowCount) => {
  if (receiveError === 'notConnected') return chatsMessage('chats.notConnected');
  if (receiveError) return chatsMessage('chats.receiveFailed', receiveError);
  if (foundCount) return chatsMessage('chats.count', String(foundCount));
  if (rowCount) return chatsMessage('chats.ready');
  chatsMessage('chats.none');
};
controller.showChats = async (call = invoke) => {
  const generation = identityState.generation;
  const request = ++chatsRequest;
  const currentChats = () => identityCurrent(generation) && request === chatsRequest;
  chatsMessage('chats.loading');
  // Keep the device-local projection visible even if mailbox or profile lookup fails.
  const receiveError = await call('receive_mail').then(
    () => null,
    (error) => String(error),
  );
  if (!currentChats()) return;
  let rows;
  try {
    rows = await call('chat_rows');
  } catch (error) {
    if (currentChats()) chatsMessage('chats.rowsFailed', String(error));
    return;
  }
  if (!currentChats()) return;
  await loadPeerNames(
    rows.map((row) => row.peerId),
    call,
  );
  if (!currentChats()) return;
  renderChatRows(rows, call);
  try {
    const found = await withPlaces(await call('liked_you'));
    if (!currentChats()) return;
    addKnownPeers(found);
    likedList.replaceChildren(
      ...found.map((item, index) => resultCard(item, `liked-${index + 1}`)),
    );
    peerPhotosController.showCardPhotos(found, [...likedList.children], call);
    domById('chats-liked-count', HTMLSpanElement).textContent = String(found.length);
    showChatsStatus(receiveError, found.length, rows.length);
  } catch (error) {
    if (!currentChats()) return;
    const code = String(error);
    if (code === 'notConnected') chatsMessage('chats.notConnected');
    else chatsMessage('chats.failed', code);
  }
};
// The runtime reports each background pass: its changed-event count, or null after a failure,
// which stays quiet because the next pass retries. A failure can follow a commit, such as a lost
// ACK, so the list still follows the local projection, and with no mail a presence can expire.
/** @param {number | null} changed */
controller.mailUpdated = async (changed, call = invoke) => {
  const generation = identityState.generation;
  if (domById('chats-area', HTMLElement).hidden) return;
  stickerRequestsController.stickerRequests?.refresh(changed ?? 0);
  // Retry and signed receipts can change delivery status without a new visible chat event.
  if (
    chatState.activeChat &&
    !editDialog.open &&
    !deleteDialog.open &&
    !clearDialog.open &&
    !chatSending &&
    !chatBlocking
  ) {
    await openChat(chatState.activeChat.peerId, call);
  } else if (!changed) {
    const request = chatsRequest;
    const rows = await call('chat_rows').catch(() => null);
    // An unchanged list is left alone so an offline poll cannot take keyboard focus from a row.
    if (
      rows &&
      identityCurrent(generation) &&
      request === chatsRequest &&
      JSON.stringify(rows) !== JSON.stringify(visibleChatRows)
    )
      renderChatRows(rows);
  } else {
    progress.chatsDone = controller.showChats(call);
  }
};
document.addEventListener('visibilitychange', () => {
  if (!document.hidden) controller.mailUpdated(0);
});
controller.chatMessageHelpers = {
  openEdit,
  openDelete,
  openForward,
  chatLinkHosts,
  photoBubble: chatPhotosController.chatPhotos.photoBubble,
};
// Sent on launch and on focus, never on a timer, so it follows real use; the Store keeps the hour.
controller.announcePresence = async (call = invoke) => {
  if (document.hidden) return;
  try {
    await call('announce_presence');
  } catch {
    /* the page shows nothing for a background signal */
  }
};
window.addEventListener('focus', () => {
  controller.announcePresence();
});
likedButton.addEventListener('click', () => {
  const open = likedButton.getAttribute('aria-expanded') !== 'true';
  likedButton.setAttribute('aria-expanded', String(open));
  likedList.hidden = !open;
});

export {
  acknowledgeDevice,
  blockChat,
  chatLinkHosts,
  chatSending,
  chatBlocking,
  chatTime,
  clearChat,
  closeChat,
  closeClear,
  closeDelete,
  closeForward,
  deleteChatText,
  editChatText,
  forwardChatText,
  muteChat,
  openChat,
  openDelete,
  renderChatRows,
  sendChatText,
  sendInChat,
  updateComposer,
  visibleChatRows,
};
