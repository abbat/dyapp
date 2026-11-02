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
const newDeviceNotice = document.getElementById('chat-new-device-notice');
let activeChat = null;
let newDevicePeer = null;
let chatSending = false;
let chatBlocking = false;
const updateComposer = () => {
  const waiting = activeChat?.state === 'requestSent';
  const closed = activeChat?.state === 'closed';
  const writable = Boolean(activeChat) && !waiting && !closed && !chatSending && !chatBlocking;
  const bytes = new TextEncoder().encode(chatInput.value).length;
  chatInput.disabled = !writable;
  chatSend.disabled = !writable || !chatInput.value.trim() || bytes > 65536;
  const attach = document.getElementById('chat-attach');
  attach.hidden = activeChat?.state !== 'open';
  attach.disabled = !writable;
  document.getElementById('chat-first-notice').hidden = !waiting;
  document.getElementById('chat-closed-notice').hidden = !closed;
  document.getElementById('chat-request-notice').hidden = activeChat?.state !== 'requestReceived';
  document.getElementById('chat-block').hidden = activeChat?.state === 'requestReceived';
  for (const id of ['chat-block', 'request-block', 'chat-clear', 'chat-mute']) {
    document.getElementById(id).disabled = chatSending || chatBlocking;
  }
  const mute = document.getElementById('chat-mute');
  mute.dataset.i18n = activeChat?.muted ? 'chats.unmute' : 'chats.mute';
  mute.textContent = window.strings[mute.dataset.i18n];
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
  if (!activeChat?.messages.includes(message)
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
const deleteDialog = document.getElementById('chat-delete-dialog');
const deleteError = document.getElementById('chat-delete-error');
const deleteCancel = document.getElementById('chat-delete-cancel');
const deleteConfirm = document.getElementById('chat-delete-confirm');
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
deleteDialog.addEventListener('close', () => { if (!deleteDialog.open) closeDelete(); });
deleteDialog.addEventListener('cancel', (event) => {
  if (deleteBusy) event.preventDefault();
});
deleteCancel.addEventListener('click', closeDelete);
const openDelete = (message) => {
  if (!activeChat?.messages.includes(message)
    || message.author !== current.peerId || message.deleted) return;
  deleteTarget = { peerId: activeChat.peerId, groupId: message.groupId,
    messageId: message.id, generation: identityGeneration, request: chatOpenRequest };
  deleteError.hidden = true;
  deleteError.textContent = '';
  deleteDialog.hidden = false;
  deleteDialog.showModal();
  deleteCancel.focus();
};
const forwardDialog = document.getElementById('chat-forward-dialog');
const forwardSelect = document.getElementById('chat-forward-peer');
const forwardError = document.getElementById('chat-forward-error');
const forwardCancel = document.getElementById('chat-forward-cancel');
const forwardConfirm = document.getElementById('chat-forward-confirm');
let forwardTarget = null;
let forwardBusy = false;
const closeForward = () => {
  if (forwardDialog.open) forwardDialog.close();
  forwardDialog.hidden = true;
  forwardTarget = null;
  forwardBusy = false;
  forwardCancel.disabled = false;
  forwardConfirm.disabled = false;
  forwardSelect.disabled = false;
  forwardSelect.replaceChildren();
};
forwardDialog.addEventListener('close', () => { if (!forwardDialog.open) closeForward(); });
forwardDialog.addEventListener('cancel', (event) => {
  if (forwardBusy) event.preventDefault();
});
forwardCancel.addEventListener('click', closeForward);
const openForward = (message) => {
  if (!activeChat?.messages.includes(message) || message.deleted || !message.text) return;
  forwardTarget = { sourcePeerId: activeChat.peerId, sourceGroupId: message.groupId,
    sourceAuthor: message.author, sourceMessageId: message.id, text: message.text,
    generation: identityGeneration, request: chatOpenRequest };
  const targets = visibleChatRows.filter((row) => row.state === 'open'
    && row.peerId !== activeChat.peerId);
  forwardSelect.replaceChildren(...targets.map((row) => {
    const option = document.createElement('option');
    option.value = row.peerId;
    option.textContent = window.peerName(row.peerId);
    return option;
  }));
  forwardConfirm.disabled = !targets.length;
  forwardError.hidden = !!targets.length;
  forwardError.textContent = targets.length ? '' : window.strings['chats.forwardNoTargets'];
  forwardDialog.hidden = false;
  forwardDialog.showModal();
  if (targets.length) forwardSelect.focus();
  else forwardCancel.focus();
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
const chatLinkHosts = (text) => {
  const hosts = new Set();
  for (const match of (text || '').matchAll(/https?:\/\/[^\s<>"']+/gi)) {
    try {
      const url = new URL(match[0].replace(/[),.!?]+$/u, ''));
      if (url.protocol === 'http:' || url.protocol === 'https:') hosts.add(url.host);
    } catch { /* A malformed URL remains plain text. */ }
  }
  return [...hosts];
};
const chatPreview = (message) => !message ? window.strings['chats.noMessages']
  : message.deleted ? window.strings['chats.deleted']
    : `${message.forwarded ? `${window.strings['chats.forwarded']} · ` : ''}${
      message.photo ? window.strings['chats.photo'] : message.text || ''}`;
// The page's CSP admits no image URLs, so photo bytes are decoded onto a canvas.
const PREVIEW_SIDE = 240;
const ORIGINAL_SIDE = 480;
const MAX_KEPT_ORIGINALS = 16;
// Decrypted originals of the open chat, so a redraw keeps them. Memory only, dropped on leaving it.
const photoOriginals = new Map();
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
const fitCanvas = (canvas, width, height, side) => {
  const scale = Math.min(1, side / Math.max(width, height));
  canvas.width = Math.max(1, Math.round(width * scale));
  canvas.height = Math.max(1, Math.round(height * scale));
};
const drawPhoto = async (canvas, bytes, mime, side) => {
  const bitmap = await createImageBitmap(new Blob([bytes], { type: mime }));
  if (side) fitCanvas(canvas, bitmap.width, bitmap.height, side);
  canvas.getContext('2d').drawImage(bitmap, 0, 0, canvas.width, canvas.height);
  bitmap.close();
};
const photoCaption = (photo) => {
  const parts = [window.strings['chats.photo'], photo.mime, sizeText(photo.size)];
  if (photo.kept) return parts.join(' · ');
  if (photo.hoursLeft === 0) parts.push(window.strings['chats.photoExpired']);
  else if (photo.hoursLeft) {
    parts.push(window.strings['chats.photoExpires'].replace('{hours}', photo.hoursLeft));
  }
  return parts.join(' · ');
};
const photoKey = (message) => `${message.author}:${message.id}`;
const photoError = (code) => window.strings[{
  notFound: 'chats.photoGone', corrupted: 'chats.photoBroken',
}[code] || 'chats.photoFailed'];
const photoButton = (key, name, click) => {
  const button = document.createElement('button');
  button.type = 'button';
  button.className = `btn quiet ${name}`;
  button.textContent = window.strings[key];
  button.addEventListener('click', click);
  return button;
};
// A mail pass redraws the chat under a running fetch, so results go to the bubble of that moment.
const downloadOriginal = async (message, peerId, call) => {
  const generation = identityGeneration;
  const key = photoKey(message);
  if (photoFetches.has(key) || photoOriginals.has(key)) return;
  const { mime } = message.photo;
  const latest = () => (identityCurrent(generation) && photoOriginalsPeer === peerId
    ? photoBoxes.get(key) : null);
  photoFetches.add(key);
  latest()?.showFetching();
  try {
    const bytes = await call('chat_photo_original', {
      peerId, author: message.author, id: message.id,
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
const photoBubble = (message, peerId, blurred, call) => {
  const { photo } = message;
  const key = photoKey(message);
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
  canvas.setAttribute('aria-label', window.strings['chats.photo']);
  box.append(canvas, caption, status);
  box.canvas = canvas;
  box.kept = photo.kept;
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
    status.textContent = window.strings['chats.photoDownloading'];
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
  box.ready = photo.preview.length ? drawPhoto(canvas, new Uint8Array(photo.preview), 'image/jpeg')
    .catch(() => {
      canvas.hidden = true;
      if (box.classList.contains('blurred')) {
        show.remove();
        box.classList.remove('blurred');
        offerDownload();
      }
    }) : Promise.resolve();
  return box;
};
// What the policy allows on this network, latest first, one at a time, among as many photos as
// stay in memory. Asked only while something waits. A refusal forgets the tries, so a switch to an
// unmetered network retries a failed fetch. A kept original opens either way: it costs no network.
const fetchAllowedPhotos = async (peerId, call) => {
  const waiting = () => [...photoBoxes].filter(([, box]) => box.isConnected)
    .slice(-MAX_KEPT_ORIGINALS).reverse()
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
const chatRow = (row) => {
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
    marker.textContent = window.strings['chats.state.requestReceived'];
    content.append(document.createTextNode(' '), marker);
  }
  if (row.activeRecently) {
    const active = document.createElement('span');
    active.className = 'caption';
    active.textContent = window.strings['chats.active'];
    content.append(document.createTextNode(' '), active);
  }
  if (row.muted) {
    const muted = document.createElement('span');
    muted.className = 'caption';
    muted.textContent = window.strings['chats.muted'];
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
    unread.setAttribute('aria-label', `${window.strings['chats.unread']}: ${row.unreadCount}`);
    meta.append(document.createElement('br'), unread);
  }
  button.append(avatar, content, meta);
  button.addEventListener('click', () => { window.chatOpened = openChat(row.peerId); });
  return button;
};
const renderChatRows = (rows, call = invoke) => {
  visibleChatRows = rows;
  knownPeers = [...new Set([...knownPeers, ...rows.map((row) => row.peerId)])];
  const requests = rows.filter((row) => row.state === 'requestReceived');
  const conversations = rows.filter((row) => row.state !== 'requestReceived');
  chatRequests.replaceChildren(...requests.map(chatRow));
  document.getElementById('chats-requests-count').textContent = String(requests.length);
  chatList.replaceChildren(...conversations.map(chatRow));
  const cards = [...chatRequests.children, ...chatList.children];
  window.chatPhotosShown = window.showCardPhotos(cards.map((card) => ({
    draft: { photos: [window.peerPhoto(card.dataset.peerId)] } })), cards, call);
  if (!requests.length) chatRequests.textContent = window.strings['chats.noRequests'];
  if (!conversations.length) chatList.textContent = window.strings['chats.noConversations'];
};
const showNewDevice = (peerId) => {
  newDevicePeer = peerId;
  newDeviceNotice.hidden = peerId === null;
};
// A failed read keeps what is shown; the next mail pass asks again.
const linkedDevice = async (call) => {
  try {
    return await call('linked_device') === true;
  } catch {
    return false;
  }
};
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
  const generation = identityGeneration;
  const shown = () => identityCurrent(generation) && newDevicePeer === peerId;
  const acknowledge = document.getElementById('chat-new-device-ack');
  acknowledge.disabled = true;
  try {
    await call('acknowledge_new_device', { peerId });
    if (!shown()) return;
    showNewDevice(null);
    // Reopening supersedes a load that read the flag before the acknowledgement.
    await openChat(peerId, call);
    (chatInput.disabled ? document.getElementById('chat-back') : chatInput).focus();
  } catch (error) {
    if (shown()) chatsMessage('chats.newDevice.failed', String(error));
  } finally {
    acknowledge.disabled = false;
  }
};
document.getElementById('chat-new-device-ack').addEventListener('click', () => acknowledgeDevice());
const closeChat = () => {
  chatOpenRequest += 1;
  forgetPhotos();
  photoOriginalsPeer = null;
  showNewDevice(null);
  closeClear();
  closeEdit();
  closeRename();
  closeDelete();
  closeForward();
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
// The next view or mail pass retries a receipt that did not go out.
const sendRead = async (peerId, call) => {
  try {
    await call('send_read_receipts', { peerId });
  } catch { /* the page shows nothing for a background receipt */ }
};
const openChat = async (peerId, call = invoke) => {
  const generation = identityGeneration;
  const request = ++chatOpenRequest;
  const currentChat = () => identityCurrent(generation) && request === chatOpenRequest;
  if (newDevicePeer !== peerId) showNewDevice(null);
  if (photoOriginalsPeer !== peerId) forgetPhotos();
  photoOriginalsPeer = peerId;
  activeChat = null;
  updateComposer();
  chatsMessage('chats.loading');
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
    document.getElementById('chat-linked-notice').hidden = !linked;
    chatMessages.replaceChildren(...history.messages.map((message) => {
      const item = document.createElement('p');
      item.className = message.author === current.peerId ? 'msg out' : 'msg';
      if (message.deleted) item.classList.add('deleted');
      const label = document.createElement('b');
      label.textContent = message.author === current.peerId ?
        window.strings['chats.you'] : window.peerName(peerId);
      const body = document.createElement('span');
      body.textContent = message.deleted ? window.strings['chats.deleted'] : message.text || '';
      item.append(label, document.createTextNode(': '));
      if (message.forwarded && !message.deleted) {
        const forwarded = document.createElement('span');
        forwarded.className = 'caption';
        forwarded.textContent = `${window.strings['chats.forwarded']} · `;
        item.append(forwarded);
      }
      item.append(message.photo ? photoBubble(message, peerId,
        message.author !== current.peerId && history.state !== 'open', call) : body);
      const hosts = chatLinkHosts(message.deleted ? '' : message.text);
      if (hosts.length) {
        const host = document.createElement('span');
        host.className = 'caption';
        host.textContent = ` · ${window.strings['chats.linkHost']}: ${hosts.join(', ')}`;
        item.append(host);
      }
      const status = message.read ? 'read' : message.delivery;
      if (message.author === current.peerId && status) {
        const delivery = document.createElement('span');
        delivery.className = 'caption';
        delivery.textContent = ` · ${window.strings[`chats.delivery.${status}`] || ''}`;
        item.append(delivery);
      }
      if (message.edited && !message.deleted) {
        const edited = document.createElement('span');
        edited.className = 'caption';
        edited.textContent = ` (${window.strings['chats.edited']})`;
        item.append(edited);
      }
      if (message.author === current.peerId && !message.deleted && !message.photo
        && history.state !== 'closed') {
        const edit = document.createElement('button');
        edit.type = 'button';
        edit.className = 'btn quiet';
        edit.textContent = window.strings['chats.edit'];
        edit.addEventListener('click', () => openEdit(message));
        const remove = document.createElement('button');
        remove.type = 'button';
        remove.className = 'btn quiet';
        remove.textContent = window.strings['chats.delete'];
        remove.addEventListener('click', () => openDelete(message));
        item.append(document.createTextNode(' '), edit, document.createTextNode(' '), remove);
      }
      if (!message.deleted && message.text) {
        const forward = document.createElement('button');
        forward.type = 'button';
        forward.className = 'btn quiet';
        forward.textContent = window.strings['chats.forward'];
        forward.addEventListener('click', () => openForward(message));
        item.append(document.createTextNode(' '), forward);
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
    fetchAllowedPhotos(peerId, call);
    const cleared = clearedPeer === peerId && !history.messages.length;
    document.getElementById('chat-cleared').hidden = !cleared;
    if (!history.messages.length && !cleared) chatMessages.textContent = window.strings['chats.noMessages'];
    const peer = document.getElementById('chat-peer');
    peer.textContent = peerLabel(peer, peerId);
    document.getElementById('chat-state').textContent =
      window.strings[`chats.state.${history.state}`] || '';
    document.getElementById('chat-presence').hidden = !history.activeRecently;
    activeChat = { peerId, ...history };
    document.getElementById('chat-clear').hidden = history.state === 'none';
    document.getElementById('chat-rename').hidden = history.state === 'none';
    document.getElementById('chat-mute').hidden = history.state === 'none';
    updateComposer();
    chatOverview.hidden = true;
    chatDetail.hidden = false;
    chatsMessage('chats.ready');
    // A mail pass behind a hidden window is not reading; visibilitychange re-runs this.
    if (history.state === 'none' || document.hidden) return;
    await call('mark_chat_read', { peerId });
    if (!currentChat()) return;
    window.chatRead = sendRead(peerId, call);
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
const muteChat = async (call = invoke) => {
  if (!activeChat) return;
  const { peerId } = activeChat;
  const muted = !activeChat.muted;
  const generation = identityGeneration;
  try {
    await call('set_muted', { peerId, muted });
  } catch (error) {
    if (identityCurrent(generation) && activeChat?.peerId === peerId) {
      chatComposeStatus.textContent = `${window.strings['chats.muteFailed']} ${String(error)}`;
    }
    return;
  }
  if (!identityCurrent(generation)) return;
  // Invalidate a mailbox refresh that started before the flag was written.
  chatsRequest += 1;
  renderChatRows(visibleChatRows.map((row) => (row.peerId === peerId ? { ...row, muted } : row)), call);
  if (activeChat?.peerId !== peerId) return;
  activeChat.muted = muted;
  chatComposeStatus.textContent = '';
  updateComposer();
};
document.getElementById('chat-mute').addEventListener('click', () => { window.chatMuted = muteChat(); });
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
// One send of the open chat: holds the composer, tells a queued message from a failed one and
// redraws. `sent` runs before the redraw; `failure` words an unstaged error.
const sendInChat = async (send, sent, failure, call) => {
  const { peerId, messages } = activeChat;
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
    await send(peerId);
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
    chatComposeStatus.textContent = failure(errorText);
    updateComposer();
    return;
  }
  sent();
  updateComposer();
  await openChat(peerId, call);
  if (identityCurrent(generation) && activeChat?.peerId === peerId) {
    chatComposeStatus.textContent = window.strings[`chats.${result}`];
  }
};
const sendChatText = async (call = invoke) => {
  if (!activeChat || chatSending || chatSend.disabled) return;
  const text = chatInput.value;
  await sendInChat((peerId) => call('send_chat_text', { peerId, text }),
    () => { chatInput.value = ''; },
    (error) => {
      if (error === 'peerUnavailable') return window.strings['chats.peerUnavailable'];
      // The peer's unlike landed after this chat was drawn.
      if (String(error).endsWith('chat is closed')) return window.strings['chats.closedNotice'];
      return `${window.strings['chats.sendFailed']} ${error}`;
    }, call);
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
const deleteChatText = async (call = invoke) => {
  const target = deleteTarget;
  if (!target || deleteBusy) return;
  const currentDelete = () => deleteTarget === target && identityCurrent(target.generation)
    && target.request === chatOpenRequest && activeChat?.peerId === target.peerId;
  deleteBusy = true;
  deleteCancel.disabled = true;
  deleteConfirm.disabled = true;
  let result = 'deleteSent';
  let errorText = '';
  try {
    await call('delete_chat_text', { peerId: target.peerId, groupId: target.groupId,
      messageId: target.messageId });
  } catch (error) {
    errorText = String(error);
    // A failed delivery can follow a durable local delete and terminal placeholder.
    const history = await call('chat_history', { peerId: target.peerId }).catch(() => null);
    if (!currentDelete()) return;
    result = history?.messages.some((message) => message.author === current.peerId
      && JSON.stringify(message.id) === JSON.stringify(target.messageId)
      && JSON.stringify(message.groupId) === JSON.stringify(target.groupId)
      && message.deleted && message.text == null) ? 'deleteQueued' : 'failed';
  }
  if (!currentDelete()) return;
  deleteBusy = false;
  if (result === 'failed') {
    deleteCancel.disabled = false;
    deleteConfirm.disabled = false;
    deleteError.textContent = `${window.strings['chats.deleteFailed']} ${errorText}`;
    deleteError.hidden = false;
    return;
  }
  closeDelete();
  await openChat(target.peerId, call);
  if (identityCurrent(target.generation) && activeChat?.peerId === target.peerId) {
    chatComposeStatus.textContent = window.strings[`chats.${result}`];
  }
};
deleteConfirm.addEventListener('click', () => { window.chatDeleted = deleteChatText(); });
const forwardChatText = async (call = invoke) => {
  const source = forwardTarget;
  const targetPeerId = forwardSelect.value;
  if (!source || forwardBusy || forwardConfirm.disabled || !targetPeerId) return;
  const currentForward = () => forwardTarget === source && identityCurrent(source.generation)
    && source.request === chatOpenRequest && activeChat?.peerId === source.sourcePeerId;
  forwardBusy = true;
  forwardCancel.disabled = true;
  forwardConfirm.disabled = true;
  forwardSelect.disabled = true;
  const before = await call('chat_history', { peerId: targetPeerId }).catch(() => null);
  if (!currentForward()) return;
  const oldIds = new Set(before?.messages.map((message) =>
    JSON.stringify([message.groupId, message.id])) || []);
  let result = 'forwardSent';
  let errorText = '';
  try {
    await call('forward_chat_text', { sourcePeerId: source.sourcePeerId,
      sourceGroupId: source.sourceGroupId, sourceAuthor: source.sourceAuthor,
      sourceMessageId: source.sourceMessageId, targetPeerId });
  } catch (error) {
    errorText = String(error);
    const after = await call('chat_history', { peerId: targetPeerId }).catch(() => null);
    if (!currentForward()) return;
    result = before && after?.messages.some((message) => message.author === current.peerId
      && message.forwarded && !message.deleted && message.text === source.text
      && !oldIds.has(JSON.stringify([message.groupId, message.id]))) ? 'forwardQueued' : 'failed';
  }
  if (!currentForward()) return;
  forwardBusy = false;
  if (result === 'failed') {
    forwardCancel.disabled = false;
    forwardConfirm.disabled = false;
    forwardSelect.disabled = false;
    forwardError.textContent = `${window.strings['chats.forwardFailed']} ${errorText}`;
    forwardError.hidden = false;
    return;
  }
  closeForward();
  await openChat(targetPeerId, call);
  if (identityCurrent(source.generation) && activeChat?.peerId === targetPeerId) {
    chatComposeStatus.textContent = window.strings[`chats.${result}`];
  }
};
document.getElementById('chat-forward-form').addEventListener('submit', (event) => {
  event.preventDefault();
  window.chatForwarded = forwardChatText();
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
  await loadPeerNames(rows.map((row) => row.peerId), call);
  if (!currentChats()) return;
  renderChatRows(rows, call);
  try {
    const found = await withPlaces(await call('liked_you'));
    if (!currentChats()) return;
    knownPeers = [...new Set([...knownPeers, ...found.map((item) => item.peerId)])];
    likedList.replaceChildren(...found.map((item, index) => resultCard(item, `liked-${index + 1}`)));
    window.showCardPhotos(found, [...likedList.children], call);
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
// The runtime reports each background pass: its changed-event count, or null after a failure,
// which stays quiet because the next pass retries. A failure can follow a commit, such as a lost
// ACK, so the list still follows the local projection, and with no mail a presence can expire.
window.mailUpdated = async (changed, call = invoke) => {
  const generation = identityGeneration;
  if (document.getElementById('chats-area').hidden) return;
  // Retry and signed receipts can change delivery status without a new visible chat event.
  if (activeChat && !editDialog.open && !deleteDialog.open && !clearDialog.open
    && !chatSending && !chatBlocking) {
    await openChat(activeChat.peerId, call);
  } else if (!changed) {
    const request = chatsRequest;
    const rows = await call('chat_rows').catch(() => null);
    // An unchanged list is left alone so an offline poll cannot take keyboard focus from a row.
    if (rows && identityCurrent(generation) && request === chatsRequest
      && JSON.stringify(rows) !== JSON.stringify(visibleChatRows)) renderChatRows(rows);
  } else if (changed) {
    window.chatsDone = window.showChats(call);
  }
};
document.addEventListener('visibilitychange', () => {
  if (!document.hidden) window.mailUpdated(0);
});
// Sent on launch and on focus, never on a timer, so it follows real use; the Store keeps the hour.
window.announcePresence = async (call = invoke) => {
  if (document.hidden) return;
  try {
    await call('announce_presence');
  } catch { /* the page shows nothing for a background signal */ }
};
window.addEventListener('focus', () => { window.announcePresence(); });
likedButton.addEventListener('click', () => {
  const open = likedButton.getAttribute('aria-expanded') !== 'true';
  likedButton.setAttribute('aria-expanded', String(open));
  likedList.hidden = !open;
});
