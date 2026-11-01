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
globalThis.uiSmokeChats = Object.freeze({
  checkChatProjection, checkFirstMessage, checkClearChat,
});
