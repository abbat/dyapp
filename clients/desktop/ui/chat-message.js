// Message-row construction shares the chat controller's classic-script scope.
(() => {
  function appendMessageBody(item, message, peerId, history, call) {
    const sticker = message.sticker && !message.deleted;
    const label = document.createElement('b');
    label.textContent =
      message.author === current.peerId ? window.strings['chats.you'] : window.peerName(peerId);
    const body = document.createElement('span');
    body.textContent = messageText(message);
    if (sticker) item.append(label);
    else item.append(label, document.createTextNode(': '));
    if (message.forwarded && !message.deleted) {
      const forwarded = document.createElement('span');
      forwarded.className = 'caption';
      forwarded.textContent = `${window.strings['chats.forwarded']} · `;
      item.append(forwarded);
    }
    item.append(messageContent(message, peerId, history, call, body));
    if (sticker && message.author === peerId && history.state === 'open')
      item.append(window.stickerRequests.controls(message, peerId, call));
  }

  function messageText(message) {
    if (message.deleted) return window.strings['chats.deleted'];
    if (message.sticker) return window.stickerMessageText(message.sticker);
    return message.text || '';
  }

  function messageContent(message, peerId, history, call, body) {
    const { photoBubble } = window.chatMessageHelpers;
    if (message.photo) {
      return photoBubble(
        message,
        peerId,
        message.author !== current.peerId && history.state !== 'open',
        call,
      );
    }
    if (message.sticker && !message.deleted)
      return window.stickerMessages.bubble(message, peerId, call);
    if (message.sharedPeer && !message.deleted) return window.profileCard(message.sharedPeer, call);
    return body;
  }

  function appendMessageMetadata(item, message) {
    const { chatLinkHosts } = window.chatMessageHelpers;
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
  }

  function appendMessageActions(item, message, history) {
    const { openEdit, openDelete, openForward } = window.chatMessageHelpers;
    const sticker = message.sticker && !message.deleted;
    if (
      message.author === current.peerId &&
      !message.deleted &&
      !message.photo &&
      !message.sticker &&
      history.state !== 'closed'
    ) {
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
    if (sticker && message.author === current.peerId && history.state !== 'closed') {
      const remove = document.createElement('button');
      remove.type = 'button';
      remove.className = 'btn quiet';
      remove.textContent = window.strings['chats.delete'];
      remove.addEventListener('click', () => openDelete(message));
      item.append(remove);
    }
    if (!message.deleted && message.text) {
      const forward = document.createElement('button');
      forward.type = 'button';
      forward.className = 'btn quiet';
      forward.textContent = window.strings['chats.forward'];
      forward.addEventListener('click', () => openForward(message));
      item.append(document.createTextNode(' '), forward);
    }
  }

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

  window.chatMessage = (message, peerId, history, call) => {
    const sticker = message.sticker && !message.deleted;
    const item = document.createElement(sticker ? 'div' : 'p');
    item.className = sticker ? 'sticker-message' : 'msg';
    if (message.author === current.peerId) item.classList.add('out');
    if (message.deleted) item.classList.add('deleted');
    appendMessageBody(item, message, peerId, history, call);
    appendMessageMetadata(item, message);
    appendMessageActions(item, message, history);
    appendMessageTime(item, message);
    return item;
  };
})();
