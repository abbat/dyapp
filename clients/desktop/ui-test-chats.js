// Desktop UI smoke: Chats list, first text, send, Clear and the two-client scenario.
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
    || !request.textContent.includes(window.strings['chats.state.requestReceived'])
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
    || document.querySelector('#chats-requests .meta .badge')) {
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
    || activeChat.state !== 'open'
    || document.querySelector('#chats-requests button')
    || document.querySelector(`#chat-list button[data-peer-id="${peer}"]`)
      ?.textContent.includes(window.strings['chats.state.requestReceived'])) {
    throw new Error('reply did not open conversation or remove request marker');
  }

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
const checkNewDevice = async () => {
  await openArea('chats-area');
  await window.chatsDone;
  const peer = '9'.repeat(64);
  const notice = document.getElementById('chat-new-device-notice');
  const status = document.getElementById('chats-status');
  let raised = true;
  let stop = null;
  let acknowledge = () => Promise.reject(new Error('busy'));
  const call = (command) => {
    if (command === 'chat_history') return Promise.resolve({ state: 'open', messages: [] });
    if (command === 'contact_new_device') {
      if (stop) {
        stop.reached();
        return stop.answered;
      }
      return raised === null ? Promise.reject(new Error('locked')) : Promise.resolve(raised);
    }
    if (command === 'acknowledge_new_device') return acknowledge();
    if (command === 'mark_chat_read') return Promise.resolve(0);
    if (command === 'chat_rows') return Promise.resolve([]);
    throw new Error(`unexpected new-device command: ${command}`);
  };
  await openChat(peer, call);
  if (notice.hidden) throw new Error('a raised new-device notice stayed hidden');
  await shot('chat-new-device');
  for (const flag of [true, null]) {
    raised = flag;
    await window.mailUpdated(0, call);
    if (notice.hidden) throw new Error('a mail pass hid the new-device notice');
  }
  raised = true;
  await acknowledgeDevice(call);
  if (notice.hidden || !status.textContent.includes(window.strings['chats.newDevice.failed'])) {
    throw new Error('a failed acknowledgement dismissed the notice silently');
  }
  acknowledge = () => {
    raised = false;
    return Promise.resolve();
  };
  await acknowledgeDevice(call);
  if (!notice.hidden || document.activeElement !== chatInput) {
    throw new Error('the acknowledgement left the notice or lost focus');
  }
  raised = true;
  await openChat(peer, call);
  if (notice.hidden) throw new Error('reopening lost a raised notice');
  document.getElementById('chat-back').click();
  if (!notice.hidden) throw new Error('leaving the chat kept the notice');
  // An answer for a chat left behind, or for an earlier identity, shows nothing.
  const departures = [
    () => document.getElementById('chat-back').click(),
    () => { identityGeneration += 1; },
  ];
  for (const leave of departures) {
    const held = {};
    held.asked = new Promise((resolve) => { held.reached = resolve; });
    held.answered = new Promise((resolve) => { held.answer = resolve; });
    stop = held;
    const opening = openChat(peer, call);
    await held.asked;
    leave();
    held.answer(true);
    await opening;
    if (!notice.hidden) throw new Error('a stale answer raised the new-device notice');
  }
  stop = null;
  document.getElementById('chat-back').click();
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
  await window.mailUpdated(0, call);
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
  // A lost ACK fails the pass after the commit; the committed row still appears, once.
  const committed = [{ peerId: peer, state: 'open', lastMessage: theirs, unreadCount: 1 }];
  await window.mailUpdated(null, (command) => {
    if (command === 'chat_rows') return Promise.resolve(committed);
    throw new Error(`unexpected refresh command: ${command}`);
  });
  if (document.querySelectorAll('#chat-list button').length !== 1
    || !chatList.textContent.includes('incoming')) {
    throw new Error('a failed pass hid the committed message');
  }
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
const checkChatLinkHosts = async () => {
  await openArea('chats-area');
  await window.chatsDone;
  const peer = '3'.repeat(64);
  const text = 'See https://bücher.example/path and https://paypal.com@evil.example/login';
  const hosts = chatLinkHosts(text);
  if (hosts.length !== 2 || hosts[0] !== 'xn--bcher-kva.example'
    || hosts[1] !== 'evil.example' || chatLinkHosts('javascript:alert(1)').length) {
    throw new Error('IDN or userinfo link host was misidentified');
  }
  const message = { author: peer, groupId: [1], id: [1], text,
    deleted: false, edited: false, sentAtMs: null };
  await openChat(peer, (command) => {
    if (command === 'chat_history') return Promise.resolve({ state: 'requestReceived', messages: [message] });
    if (command === 'mark_chat_read') return Promise.resolve(0);
    if (command === 'chat_rows') return Promise.resolve([]);
    throw new Error(`unexpected link host command: ${command}`);
  });
  const rendered = chatMessages.querySelector('p');
  if (!rendered?.textContent.includes(`${window.strings['chats.linkHost']}: ${hosts.join(', ')}`)
    || rendered.querySelector('a, img, iframe')) {
    throw new Error('link host missing or link preview was created');
  }
  document.getElementById('chat-back').click();
};
const checkChatRead = async () => {
  await openArea('chats-area');
  await window.chatsDone;
  const peer = '5'.repeat(64);
  const own = { author: current.peerId, groupId: [1], text: 'seen', deleted: false, edited: false,
    delivery: 'nodeAccepted', sentAtMs: null };
  const messages = [{ ...own, id: [1], read: true }, { ...own, id: [2], text: 'later', read: false }];
  const sent = [];
  const call = (command) => {
    sent.push(command);
    if (command === 'chat_history') return Promise.resolve({ state: 'open', messages });
    return Promise.resolve(command === 'chat_rows' ? [] : 0);
  };
  Object.defineProperty(document, 'hidden', { configurable: true, value: true });
  await openChat(peer, call);
  const [seen, later] = chatMessages.querySelectorAll('.msg');
  const delivered = window.strings['chats.delivery.nodeAccepted'];
  if (!seen.textContent.includes(window.strings['chats.delivery.read'])
    || seen.textContent.includes(delivered) || !later.textContent.includes(delivered)
    || later.textContent.includes(window.strings['chats.delivery.read'])) {
    throw new Error('read mark is not on the covered text alone');
  }
  if (sent.includes('mark_chat_read') || sent.includes('send_read_receipts')) {
    throw new Error('a hidden window counted as reading');
  }
  delete document.hidden;
  const shown = window.mailUpdated;
  let pass;
  window.mailUpdated = (changed) => { pass = shown(changed, call); };
  document.dispatchEvent(new Event('visibilitychange'));
  window.mailUpdated = shown;
  await pass;
  await window.chatRead;
  if (!sent.includes('mark_chat_read') || !sent.includes('send_read_receipts')) {
    throw new Error('returning to the window did not read the chat');
  }
  document.getElementById('chat-back').click();
};
// Mail from the other client arrives on a later Chats refresh, so live checks poll for it.
const livePoll = async (check, what) => {
  for (let attempt = 0; attempt < 30; attempt += 1) {
    if (await check()) return;
    await new Promise((resolve) => { setTimeout(resolve, 500); });
  }
  throw new Error(`timed out waiting for ${what}`);
};
const liveChats = async (ready, what) => {
  await openArea('chats-area');
  await livePoll(async () => {
    await window.chatsDone;
    if (ready()) return true;
    window.chatsDone = window.showChats();
    return false;
  }, what);
};
const liveFound = async (peer) => {
  await openArea('search-area');
  document.getElementById('run-search').click();
  await window.searchDone;
  const card = document.querySelector(`#search-results [data-peer-id="${peer}"]`);
  if (!card) throw new Error('search did not find the other client');
  card.click();
};
const liveRow = (list, peer) => document.querySelector(`#${list} [data-peer-id="${peer}"]`);
const liveOpen = async (row) => {
  row.click();
  await window.chatOpened;
  if (chatDetail.hidden) throw new Error('chat did not open');
};
const liveSend = async (text, results = ['chats.sent']) => {
  chatInput.value = text;
  chatInput.dispatchEvent(new Event('input'));
  document.getElementById('chat-composer').requestSubmit();
  await window.chatSent;
  const status = chatComposeStatus.textContent;
  if (!results.some((key) => window.strings[key] === status)) {
    throw new Error(`sending "${text}" shows "${status}"`);
  }
};
// Clicks Edit or Delete on the own message with this text.
const liveAction = (text, action) => {
  const message = [...chatMessages.querySelectorAll('.msg.out')]
    .find((item) => item.textContent.includes(text));
  [...message.querySelectorAll('button')]
    .find((button) => button.textContent === window.strings[action]).click();
};
const liveRefused = async (command, args, code) => {
  const error = await invoke(command, args).then(() => null, String);
  if (!error || (code && error !== code)) throw new Error(`${command} answered ${error}`);
};
const liveHistory = async (peer) => {
  const history = await invoke('chat_history', { peerId: peer });
  return history.messages.map((message) => message.text);
};
const liveLikes = async (peer, ours) => {
  if ((await invoke('peer_likes', { peerId: peer })).ours !== ours) {
    throw new Error(`own like is not ${ours}`);
  }
};
const liveUnlikeShown = () => livePoll(() => !document.getElementById('view-unlike').hidden,
  'the stored like');
const liveState = (state) => {
  if (activeChat?.state !== state) throw new Error(`chat is ${activeChat?.state}, not ${state}`);
};
// One step of the two-client scenario; each runs in a fresh process against the same node.
const peerSteps = {
  like: async (peer) => {
    await liveFound(peer);
    await liveLikes(peer, false);
    document.getElementById('view-like').click();
    await window.likeDone;
    const status = document.getElementById('view-status').textContent;
    if (document.getElementById('view-unlike').hidden || !document.getElementById('view-like').hidden
      || !document.getElementById('view-match').hidden || status) {
      throw new Error(`Like left the wrong profile actions: ${status}`);
    }
  },
  'like-restart': async (peer) => {
    await liveFound(peer);
    await liveUnlikeShown();
    await openArea('chats-area');
    await window.chatsDone;
    if (liveRow('chats-liked-list', peer)) throw new Error('own like shown in own Liked you');
  },
  'liked-you': async (peer) => {
    await liveChats(() => liveRow('chats-liked-list', peer), 'the like');
    if (document.getElementById('chats-liked-count').textContent !== '1') {
      throw new Error('Liked you count is not 1');
    }
    await liveLikes(peer, false);
  },
  unlike: async (peer) => {
    await liveFound(peer);
    await liveUnlikeShown();
    document.getElementById('view-unlike').click();
    document.getElementById('unlike-confirm').click();
    await window.likeDone;
    if (document.getElementById('view-like').hidden) throw new Error('Unlike kept the like');
  },
  'liked-gone': async (peer) => {
    await liveChats(() => !liveRow('chats-liked-list', peer), 'the unlike');
    if (document.getElementById('chats-liked-count').textContent !== '0') {
      throw new Error('Liked you count is not 0');
    }
  },
  'chat-first': async (peer) => {
    await liveFound(peer);
    document.getElementById('view-message').click();
    await Promise.all([window.chatOpened, window.chatsDone]);
    liveState('none');
    if (!document.getElementById('chat-clear').hidden) throw new Error('Clear shown without a chat');
    await liveSend('first text');
    liveState('requestSent');
    if (!chatInput.disabled || document.getElementById('chat-first-notice').hidden
      || !liveRow('chat-list', peer)) {
      throw new Error('the first text left no pending request');
    }
    await liveRefused('send_chat_text', { peerId: peer, text: 'second text' });
    if ((await liveHistory(peer)).join() !== 'first text') throw new Error('second text was stored');
  },
  'chat-reply': async (peer) => {
    await liveChats(() => liveRow('chats-requests', peer), 'the request');
    if (!liveRow('chats-requests', peer).querySelector('.meta .badge')) {
      throw new Error('request row has no unread badge');
    }
    await liveOpen(liveRow('chats-requests', peer));
    liveState('requestReceived');
    if (!chatMessages.textContent.includes('first text')
      || document.getElementById('chat-request-notice').hidden
      || document.getElementById('request-block').hidden
      || !document.getElementById('chat-block').hidden) {
      throw new Error('request view is incomplete');
    }
    await liveSend('reply text');
    liveState('open');
    if (liveRow('chats-requests', peer) || !liveRow('chat-list', peer)) {
      throw new Error('the reply did not move the row');
    }
  },
  'chat-open': async (peer) => {
    await liveChats(() => liveRow('chat-list', peer)?.textContent.includes('reply text'), 'the reply');
    const privacy = await invoke('privacy_settings');
    await invoke('set_privacy', { privacy: { ...privacy, sendReadReceipts: true } });
    await liveOpen(liveRow('chat-list', peer));
    await window.chatRead;
    if (await invoke('send_read_receipts', { peerId: peer }) !== 0) {
      throw new Error('the read receipt is still due');
    }
    liveState('open');
    if (chatInput.disabled) throw new Error('the reply left the composer disabled');
    await liveSend('later text');
    liveAction('later text', 'chats.edit');
    editInput.value = 'edited text';
    editInput.dispatchEvent(new Event('input'));
    document.getElementById('chat-edit-form').requestSubmit();
    await window.chatEdited;
    if (chatComposeStatus.textContent !== window.strings['chats.editSent']) {
      throw new Error(`edit shows ${chatComposeStatus.textContent}`);
    }
    await liveSend('gone text');
    liveAction('gone text', 'chats.delete');
    document.getElementById('chat-delete-confirm').click();
    await window.chatDeleted;
    if (chatComposeStatus.textContent !== window.strings['chats.deleteSent']) {
      throw new Error(`delete shows ${chatComposeStatus.textContent}`);
    }
  },
  // Nothing here opens Chats or asks for a pass, so the mail can only come from the runtime job.
  'catch-up': async (peer) => {
    await new Promise((resolve) => {
      const shown = window.mailUpdated;
      window.mailUpdated = (count) => { shown(count); resolve(); };
    });
    if (!(await liveHistory(peer)).includes('edited text')) {
      throw new Error('the background pass stored no mail');
    }
  },
  'chat-clear': async (peer) => {
    const deleted = window.strings['chats.deleted'];
    await liveChats(() => liveRow('chat-list', peer)?.textContent.includes(deleted), 'the delete');
    await liveOpen(liveRow('chat-list', peer));
    const shownText = chatMessages.textContent;
    if (!shownText.includes(`edited text (${window.strings['chats.edited']})`)
      || !shownText.includes(deleted) || shownText.includes('later text')
      || shownText.includes('gone text')) {
      throw new Error(`edit or delete did not arrive: ${shownText}`);
    }
    const replied = [...chatMessages.querySelectorAll('.msg')]
      .find((item) => item.textContent.includes('reply text'));
    if (!replied?.textContent.includes(window.strings['chats.delivery.read'])) {
      throw new Error('the read receipt left no mark on the reply');
    }
    document.getElementById('chat-clear').click();
    if (!clearDialog.open) throw new Error('Clear asked for no confirmation');
    document.getElementById('chat-clear-confirm').click();
    await window.chatCleared;
    liveState('open');
    if (chatMessages.querySelector('.msg') || document.getElementById('chat-cleared').hidden
      || chatInput.disabled) {
      throw new Error('Clear did not empty a writable chat');
    }
    await liveSend('after clear');
  },
  'chat-block': async (peer) => {
    await liveChats(() => liveRow('chat-list', peer)?.textContent.includes('after clear'), 'the text after Clear');
    await liveOpen(liveRow('chat-list', peer));
    const { messages } = await invoke('chat_history', { peerId: peer });
    // The peer opened these with the default setting, and this side's setting survived a restart.
    if (messages.some((message) => message.read)
      || !(await invoke('privacy_settings')).sendReadReceipts) {
      throw new Error('default-off read receipts or the setting went wrong');
    }
    document.getElementById('chat-block').click();
    await window.chatBlocked;
    const status = document.getElementById('chats-status').textContent;
    if (!chatDetail.hidden || liveRow('chat-list', peer)
      || !['confirmed', 'notNeeded'].some((key) => window.strings[`chats.blocked.${key}`] === status)) {
      throw new Error(`Block left the chat: ${status}`);
    }
    await liveRefused('send_chat_text', { peerId: peer, text: 'blocked text' }, 'blocked');
    await liveRefused('chat_history', { peerId: peer }, 'blocked');
  },
  'chat-after-block': async (peer) => {
    await liveChats(() => liveRow('chat-list', peer), 'the chat');
    await liveOpen(liveRow('chat-list', peer));
    if ((await liveHistory(peer)).join() !== 'after clear') throw new Error('Clear did not survive');
    await liveSend('after block', ['chats.sent', 'chats.queued']);
  },
  'chat-blocked': async (peer) => {
    await openArea('chats-area');
    await window.chatsDone;
    if (liveRow('chat-list', peer) || liveRow('chats-requests', peer)) {
      throw new Error('blocked peer is back in Chats');
    }
    await invoke('unblock_peer', { peerId: peer });
    if ((await liveHistory(peer)).includes('after block')) throw new Error('blocked text was stored');
  },
};
const checkPeerStep = (kind, peer) => peerSteps[kind](peer);
globalThis.uiSmokeChats = Object.freeze({
  checkChatProjection, checkFirstMessage, checkClearChat, checkChatBlock, checkChatEdit,
  checkChatDelete, checkChatDelivery, checkChatForwarded, checkChatForwardAction, checkChatLinkHosts,
  checkNewDevice, checkChatRead, checkPeerStep,
});
