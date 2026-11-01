// Chats: local projection, requests, text, clearing and Liked you.
// Loaded after search.js; both classic scripts share their page lexical scope.
// Liked you: peers whose like this profile has not answered, fetched again on each visit.
const likedButton = document.getElementById('chats-liked');
const likedList = document.getElementById('chats-liked-list');
const chatsMessage = (key, detail = '') =>
  statusMessage(document.getElementById('chats-status'), 'chats-open-network', key, detail);
const chatOverview = document.getElementById('chats-overview');
const chatDetail = document.getElementById('chat-detail');
const chatRequests = document.getElementById('chats-requests');
const chatList = document.getElementById('chat-list');
const chatMessages = document.getElementById('chat-messages');
const chatInput = document.getElementById('chat-input');
const chatSend = document.getElementById('chat-send');
const chatComposeStatus = document.getElementById('chat-compose-status');
let activeChat = null;
let chatSending = false;
let chatBlocking = false;
const updateComposer = () => {
  const waiting = activeChat?.state === 'requestSent';
  const writable = Boolean(activeChat) && !waiting && !chatSending && !chatBlocking;
  const bytes = new TextEncoder().encode(chatInput.value).length;
  chatInput.disabled = !writable;
  chatSend.disabled = !writable || !chatInput.value.trim() || bytes > 65536;
  document.getElementById('chat-first-notice').hidden = !waiting;
  document.getElementById('chat-request-notice').hidden = activeChat?.state !== 'requestReceived';
  document.getElementById('chat-block').hidden = activeChat?.state === 'requestReceived';
  for (const id of ['chat-block', 'request-block', 'chat-clear']) {
    document.getElementById(id).disabled = chatSending || chatBlocking;
  }
  if (bytes > 65536) chatComposeStatus.textContent = window.strings['chats.tooLong'];
};
chatInput.addEventListener('input', () => {
  chatComposeStatus.textContent = '';
  updateComposer();
});
let chatsRequest = 0;
let visibleChatRows = [];
let chatOpenRequest = 0;
const clearDialog = document.getElementById('chat-clear-dialog');
const clearError = document.getElementById('chat-clear-error');
const clearCancel = document.getElementById('chat-clear-cancel');
const clearConfirm = document.getElementById('chat-clear-confirm');
let clearTarget = null;
let clearBusy = false;
let clearedPeer = null;
const editDialog = document.getElementById('chat-edit-dialog');
const editInput = document.getElementById('chat-edit-input');
const editError = document.getElementById('chat-edit-error');
const editCancel = document.getElementById('chat-edit-cancel');
const editSave = document.getElementById('chat-edit-save');
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
editDialog.addEventListener('close', () => { if (!editDialog.open) closeEdit(); });
editDialog.addEventListener('cancel', (event) => {
  if (editBusy) event.preventDefault();
});
editCancel.addEventListener('click', closeEdit);
const updateEdit = () => {
  const bytes = new TextEncoder().encode(editInput.value).length;
  editSave.disabled = editBusy || !editTarget || !editInput.value.trim()
    || editInput.value === editTarget.text || bytes > 65536;
  if (bytes > 65536) {
    editError.textContent = window.strings['chats.tooLong'];
    editError.hidden = false;
  }
};
editInput.addEventListener('input', () => {
  editError.hidden = true;
  editError.textContent = '';
  updateEdit();
});
const openEdit = (message) => {
  if (!activeChat || !activeChat.messages.includes(message)
    || message.author !== current.peerId || message.deleted) return;
  editTarget = { peerId: activeChat.peerId, groupId: message.groupId,
    messageId: message.id, text: message.text, generation: identityGeneration,
    request: chatOpenRequest };
  editInput.value = message.text;
  editError.hidden = true;
  editError.textContent = '';
  updateEdit();
  editDialog.hidden = false;
  editDialog.showModal();
  editInput.focus();
};
const closeClear = () => {
  if (clearDialog.open) clearDialog.close();
  clearDialog.hidden = true;
  clearTarget = null;
  clearBusy = false;
  clearCancel.disabled = false;
  clearConfirm.disabled = false;
};
clearDialog.addEventListener('close', () => { if (!clearDialog.open) closeClear(); });
clearDialog.addEventListener('cancel', (event) => {
  if (clearBusy) event.preventDefault();
});
clearCancel.addEventListener('click', closeClear);
document.getElementById('chat-clear').addEventListener('click', () => {
  if (!activeChat) return;
  clearTarget = { peerId: activeChat.peerId, generation: identityGeneration, request: chatOpenRequest };
  clearError.hidden = true;
  clearError.textContent = '';
  clearDialog.hidden = false;
  clearDialog.showModal();
  clearCancel.focus();
});
const chatTime = (message) => message?.sentAtMs == null ? '' :
  new Intl.DateTimeFormat(document.documentElement.lang, { dateStyle: 'short', timeStyle: 'short' })
    .format(new Date(message.sentAtMs));
