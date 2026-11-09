import { who, peerLabel } from './app.js';
import { profileCardText } from './chats-share.js';
import { chatTime } from './formatting.js';
import { strings } from './locale.js';
import { controller as stickersController } from './stickers.js';

/** @param {import('./chat-state.js').ChatMessage} message */
const chatContentPreview = (message) => {
  if (message.sticker) return stickersController.stickerMessageText(message.sticker);
  if (message.photo) return strings['chats.photo'];
  if (message.sharedPeer) return profileCardText(message.sharedPeer);
  return message.text || '';
};
/** @param {import('./chat-state.js').ChatMessage | null} message */
const chatPreview = (message) => {
  if (!message) return strings['chats.noMessages'];
  if (message.deleted) return strings['chats.deleted'];
  const prefix = message.forwarded ? `${strings['chats.forwarded']} · ` : '';
  return `${prefix}${chatContentPreview(message)}`;
};
/** @param {import('./chat-state.js').ChatRow} row @param {() => void} onOpen */
export const chatRow = (row, onOpen) => {
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'list-button';
  button.dataset.peerId = row.peerId;
  const [avatar] = who(button, row.peerId, '', '');
  const content = document.createElement('span');
  content.className = 'grow';
  const name = document.createElement('b');
  name.textContent = peerLabel(name, row.peerId);
  content.append(name);
  if (row.state === 'requestReceived') {
    const marker = document.createElement('span');
    marker.className = 'badge';
    marker.textContent = strings['chats.state.requestReceived'];
    content.append(document.createTextNode(' '), marker);
  }
  if (row.activeRecently) {
    const active = document.createElement('span');
    active.className = 'caption';
    active.textContent = strings['chats.active'];
    content.append(document.createTextNode(' '), active);
  }
  if (row.muted) {
    const muted = document.createElement('span');
    muted.className = 'caption';
    muted.textContent = strings['chats.muted'];
    content.append(document.createTextNode(' '), muted);
  }
  const preview = document.createElement('span');
  preview.className = 'caption ellipsis';
  preview.textContent = chatPreview(row.lastMessage);
  content.append(document.createElement('br'), preview);
  const meta = document.createElement('span');
  meta.className = 'meta';
  const time = document.createElement('span');
  time.className = 'caption';
  time.textContent = chatTime(row.lastMessage);
  meta.append(time);
  if (row.unreadCount) {
    const unread = document.createElement('span');
    unread.className = 'badge';
    unread.textContent = String(row.unreadCount);
    unread.setAttribute('aria-label', `${strings['chats.unread']}: ${row.unreadCount}`);
    meta.append(document.createElement('br'), unread);
  }
  button.append(avatar, content, meta);
  button.addEventListener('click', onOpen);
  return button;
};
