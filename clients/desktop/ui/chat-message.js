import { controller as chatsShareController } from './chats-share.js';
import { chatTime, controller as chatsUiController } from './chats-ui.js';
import { identityState } from './identity-state.js';
import { strings } from './locale.js';
import { controller as peerNamesController } from './peer-names.js';
import { controller as stickerMessagesController } from './sticker-messages.js';
import { controller as stickerRequestsController } from './sticker-requests.js';
import { controller as stickersController } from './stickers.js';
export const controller = {};

// Message rows use the chat controller's explicitly imported rendering helpers.
/**
 * @param {HTMLDivElement | HTMLParagraphElement} item
 * @param {import('./chat-state.js').ChatMessage} message
 * @param {string} peerId
 * @param {import('./chat-state.js').ChatHistory} history
 * @param {import('./ipc.js').Invoke} call
 */
function appendMessageBody(item, message, peerId, history, call) {
  const sticker = message.sticker && !message.deleted;
  const label = document.createElement('b');
  label.textContent =
    message.author === identityState.current.peerId
      ? strings['chats.you']
      : peerNamesController.peerName(peerId);
  const body = document.createElement('span');
  body.textContent = messageText(message);
  if (sticker) item.append(label);
  else item.append(label, document.createTextNode(': '));
  if (message.forwarded && !message.deleted) {
    const forwarded = document.createElement('span');
    forwarded.className = 'caption';
    forwarded.textContent = `${strings['chats.forwarded']} · `;
    item.append(forwarded);
  }
  item.append(messageContent(message, peerId, history, call, body));
  if (sticker && message.author === peerId && history.state === 'open')
    item.append(stickerRequestsController.stickerRequests.controls(message, peerId, call));
}

/** @param {import('./chat-state.js').ChatMessage} message */
function messageText(message) {
  if (message.deleted) return strings['chats.deleted'];
  if (message.sticker) return stickersController.stickerMessageText(message.sticker);
  return message.text || '';
}

/**
 * @param {HTMLSpanElement} body
 * @param {import('./chat-state.js').ChatMessage} message
 * @param {string} peerId
 * @param {import('./chat-state.js').ChatHistory} history
 * @param {import('./ipc.js').Invoke} call
 */
function messageContent(message, peerId, history, call, body) {
  const { photoBubble } = chatsUiController.chatMessageHelpers;
  if (message.photo) {
    return photoBubble(
      message,
      peerId,
      message.author !== identityState.current.peerId && history.state !== 'open',
      call,
    );
  }
  if (message.sticker && !message.deleted)
    return stickerMessagesController.stickerMessages.bubble(message, peerId, call);
  if (message.sharedPeer && !message.deleted)
    return chatsShareController.profileCard(message.sharedPeer, call);
  return body;
}

/**
 * @param {HTMLDivElement | HTMLParagraphElement} item
 * @param {import('./chat-state.js').ChatMessage} message
 */
function appendMessageMetadata(item, message) {
  const { chatLinkHosts } = chatsUiController.chatMessageHelpers;
  const hosts = chatLinkHosts(message.deleted ? '' : message.text);
  if (hosts.length) {
    const host = document.createElement('span');
    host.className = 'caption';
    host.textContent = ` · ${strings['chats.linkHost']}: ${hosts.join(', ')}`;
    item.append(host);
  }
  const status = message.read ? 'read' : message.delivery;
  if (message.author === identityState.current.peerId && status) {
    const delivery = document.createElement('span');
    delivery.className = 'caption';
    delivery.textContent = ` · ${strings[`chats.delivery.${status}`] || ''}`;
    item.append(delivery);
  }
  if (message.edited && !message.deleted) {
    const edited = document.createElement('span');
    edited.className = 'caption';
    edited.textContent = ` (${strings['chats.edited']})`;
    item.append(edited);
  }
}

/**
 * @param {HTMLDivElement | HTMLParagraphElement} item
 * @param {import('./chat-state.js').ChatMessage} message
 * @param {import('./chat-state.js').ChatHistory} history
 */
function appendMessageActions(item, message, history) {
  const { openEdit, openDelete, openForward } = chatsUiController.chatMessageHelpers;
  const sticker = message.sticker && !message.deleted;
  if (
    message.author === identityState.current.peerId &&
    !message.deleted &&
    !message.photo &&
    !message.sticker &&
    history.state !== 'closed'
  ) {
    const edit = document.createElement('button');
    edit.type = 'button';
    edit.className = 'btn quiet';
    edit.textContent = strings['chats.edit'];
    edit.addEventListener('click', () => openEdit(message));
    const remove = document.createElement('button');
    remove.type = 'button';
    remove.className = 'btn quiet';
    remove.textContent = strings['chats.delete'];
    remove.addEventListener('click', () => openDelete(message));
    item.append(document.createTextNode(' '), edit, document.createTextNode(' '), remove);
  }
  if (sticker && message.author === identityState.current.peerId && history.state !== 'closed') {
    const remove = document.createElement('button');
    remove.type = 'button';
    remove.className = 'btn quiet';
    remove.textContent = strings['chats.delete'];
    remove.addEventListener('click', () => openDelete(message));
    item.append(remove);
  }
  if (!message.deleted && message.text) {
    const forward = document.createElement('button');
    forward.type = 'button';
    forward.className = 'btn quiet';
    forward.textContent = strings['chats.forward'];
    forward.addEventListener('click', () => openForward(message));
    item.append(document.createTextNode(' '), forward);
  }
}

/**
 * @param {HTMLDivElement | HTMLParagraphElement} item
 * @param {import('./chat-state.js').ChatMessage} message
 */
function appendMessageTime(item, message) {
  const time = chatTime(message);
  if (time) {
    const stamp = document.createElement('time');
    stamp.className = 'caption';
    stamp.dateTime = new Date(message.sentAtMs).toISOString();
    stamp.textContent = time;
    item.append(document.createTextNode(' '), stamp);
  }
}

/**
 * @param {import('./chat-state.js').ChatMessage} message
 * @param {string} peerId
 * @param {import('./chat-state.js').ChatHistory} history
 * @param {import('./ipc.js').Invoke} call
 */
controller.chatMessage = (message, peerId, history, call) => {
  const sticker = message.sticker && !message.deleted;
  const item = document.createElement(sticker ? 'div' : 'p');
  item.className = sticker ? 'sticker-message' : 'msg';
  if (message.author === identityState.current.peerId) item.classList.add('out');
  if (message.deleted) item.classList.add('deleted');
  appendMessageBody(item, message, peerId, history, call);
  appendMessageMetadata(item, message);
  appendMessageActions(item, message, history);
  appendMessageTime(item, message);
  return item;
};