const chatPreview = (message) => !message ? window.strings['chats.noMessages']
  : message.deleted ? window.strings['chats.deleted'] : message.text || '';
const chatRow = (row) => {
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'list-button';
  button.dataset.peerId = row.peerId;
  const content = document.createElement('span');
  content.className = 'grow';
  const name = document.createElement('b');
  name.textContent = peerLabel(name, row.peerId);
  const preview = document.createElement('span');
  preview.className = 'caption ellipsis';
  preview.textContent = chatPreview(row.lastMessage);
  content.append(name, document.createElement('br'), preview);
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
    unread.setAttribute('aria-label', `${window.strings['chats.unread']}: ${row.unreadCount}`);
    meta.append(document.createElement('br'), unread);
  }
  button.append(content, meta);
  button.addEventListener('click', () => { window.chatOpened = openChat(row.peerId); });
  return button;
};
const renderChatRows = (rows) => {
  visibleChatRows = rows;
  knownPeers = [...new Set([...knownPeers, ...rows.map((row) => row.peerId)])];
  const requests = rows.filter((row) => row.state === 'requestReceived');
  const conversations = rows.filter((row) => row.state !== 'requestReceived');
  chatRequests.replaceChildren(...requests.map(chatRow));
  document.getElementById('chats-requests-count').textContent = String(requests.length);
  chatList.replaceChildren(...conversations.map(chatRow));
  if (!requests.length) chatRequests.textContent = window.strings['chats.noRequests'];
  if (!conversations.length) chatList.textContent = window.strings['chats.noConversations'];
};
const closeChat = () => {
  chatOpenRequest += 1;
  closeClear();
  closeEdit();
  activeChat = null;
  chatSending = false;
  chatBlocking = false;
  chatInput.value = '';
  chatComposeStatus.textContent = '';
  updateComposer();
  chatDetail.hidden = true;
  chatOverview.hidden = false;
};
document.getElementById('chat-back').addEventListener('click', closeChat);
document.addEventListener('dyapp:area-change', (event) => {
  if (event.detail === 'chats-area') closeChat();
});
document.addEventListener('dyapp:identity-change', () => {
  chatsRequest += 1;
  visibleChatRows = [];
  clearedPeer = null;
  closeChat();
  chatRequests.replaceChildren();
  document.getElementById('chats-requests-count').textContent = '0';
  chatList.replaceChildren();
  chatMessages.replaceChildren();
  document.getElementById('chat-cleared').hidden = true;
  likedList.replaceChildren();
  document.getElementById('chats-liked-count').textContent = '0';
});
const openChat = async (peerId, call = invoke) => {
  const generation = identityGeneration;
  const request = ++chatOpenRequest;
  const currentChat = () => identityCurrent(generation) && request === chatOpenRequest;
  activeChat = null;
  updateComposer();
  chatsMessage('chats.loading');
  try {
    const history = await call('chat_history', { peerId });
    if (!currentChat()) return;
    chatMessages.replaceChildren(...history.messages.map((message) => {
      const item = document.createElement('p');
      item.className = message.author === current.peerId ? 'msg out' : 'msg';
      if (message.deleted) item.classList.add('deleted');
      const label = document.createElement('b');
      label.textContent = message.author === current.peerId ?
        window.strings['chats.you'] : shortPeerId(peerId, knownPeers);
      const body = document.createElement('span');
      body.textContent = chatPreview(message);
      item.append(label, document.createTextNode(': '), body);
      if (message.edited && !message.deleted) {
        const edited = document.createElement('span');
        edited.className = 'caption';
        edited.textContent = ` (${window.strings['chats.edited']})`;
        item.append(edited);
      }
      if (message.author === current.peerId && !message.deleted) {
        const edit = document.createElement('button');
        edit.type = 'button';
        edit.className = 'btn quiet';
        edit.textContent = window.strings['chats.edit'];
        edit.addEventListener('click', () => openEdit(message));
        item.append(document.createTextNode(' '), edit);
      }
      const time = chatTime(message);
      if (time) {
        const stamp = document.createElement('time');
        stamp.className = 'caption';
        stamp.dateTime = new Date(message.sentAtMs).toISOString();
        stamp.textContent = time;
        item.append(document.createTextNode(' '), stamp);
      }
      return item;
    }));
    const cleared = clearedPeer === peerId && !history.messages.length;
    document.getElementById('chat-cleared').hidden = !cleared;
    if (!history.messages.length && !cleared) chatMessages.textContent = window.strings['chats.noMessages'];
    const peer = document.getElementById('chat-peer');
    peer.textContent = peerLabel(peer, peerId);
    document.getElementById('chat-state').textContent =
      window.strings[`chats.state.${history.state}`] || '';
    activeChat = { peerId, ...history };
    document.getElementById('chat-clear').hidden = history.state === 'none';
    updateComposer();
    chatOverview.hidden = true;
    chatDetail.hidden = false;
    chatsMessage('chats.ready');
    if (history.state === 'none') return;
    await call('mark_chat_read', { peerId });
    if (!currentChat()) return;
    const rows = await call('chat_rows');
    if (!currentChat()) return;
    renderChatRows(rows);
  } catch (error) {
    if (currentChat()) chatsMessage('chats.failed', String(error));
  }
};
document.getElementById('view-message').addEventListener('click', () => {
  const found = viewed?.found;
  if (!found) return;
  showArea('chats-area');
  window.chatOpened = openChat(found.peerId);
});
const blockChat = async (call = invoke) => {
  if (!activeChat || chatBlocking) return;
  const { peerId } = activeChat;
  const generation = identityGeneration;
  const request = chatOpenRequest;
  chatBlocking = true;
  updateComposer();
  let result;
  try {
    result = await call('block_peer', { peerId });
  } catch (error) {
    if (identityCurrent(generation) && request === chatOpenRequest && activeChat?.peerId === peerId) {
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
  if (request === chatOpenRequest && activeChat?.peerId === peerId) closeChat();
  else { chatBlocking = false; updateComposer(); }
  chatsMessage(`chats.blocked.${result.revocation}`);
  const rows = await call('chat_rows').catch(() => null);
  if (identityCurrent(generation) && listRequest === chatsRequest && rows) renderChatRows(rows);
};
for (const id of ['chat-block', 'request-block']) {
  document.getElementById(id).addEventListener('click', () => { window.chatBlocked = blockChat(); });
}
const clearChat = async (call = invoke) => {
  const target = clearTarget;
  if (!target || clearBusy) return;
  const currentClear = () => clearTarget === target && identityCurrent(target.generation)
    && target.request === chatOpenRequest && activeChat?.peerId === target.peerId;
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
    clearError.textContent = `${window.strings['chats.clearFailed']} ${String(error)}`;
    clearError.hidden = false;
    return;
  }
  if (!currentClear()) return;
  clearedPeer = target.peerId;
  chatInput.value = '';
  chatMessages.replaceChildren();
  document.getElementById('chat-cleared').hidden = false;
  closeClear();
  await openChat(target.peerId, call);
};
clearConfirm.addEventListener('click', () => { window.chatCleared = clearChat(); });
const sendChatText = async (call = invoke) => {
  if (!activeChat || chatSending || chatSend.disabled) return;
  const { peerId, messages } = activeChat;
  const text = chatInput.value;
  const generation = identityGeneration;
  const request = chatOpenRequest;
  const currentSend = () => identityCurrent(generation) && request === chatOpenRequest
    && activeChat?.peerId === peerId;
  const existing = new Set(messages.map((message) => JSON.stringify(message.id)));
  chatSending = true;
  chatComposeStatus.textContent = window.strings['chats.sending'];
  updateComposer();
  let result = 'sent';
  let errorText = '';
  try {
    await call('send_chat_text', { peerId, text });
  } catch (error) {
    errorText = String(error);
    // A failed node delivery may already have committed the text and its retryable outbox.
    // Show that local text instead of allowing a second copy from the same draft.
    const history = await call('chat_history', { peerId }).catch(() => null);
    if (!currentSend()) return;
    result = history?.messages.some((message) => message.author === current.peerId
      && !existing.has(JSON.stringify(message.id))) ? 'queued' : 'failed';
  }
  if (!currentSend()) return;
  chatSending = false;
  if (result === 'failed') {
    chatComposeStatus.textContent = `${window.strings['chats.sendFailed']} ${errorText}`;
    updateComposer();
    return;
  }
  chatInput.value = '';
  updateComposer();
  await openChat(peerId, call);
  if (identityCurrent(generation) && activeChat?.peerId === peerId) {
    chatComposeStatus.textContent = window.strings[`chats.${result}`];
  }
};
document.getElementById('chat-composer').addEventListener('submit', (event) => {
  event.preventDefault();
  window.chatSent = sendChatText();
});
const editChatText = async (call = invoke) => {
  const target = editTarget;
  if (!target || editSave.disabled || editBusy) return;
  const text = editInput.value;
  const currentEdit = () => editTarget === target && identityCurrent(target.generation)
    && target.request === chatOpenRequest && activeChat?.peerId === target.peerId;
  editBusy = true;
  editCancel.disabled = true;
  updateEdit();
  let result = 'editSent';
  let errorText = '';
  try {
    await call('edit_chat_text', { peerId: target.peerId, groupId: target.groupId,
      messageId: target.messageId, text });
  } catch (error) {
    errorText = String(error);
    // The edit may be committed locally even when network delivery failed.
    const history = await call('chat_history', { peerId: target.peerId }).catch(() => null);
    if (!currentEdit()) return;
    result = history?.messages.some((message) => message.author === current.peerId
      && JSON.stringify(message.id) === JSON.stringify(target.messageId)
      && JSON.stringify(message.groupId) === JSON.stringify(target.groupId)
      && message.edited && !message.deleted && message.text === text) ? 'editQueued' : 'failed';
  }
  if (!currentEdit()) return;
  editBusy = false;
  if (result === 'failed') {
    editCancel.disabled = false;
    editError.textContent = `${window.strings['chats.editFailed']} ${errorText}`;
    editError.hidden = false;
    updateEdit();
    return;
  }
  closeEdit();
  await openChat(target.peerId, call);
  if (identityCurrent(target.generation) && activeChat?.peerId === target.peerId) {
    chatComposeStatus.textContent = window.strings[`chats.${result}`];
  }
};
document.getElementById('chat-edit-form').addEventListener('submit', (event) => {
  event.preventDefault();
  window.chatEdited = editChatText();
});
window.showChats = async (call = invoke) => {
  const generation = identityGeneration;
  const request = ++chatsRequest;
  const currentChats = () => identityCurrent(generation) && request === chatsRequest;
  chatsMessage('chats.loading');
  // Keep the device-local projection visible even if mailbox or profile lookup fails.
  const receiveError = await call('receive_mail').then(() => null, (error) => String(error));
  if (!currentChats()) return;
  let rows;
  try {
    rows = await call('chat_rows');
  } catch (error) {
    if (currentChats()) chatsMessage('chats.rowsFailed', String(error));
    return;
  }
  if (!currentChats()) return;
  renderChatRows(rows);
  try {
    const found = await withPlaces(await call('liked_you'));
    if (!currentChats()) return;
    knownPeers = [...new Set([...knownPeers, ...found.map((item) => item.peerId)])];
    likedList.replaceChildren(...found.map((item, index) => resultCard(item, `liked-${index + 1}`)));
    document.getElementById('chats-liked-count').textContent = String(found.length);
    if (receiveError === 'notConnected') chatsMessage('chats.notConnected');
    else if (receiveError) chatsMessage('chats.receiveFailed', receiveError);
    else if (found.length) chatsMessage('chats.count', String(found.length));
    else if (rows.length) chatsMessage('chats.ready');
    else chatsMessage('chats.none');
  } catch (error) {
    if (!currentChats()) return;
    const code = String(error);
    if (code === 'notConnected') chatsMessage('chats.notConnected');
    else chatsMessage('chats.failed', code);
  }
};
// The mailbox is polled in the background; failures stay quiet, the next poll retries.
const pollMailbox = async (call = invoke) => {
  const generation = identityGeneration;
  const changed = await call('receive_mail').catch(() => 0);
  if (identityCurrent(generation) && changed && !document.getElementById('chats-area').hidden) {
    window.chatsDone = window.showChats();
  }
};
setInterval(pollMailbox, 60000);
likedButton.addEventListener('click', () => {
  const open = likedButton.getAttribute('aria-expanded') !== 'true';
  likedButton.setAttribute('aria-expanded', String(open));
  likedList.hidden = !open;
});
