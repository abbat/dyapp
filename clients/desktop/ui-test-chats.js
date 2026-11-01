// Desktop UI smoke: Chats list, first text, send and Clear.
// Evaluated after ui-test-identity.js in the same page scope.
// The local projection renders requests, unread counts and original timestamps without a node.
const checkChatProjection = async () => {
  await openArea('chats-area');
  await window.chatsDone;
  const peer = 'd'.repeat(64);
  const other = 'e'.repeat(64);
  const message = { author: peer, text: 'hello', deleted: false, sentAtMs: 1_700_000_000_000 };
  const rows = [
    { peerId: peer, state: 'requestReceived', lastMessage: message, unreadCount: 2 },
    { peerId: other, state: 'open', lastMessage: { text: null, deleted: true, sentAtMs: null }, unreadCount: 0 },
  ];
  await window.showChats((command) => {
    if (command === 'receive_mail') return Promise.reject('notConnected');
    if (command === 'chat_rows') return Promise.resolve(rows);
    if (command === 'liked_you') return Promise.reject('notConnected');
    throw new Error(`unexpected Chats command: ${command}`);
  });
  const request = document.querySelector('#chats-requests button');
  const conversation = document.querySelector('#chat-list button');
  if (!request || !conversation || !request.textContent.includes('hello')
    || !request.querySelector('.meta .caption').textContent
    || !request.textContent.includes('2')
    || !conversation.textContent.includes(window.strings['chats.deleted'])
    || document.getElementById('chats-status').textContent !== window.strings['chats.notConnected']) {
    throw new Error('local Chats rows disappeared when the node failed');
  }
  let marked = false;
  await openChat(peer, (command) => {
    if (command === 'chat_history') return Promise.resolve({ state: 'requestReceived', messages: [message] });
    if (command === 'mark_chat_read') { marked = true; return Promise.resolve(1); }
    if (command === 'chat_rows') return Promise.resolve([{ ...rows[0], unreadCount: 0 }, rows[1]]);
    throw new Error(`unexpected chat command: ${command}`);
  });
  if (!marked || chatDetail.hidden || chatMessages.textContent.indexOf('hello') < 0
    || document.querySelector('#chats-requests .badge')) {
    throw new Error('chat history did not clear local unread');
  }
  chatInput.value = 'reply';
  chatInput.dispatchEvent(new Event('input'));
  let sentArgs;
  const reply = { author: current.peerId, id: [1], text: 'reply', deleted: false, sentAtMs: null };
  await sendChatText((command, args) => {
    if (command === 'send_chat_text') { sentArgs = args; return Promise.resolve([1]); }
    if (command === 'chat_history') return Promise.resolve({ state: 'open', messages: [message, reply] });
    if (command === 'mark_chat_read') return Promise.resolve(0);
    if (command === 'chat_rows') return Promise.resolve([{ ...rows[0], state: 'open', unreadCount: 0 }]);
    throw new Error(`unexpected reply command: ${command}`);
  });
  if (sentArgs.peerId !== peer || sentArgs.text !== 'reply' || chatInput.value
    || activeChat.state !== 'open') throw new Error('reply did not open conversation');

  await openChat(peer, (command) => {
    if (command === 'chat_history') return Promise.resolve({ state: 'requestSent', messages: [reply] });
    if (command === 'mark_chat_read') return Promise.resolve(0);
    if (command === 'chat_rows') return Promise.resolve(rows);
    throw new Error(`unexpected first-message command: ${command}`);
  });
  if (!chatInput.disabled || !chatSend.disabled
    || document.getElementById('chat-first-notice').hidden) {
    throw new Error('second unanswered first message is enabled');
  }

  await openChat(peer, (command) => {
    if (command === 'chat_history') return Promise.resolve({ state: 'open', messages: [message] });
    if (command === 'mark_chat_read') return Promise.resolve(0);
    if (command === 'chat_rows') return Promise.resolve(rows);
    throw new Error(`unexpected open command: ${command}`);
  });
  chatInput.value = 'keep draft';
  chatInput.dispatchEvent(new Event('input'));
  await sendChatText((command) => {
    if (command === 'send_chat_text') return Promise.reject('notConnected');
    if (command === 'chat_history') return Promise.resolve({ state: 'open', messages: [message] });
    throw new Error(`unexpected failed send command: ${command}`);
  });
  if (chatInput.value !== 'keep draft' || !chatComposeStatus.textContent.includes('notConnected')) {
    throw new Error('failed send lost the unstaged draft');
  }
  await sendChatText((command) => {
    if (command === 'send_chat_text') return Promise.reject('nodeFailed');
    if (command === 'chat_history') return Promise.resolve({ state: 'open', messages: [message, reply] });
    if (command === 'mark_chat_read') return Promise.resolve(0);
    if (command === 'chat_rows') return Promise.resolve(rows);
    throw new Error(`unexpected staged send command: ${command}`);
  });
  if (chatInput.value || chatComposeStatus.textContent !== window.strings['chats.queued']) {
    throw new Error('staged failed send could be duplicated');
  }
  document.getElementById('chat-back').click();
  if (!chatDetail.hidden || chatOverview.hidden) throw new Error('chat Back did not show the list');
  const readyState = current;
  let finishHistory;
  const oldHistory = openChat(peer, () => new Promise((resolve) => { finishHistory = resolve; }));
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  finishHistory({ state: 'open', messages: [message] });
  await oldHistory;
  if (!chatDetail.hidden) throw new Error('old chat history opened after identity switch');
  await openChat(peer, (command) => {
    if (command === 'chat_history') return Promise.resolve({ state: 'open', messages: [message] });
    if (command === 'mark_chat_read') return Promise.resolve(0);
    if (command === 'chat_rows') return Promise.resolve(rows);
    throw new Error(`unexpected stale-send setup: ${command}`);
  });
  chatInput.value = 'late';
  chatInput.dispatchEvent(new Event('input'));
  let finishSend;
  const oldSend = sendChatText(() => new Promise((resolve) => { finishSend = resolve; }));
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  finishSend([1]);
  await oldSend;
  if (!chatDetail.hidden || chatInput.value) throw new Error('old send painted new identity');
};
const checkFirstMessage = async () => {
  await openArea('search-area');
  const found = { peerId: 'a'.repeat(64),
    draft: { birthYear: null, birthMonth: null, birthDay: null }, place: '' };
  const card = resultCard(found, 'first-message-profile');
  searchResults.append(card);
  openFound(found, card, () => Promise.resolve({ ours: false, matched: false }));
  const messageButton = document.getElementById('view-message');
  if (messageButton.disabled) throw new Error('Found profile cannot start a message');
  messageButton.click();
  await window.chatOpened;
  if (activeChat?.peerId !== found.peerId || activeChat.state !== 'none'
    || chatInput.disabled || !document.getElementById('chat-clear').hidden) {
    throw new Error('first-message detail did not open');
  }
  chatInput.value = 'first text';
  chatInput.dispatchEvent(new Event('input'));
  let sent;
  const own = { author: current.peerId, id: [7], text: 'first text',
    deleted: false, sentAtMs: null };
  await sendChatText((command, args) => {
    if (command === 'send_chat_text') { sent = args; return Promise.resolve([7]); }
    if (command === 'chat_history') return Promise.resolve({ state: 'requestSent', messages: [own] });
    if (command === 'mark_chat_read') return Promise.resolve(0);
    if (command === 'chat_rows') return Promise.resolve([
      { peerId: found.peerId, state: 'requestSent', lastMessage: own, unreadCount: 0 },
    ]);
    throw new Error(`unexpected first text command: ${command}`);
  });
  if (sent.peerId !== found.peerId || sent.text !== 'first text'
    || activeChat.state !== 'requestSent' || !chatInput.disabled
    || !document.querySelector('#chat-list button')) {
    throw new Error('first text did not create a pending request row');
  }
  let second = 0;
  await sendChatText(() => { second += 1; throw new Error('second request sent'); });
  if (second) throw new Error('second unanswered text was sent');
  showArea('search-area');
  closeFound();
};
const checkClearChat = async () => {
  await openArea('chats-area');
  await window.chatsDone;
  const peer = 'c'.repeat(64);
  const message = { author: peer, id: [3], text: 'private', deleted: false, sentAtMs: null };
  const row = { peerId: peer, state: 'open', lastMessage: message, unreadCount: 0 };
  const history = { state: 'open', messages: [message] };
  const open = (currentHistory) => openChat(peer, (command) => {
    if (command === 'chat_history') return Promise.resolve(currentHistory);
    if (command === 'mark_chat_read') return Promise.resolve(0);
    if (command === 'chat_rows') return Promise.resolve([row]);
    throw new Error(`unexpected clear setup: ${command}`);
  });
  await open(history);
  document.getElementById('chat-clear').click();
  if (!clearDialog.open || !clearDialog.textContent.includes(window.strings['chats.clearWarning'])) {
    throw new Error('Clear chat confirmation missing');
  }
  clearCancel.click();
  if (clearDialog.open || !chatMessages.textContent.includes('private')) {
    throw new Error('Cancel cleared the chat');
  }
  document.getElementById('chat-clear').click();
  await clearChat((command) => {
    if (command === 'clear_chat') return Promise.reject('diskFailed');
    throw new Error(`unexpected failed clear: ${command}`);
  });
  if (!clearDialog.open || !clearError.textContent.includes('diskFailed')
    || !chatMessages.textContent.includes('private')) throw new Error('failed Clear lost chat');
  let clearCalls = 0;
  await clearChat((command) => {
    if (command === 'clear_chat') { clearCalls += 1; return Promise.resolve(1); }
    if (command === 'chat_history') return Promise.resolve({ state: 'open', messages: [] });
    if (command === 'mark_chat_read') return Promise.resolve(0);
    if (command === 'chat_rows') return Promise.resolve([{ ...row, lastMessage: null }]);
    throw new Error(`unexpected clear command: ${command}`);
  });
  if (clearCalls !== 1 || clearDialog.open || chatMessages.textContent !== ''
    || document.getElementById('chat-cleared').hidden || activeChat.state !== 'open'
    || !document.querySelector('#chat-list button[data-peer-id]')) {
    throw new Error('Clear did not keep the empty conversation');
  }
  document.getElementById('chat-clear').click();
  let finishClear;
  const oldClear = clearChat(() => new Promise((resolve) => { finishClear = resolve; }));
  const readyState = current;
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  finishClear(0);
  await oldClear;
  if (clearDialog.open || !document.getElementById('chat-cleared').hidden) {
    throw new Error('old Clear repainted new identity');
  }
};
const checkChatBlock = async () => {
  await openArea('chats-area');
  await window.chatsDone;
  const peer = 'b'.repeat(64);
  const message = { author: peer, id: [4], text: 'request', deleted: false, sentAtMs: null };
  const row = { peerId: peer, state: 'requestReceived', lastMessage: message, unreadCount: 1 };
  const open = (state) => openChat(peer, (command) => {
    if (command === 'chat_history') return Promise.resolve({ state, messages: [message] });
    if (command === 'mark_chat_read') return Promise.resolve(0);
    if (command === 'chat_rows') return Promise.resolve([row]);
    throw new Error(`unexpected block setup: ${command}`);
  });
  renderChatRows([row]);
  await open('requestReceived');
  if (document.getElementById('request-block').hidden
    || !document.getElementById('chat-block').hidden) {
    throw new Error('request Block control missing');
  }
  await blockChat((command) => {
    if (command === 'block_peer') return Promise.reject('diskFailed');
    throw new Error(`unexpected failed block: ${command}`);
  });
  if (chatDetail.hidden || !chatRequests.querySelector('button')
    || !document.getElementById('chats-status').textContent.includes('diskFailed')) {
    throw new Error('failed Block hid the chat');
  }
  await open('open');
  if (document.getElementById('chat-block').hidden) throw new Error('open chat Block missing');
  await blockChat((command) => {
    if (command === 'block_peer') return Promise.resolve({ revocation: 'pending', detail: 'offline' });
    if (command === 'chat_rows') return Promise.resolve([]);
    throw new Error(`unexpected successful block: ${command}`);
  });
  if (!chatDetail.hidden || chatRequests.querySelector('button')
    || document.getElementById('chats-status').textContent
      !== window.strings['chats.blocked.pending']) {
    throw new Error('blocked peer stayed in Chats or revocation was overstated');
  }
  await open('open');
  let finishBlock;
  const oldBlock = blockChat((command) => command === 'block_peer'
    ? new Promise((resolve) => { finishBlock = resolve; }) : Promise.resolve([]));
  const readyState = current;
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  finishBlock({ revocation: 'confirmed' });
  await oldBlock;
  if (!chatDetail.hidden) throw new Error('old Block repainted new identity');
};
const checkChatEdit = async () => {
  await openArea('chats-area');
  await window.chatsDone;
  const peer = 'f'.repeat(64);
  const groupId = [8, 9];
  const own = { author: current.peerId, groupId, id: [1], text: 'original',
    deleted: false, edited: false, sentAtMs: null };
  const theirs = { ...own, author: peer, id: [2], text: 'theirs' };
  const deleted = { ...own, id: [3], text: null, deleted: true };
  let messages = [own, theirs, deleted];
  const row = { peerId: peer, state: 'open', lastMessage: own, unreadCount: 0 };
  const call = (command) => {
    if (command === 'chat_history') return Promise.resolve({ state: 'open', messages });
    if (command === 'mark_chat_read') return Promise.resolve(0);
    if (command === 'chat_rows') return Promise.resolve([row]);
    throw new Error(`unexpected Edit command: ${command}`);
  };
  await openChat(peer, call);
  const editButtons = chatMessages.querySelectorAll('button');
  if (editButtons.length !== 4 || editButtons[0].textContent !== window.strings['chats.edit']
    || editButtons[1].textContent !== window.strings['chats.delete']
    || editButtons[2].textContent !== window.strings['chats.forward']
    || editButtons[3].textContent !== window.strings['chats.forward']) {
    throw new Error('Edit/Delete visible outside own live text or Forward missing');
  }
  editButtons[0].click();
  if (!editDialog.open || editInput.value !== 'original' || !editSave.disabled) {
    throw new Error('Edit dialog did not load current own text');
  }
  editCancel.click();
  if (editDialog.open) throw new Error('Edit cancel left dialog open');
  editButtons[0].click();
  editInput.value = 'replacement';
  editInput.dispatchEvent(new Event('input'));
  let editedArgs;
  messages = [{ ...own, text: 'replacement', edited: true }, theirs, deleted];
  await editChatText((command, args) => {
    if (command === 'edit_chat_text') { editedArgs = args; return Promise.resolve([5]); }
    return call(command);
  });
  if (!editedArgs || editedArgs.peerId !== peer || editedArgs.text !== 'replacement'
    || JSON.stringify(editedArgs.groupId) !== JSON.stringify(groupId)
    || JSON.stringify(editedArgs.messageId) !== JSON.stringify(own.id)
    || editDialog.open || !chatMessages.textContent.includes(window.strings['chats.edited'])
    || chatComposeStatus.textContent !== window.strings['chats.editSent']) {
    throw new Error('Edit did not refresh current text and marker');
  }
  chatMessages.querySelector('button').click();
  editInput.value = 'keep draft';
  editInput.dispatchEvent(new Event('input'));
  await editChatText((command) => command === 'edit_chat_text'
    ? Promise.reject('diskFailed') : call(command));
  if (!editDialog.open || editInput.value !== 'keep draft'
    || !editError.textContent.includes('diskFailed')) {
    throw new Error('unstaged Edit lost its draft');
  }
  editInput.value = 'queued text';
  editInput.dispatchEvent(new Event('input'));
  await editChatText((command) => {
    if (command === 'edit_chat_text') {
      messages = [{ ...own, text: 'queued text', edited: true }, theirs, deleted];
      return Promise.reject('nodeFailed');
    }
    return call(command);
  });
  if (editDialog.open || !chatMessages.textContent.includes('queued text')
    || chatComposeStatus.textContent !== window.strings['chats.editQueued']) {
    throw new Error('staged Edit offered a duplicate retry');
  }
  chatMessages.querySelector('button').click();
  editInput.value = 'late';
  editInput.dispatchEvent(new Event('input'));
  let finishEdit;
  const oldEdit = editChatText((command) => command === 'edit_chat_text'
    ? new Promise((resolve) => { finishEdit = resolve; }) : call(command));
  const readyState = current;
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  finishEdit([6]);
  await oldEdit;
  if (editDialog.open || !chatDetail.hidden) throw new Error('old Edit painted new identity');
};
const checkChatDelete = async () => {
  await openArea('chats-area');
  await window.chatsDone;
  const peer = '9'.repeat(64);
  const own = { author: current.peerId, groupId: [6], id: [7], text: 'remove me',
    edited: false, deleted: false, sentAtMs: null };
  const theirs = { ...own, author: peer, id: [8], text: 'keep theirs' };
  let messages = [own, theirs];
  const row = { peerId: peer, state: 'open', lastMessage: own, unreadCount: 0 };
  const call = (command) => {
    if (command === 'chat_history') return Promise.resolve({ state: 'open', messages });
    if (command === 'mark_chat_read') return Promise.resolve(0);
    if (command === 'chat_rows') return Promise.resolve([row]);
    throw new Error(`unexpected Delete command: ${command}`);
  };
  await openChat(peer, call);
  const deleteButton = () => Array.from(chatMessages.querySelectorAll('button'))
    .find((button) => button.textContent === window.strings['chats.delete']);
  if (!deleteButton() || chatMessages.querySelectorAll('button').length !== 4) {
    throw new Error('Delete action visible for peer text or Forward missing');
  }
  deleteButton().click();
  if (!deleteDialog.open || !deleteDialog.textContent.includes(window.strings['chats.deleteWarning'])) {
    throw new Error('best-effort Delete warning missing');
  }
  deleteCancel.click();
  if (deleteDialog.open || !chatMessages.textContent.includes('remove me')) {
    throw new Error('Cancel deleted own text');
  }
  deleteButton().click();
  let deletedArgs;
  await deleteChatText((command, args) => {
    if (command === 'delete_chat_text') {
      deletedArgs = args;
      messages = [{ ...own, text: null, deleted: true }, theirs];
      return Promise.resolve([9]);
    }
    return call(command);
  });
  if (!deletedArgs || deletedArgs.peerId !== peer
    || JSON.stringify(deletedArgs.groupId) !== JSON.stringify(own.groupId)
    || JSON.stringify(deletedArgs.messageId) !== JSON.stringify(own.id)
    || deleteDialog.open || chatMessages.textContent.includes('remove me')
    || !chatMessages.textContent.includes(window.strings['chats.deleted'])
    || deleteButton() || chatComposeStatus.textContent !== window.strings['chats.deleteSent']) {
    throw new Error('Delete did not show terminal placeholder');
  }
  messages = [{ ...own, id: [10], text: 'second' }, theirs];
  await openChat(peer, call);
  deleteButton().click();
  await deleteChatText((command) => command === 'delete_chat_text'
    ? Promise.reject('diskFailed') : call(command));
  if (!deleteDialog.open || !deleteError.textContent.includes('diskFailed')
    || !chatMessages.textContent.includes('second')) {
    throw new Error('unstaged Delete hid own text');
  }
  await deleteChatText((command) => {
    if (command === 'delete_chat_text') {
      messages = [{ ...messages[0], text: null, deleted: true }, theirs];
      return Promise.reject('nodeFailed');
    }
    return call(command);
  });
  if (deleteDialog.open || chatMessages.textContent.includes('second')
    || chatComposeStatus.textContent !== window.strings['chats.deleteQueued']) {
    throw new Error('staged Delete was reported as failure');
  }
  messages = [{ ...own, id: [11], text: 'late' }, theirs];
  await openChat(peer, call);
  deleteButton().click();
  let finishDelete;
  const oldDelete = deleteChatText((command) => command === 'delete_chat_text'
    ? new Promise((resolve) => { finishDelete = resolve; }) : call(command));
  const readyState = current;
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  finishDelete([12]);
  await oldDelete;
  if (deleteDialog.open || !chatDetail.hidden) throw new Error('old Delete painted new identity');
};
const checkChatDelivery = async () => {
  await openArea('chats-area');
  await window.chatsDone;
  const peer = '8'.repeat(64);
  const own = { author: current.peerId, groupId: [1], id: [1], text: 'outgoing',
    edited: false, deleted: false, sentAtMs: null };
  const theirs = { ...own, author: peer, id: [2], text: 'incoming',
    delivery: 'nodeAccepted' };
  let delivery = 'pending';
  const call = (command) => {
    if (command === 'receive_mail') return Promise.resolve(0);
    if (command === 'chat_history') return Promise.resolve({ state: 'open',
      messages: [{ ...own, delivery }, theirs] });
    if (command === 'mark_chat_read') return Promise.resolve(0);
    if (command === 'chat_rows') return Promise.resolve([]);
    throw new Error(`unexpected delivery command: ${command}`);
  };
  await openChat(peer, call);
  if (!chatMessages.textContent.includes(window.strings['chats.delivery.pending'])
    || chatMessages.textContent.includes(window.strings['chats.delivery.nodeAccepted'])) {
    throw new Error('pending own message or peer delivery status rendered incorrectly');
  }
  delivery = 'nodeAccepted';
  await pollMailbox(call);
  if (!chatMessages.textContent.includes(window.strings['chats.delivery.nodeAccepted'])
    || chatMessages.textContent.includes(window.strings['chats.delivery.pending'])) {
    throw new Error('node acceptance did not refresh an open chat');
  }
  delivery = 'deviceReceived';
  document.getElementById('chat-back').click();
  await openChat(peer, call);
  if (!chatMessages.textContent.includes(window.strings['chats.delivery.deviceReceived'])
    || chatMessages.textContent.includes(window.strings['chats.delivery.nodeAccepted'])) {
    throw new Error('device receipt status did not survive reopening');
  }
  document.getElementById('chat-back').click();
};
const checkChatForwarded = async () => {
  await openArea('chats-area');
  await window.chatsDone;
  const peer = '7'.repeat(64);
  const forwarded = { author: peer, groupId: [1], id: [1], text: 'shared words',
    forwarded: true, edited: false, deleted: false, sentAtMs: null };
  const ordinary = { ...forwarded, id: [2], text: 'own words', forwarded: false };
  renderChatRows([{ peerId: peer, state: 'open', lastMessage: forwarded, unreadCount: 0 }]);
  const preview = document.querySelector('#chat-list button .caption');
  if (!preview?.textContent.startsWith(`${window.strings['chats.forwarded']} · shared words`)) {
    throw new Error('forwarded label missing from chat list preview');
  }
  await openChat(peer, (command) => {
    if (command === 'chat_history') return Promise.resolve({ state: 'open', messages: [forwarded, ordinary] });
    if (command === 'mark_chat_read') return Promise.resolve(0);
    if (command === 'chat_rows') return Promise.resolve([]);
    throw new Error(`unexpected forwarded command: ${command}`);
  });
  const messages = chatMessages.querySelectorAll('p');
  if (messages.length !== 2
    || !messages[0].textContent.includes(window.strings['chats.forwarded'])
    || messages[1].textContent.includes(window.strings['chats.forwarded'])
    || !messages[0].textContent.startsWith(shortPeerId(peer, knownPeers))) {
    throw new Error('forwarded history marker or verified MLS sender was lost');
  }
  document.getElementById('chat-back').click();
};
const checkChatForwardAction = async () => {
  await openArea('chats-area');
  await window.chatsDone;
  const sourcePeer = '4'.repeat(64);
  const targetPeer = '5'.repeat(64);
  const requestPeer = '6'.repeat(64);
  const source = { author: sourcePeer, groupId: [3], id: [7], text: 'share this',
    forwarded: false, deleted: false, edited: false, sentAtMs: null };
  const rows = [
    { peerId: sourcePeer, state: 'open', lastMessage: source, unreadCount: 0 },
    { peerId: targetPeer, state: 'open', lastMessage: null, unreadCount: 0 },
    { peerId: requestPeer, state: 'requestReceived', lastMessage: null, unreadCount: 0 },
  ];
  let targetMessages = [];
  let sendError = null;
  let sentArgs;
  const call = (command, args) => {
    if (command === 'chat_history') return Promise.resolve({ state: 'open',
      messages: args.peerId === sourcePeer ? [source] : targetMessages });
    if (command === 'mark_chat_read') return Promise.resolve(0);
    if (command === 'chat_rows') return Promise.resolve(rows);
    if (command === 'forward_chat_text') {
      sentArgs = args;
      if (sendError !== 'diskFailed') {
        targetMessages = [{ ...source, author: current.peerId, groupId: [8], id: [9],
          forwarded: true, delivery: 'pending' }];
      }
      return sendError ? Promise.reject(sendError) : Promise.resolve([9]);
    }
    throw new Error(`unexpected Forward command: ${command}`);
  };
  renderChatRows(rows);
  await openChat(sourcePeer, call);
  const forwardButton = () => Array.from(chatMessages.querySelectorAll('button'))
    .find((button) => button.textContent === window.strings['chats.forward']);
  forwardButton().click();
  if (!forwardDialog.open || forwardSelect.options.length !== 1
    || forwardSelect.value !== targetPeer) throw new Error('Forward picker included a request or source');
  forwardCancel.click();
  if (forwardDialog.open || sentArgs) throw new Error('Forward cancel staged a message');
  forwardButton().click();
  sendError = 'diskFailed';
  await forwardChatText(call);
  if (!forwardDialog.open || !forwardError.textContent.includes('diskFailed')) {
    throw new Error('unstaged Forward closed the target picker');
  }
  sendError = null;
  await forwardChatText(call);
  if (!sentArgs || sentArgs.sourcePeerId !== sourcePeer || sentArgs.sourceAuthor !== sourcePeer
    || JSON.stringify(sentArgs.sourceGroupId) !== JSON.stringify(source.groupId)
    || JSON.stringify(sentArgs.sourceMessageId) !== JSON.stringify(source.id)
    || sentArgs.targetPeerId !== targetPeer || forwardDialog.open
    || activeChat?.peerId !== targetPeer
    || chatComposeStatus.textContent !== window.strings['chats.forwardSent']) {
    throw new Error('Forward action did not send the selected source to the open target');
  }
  await openChat(sourcePeer, call);
  targetMessages = [];
  sendError = 'nodeFailed';
  forwardButton().click();
  await forwardChatText(call);
  if (forwardDialog.open || activeChat?.peerId !== targetPeer
    || chatComposeStatus.textContent !== window.strings['chats.forwardQueued']) {
    throw new Error('staged Forward was reported as unstaged failure');
  }
  await openChat(sourcePeer, call);
  renderChatRows([rows[0]]);
  forwardButton().click();
  if (!forwardDialog.open || !forwardConfirm.disabled
    || !forwardError.textContent.includes(window.strings['chats.forwardNoTargets'])) {
    throw new Error('Forward picker did not reject an absent target');
  }
  document.getElementById('chat-back').click();
  if (forwardDialog.open) throw new Error('Forward dialog survived chat navigation');
  renderChatRows(rows);
  await openChat(sourcePeer, call);
  forwardButton().click();
  let finishForward;
  let commandStarted;
  const started = new Promise((resolve) => { commandStarted = resolve; });
  const late = forwardChatText((command, args) => command === 'forward_chat_text'
    ? new Promise((resolve) => { finishForward = resolve; commandStarted(); }) : call(command, args));
  await started;
  const readyState = current;
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  finishForward([10]);
  await late;
  if (forwardDialog.open || !chatDetail.hidden) throw new Error('old Forward painted new identity');
};
globalThis.uiSmokeChats = Object.freeze({
  checkChatProjection, checkFirstMessage, checkClearChat, checkChatBlock, checkChatEdit,
  checkChatDelete, checkChatDelivery, checkChatForwarded, checkChatForwardAction,
});
