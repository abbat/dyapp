// Scoped fixture/assertion helpers and shared smoke scenarios.
{
  // Desktop UI smoke: Chats list, first text, send, Clear and the two-client scenario.
  // Evaluated after identity.js in the same page scope.
  // The local projection renders requests, unread counts and original timestamps without a node.
  const assertChatProjectionLocalChatsRowsDisappearedWhenTheNode = (request, conversation) => {
    if (
      !request ||
      !conversation ||
      !request.textContent.includes('hello') ||
      !request.textContent.includes(appApi.strings['chats.state.requestReceived']) ||
      !request.querySelector('.meta .caption').textContent ||
      !request.textContent.includes('2') ||
      !conversation.textContent.includes(appApi.strings['chats.deleted']) ||
      document.getElementById('chats-status').textContent !== appApi.strings['chats.notConnected']
    ) {
      throw new Error('local Chats rows disappeared when the node failed');
    }
  };
  const assertChatProjectionChatHistoryDidNotClearLocalUnread = (marked) => {
    if (
      !marked ||
      appBindings.chatDetail.hidden ||
      appBindings.chatMessages.textContent.indexOf('hello') < 0 ||
      document.querySelector('#chats-requests .meta .badge')
    ) {
      throw new Error('chat history did not clear local unread');
    }
  };
  const assertChatProjectionReplyDidNotOpenConversationOrRemove = (sentArgs, peer) => {
    if (
      sentArgs.peerId !== peer ||
      sentArgs.text !== 'reply' ||
      appBindings.chatInput.value ||
      appBindings.activeChat.state !== 'open' ||
      document.querySelector('#chats-requests button') ||
      document
        .querySelector(`#chat-list button[data-peer-id="${peer}"]`)
        ?.textContent.includes(appApi.strings['chats.state.requestReceived'])
    ) {
      throw new Error('reply did not open conversation or remove request marker');
    }
  };
  const assertChatProjectionSecondUnansweredFirstMessageIsEnabled = () => {
    if (
      !appBindings.chatInput.disabled ||
      !appBindings.chatSend.disabled ||
      document.getElementById('chat-first-notice').hidden
    ) {
      throw new Error('second unanswered first message is enabled');
    }
  };
  const assertChatProjectionAClosedChatCanStillBeWritten = () => {
    if (
      !appBindings.chatInput.disabled ||
      !appBindings.chatSend.disabled ||
      document.getElementById('chat-closed-notice').hidden ||
      [...appBindings.chatMessages.querySelectorAll('button')].some(
        (button) => button.textContent === appApi.strings['chats.edit'],
      )
    ) {
      throw new Error('a closed chat can still be written to');
    }
  };
  const assertChatProjectionFailedSendLostTheUnstagedDraft = () => {
    if (
      appBindings.chatInput.value !== 'keep draft' ||
      !appBindings.chatComposeStatus.textContent.includes('notConnected')
    ) {
      throw new Error('failed send lost the unstaged draft');
    }
  };
  const assertChatProjectionAnInviteWithoutStockDidNotSay = () => {
    if (
      appBindings.chatInput.value !== 'keep draft' ||
      appBindings.chatComposeStatus.textContent !== appApi.strings['chats.peerUnavailable']
    ) {
      throw new Error('an invite without stock did not say so or lost the draft');
    }
  };
  const checkChatProjectionStale = async (peer, message, rows) => {
    const readyState = appBindings.current;
    const finishHistoryCall = smoke.pendingSmokeCall();
    const oldHistory = appBindings.openChat(peer, () => finishHistoryCall.call());
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    finishHistoryCall.resolve({ state: 'open', messages: [message] });
    await oldHistory;
    if (!appBindings.chatDetail.hidden)
      throw new Error('old chat history opened after identity switch');
    await appBindings.openChat(peer, (command) => {
      if (command === 'chat_history')
        return Promise.resolve({ state: 'open', messages: [message] });
      if (command === 'mark_chat_read') return Promise.resolve(0);
      if (command === 'chat_rows') return Promise.resolve(rows);
      throw new Error(`unexpected stale-send setup: ${command}`);
    });
    appBindings.chatInput.value = 'late';
    appBindings.chatInput.dispatchEvent(new Event('input'));
    const finishSendCall = smoke.pendingSmokeCall();
    const oldSend = appBindings.sendChatText(() => finishSendCall.call());
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    finishSendCall.resolve([1]);
    await oldSend;
    if (!appBindings.chatDetail.hidden || appBindings.chatInput.value)
      throw new Error('old send painted new identity');
  };
  const checkChatProjectionSendFailures = async (peer, message, reply, rows) => {
    await appBindings.openChat(peer, (command) => {
      if (command === 'chat_history')
        return Promise.resolve({ state: 'open', messages: [message] });
      if (command === 'mark_chat_read') return Promise.resolve(0);
      if (command === 'chat_rows') return Promise.resolve(rows);
      throw new Error(`unexpected open command: ${command}`);
    });
    appBindings.chatInput.value = 'keep draft';
    appBindings.chatInput.dispatchEvent(new Event('input'));
    await appBindings.sendChatText((command) => {
      if (command === 'send_chat_text') return Promise.reject('notConnected');
      if (command === 'chat_history')
        return Promise.resolve({ state: 'open', messages: [message] });
      throw new Error(`unexpected failed send command: ${command}`);
    });
    assertChatProjectionFailedSendLostTheUnstagedDraft();
    await appBindings.sendChatText((command) => {
      if (command === 'send_chat_text') return Promise.reject('peerUnavailable');
      if (command === 'chat_history')
        return Promise.resolve({ state: 'open', messages: [message] });
      throw new Error(`unexpected unavailable send command: ${command}`);
    });
    assertChatProjectionAnInviteWithoutStockDidNotSay();
    await appBindings.sendChatText((command) => {
      if (command === 'send_chat_text') return Promise.reject('nodeFailed');
      if (command === 'chat_history')
        return Promise.resolve({ state: 'open', messages: [message, reply] });
      if (command === 'mark_chat_read') return Promise.resolve(0);
      if (command === 'chat_rows') return Promise.resolve(rows);
      throw new Error(`unexpected staged send command: ${command}`);
    });
    if (
      appBindings.chatInput.value ||
      appBindings.chatComposeStatus.textContent !== appApi.strings['chats.queued']
    ) {
      throw new Error('staged failed send could be duplicated');
    }
    document.getElementById('chat-back').click();
    if (!appBindings.chatDetail.hidden || appBindings.chatOverview.hidden)
      throw new Error('chat Back did not show the list');
    await checkChatProjectionStale(peer, message, rows);
  };
  const checkChatProjectionWriteGuards = async (peer, message, reply, rows) => {
    await appBindings.openChat(peer, (command) => {
      if (command === 'chat_history')
        return Promise.resolve({ state: 'requestSent', messages: [reply] });
      if (command === 'mark_chat_read') return Promise.resolve(0);
      if (command === 'chat_rows') return Promise.resolve(rows);
      throw new Error(`unexpected first-message command: ${command}`);
    });
    assertChatProjectionSecondUnansweredFirstMessageIsEnabled();

    await appBindings.openChat(peer, (command) => {
      if (command === 'chat_history') {
        return Promise.resolve({
          state: 'closed',
          messages: [{ ...reply, author: appBindings.current.peerId }],
        });
      }
      if (command === 'mark_chat_read') return Promise.resolve(0);
      if (command === 'chat_rows') return Promise.resolve(rows);
      throw new Error(`unexpected closed chat command: ${command}`);
    });
    assertChatProjectionAClosedChatCanStillBeWritten();

    await checkChatProjectionSendFailures(peer, message, reply, rows);
  };
  const checkChatProjectionReply = async (peer, message, rows) => {
    appBindings.chatInput.value = 'reply';
    appBindings.chatInput.dispatchEvent(new Event('input'));
    let sentArgs;
    const reply = {
      author: appBindings.current.peerId,
      id: [1],
      text: 'reply',
      deleted: false,
      sentAtMs: null,
    };
    await appBindings.sendChatText((command, args) => {
      if (command === 'send_chat_text') {
        sentArgs = args;
        return Promise.resolve([1]);
      }
      if (command === 'chat_history')
        return Promise.resolve({ state: 'open', messages: [message, reply] });
      if (command === 'mark_chat_read') return Promise.resolve(0);
      if (command === 'chat_rows')
        return Promise.resolve([{ ...rows[0], state: 'open', unreadCount: 0 }]);
      throw new Error(`unexpected reply command: ${command}`);
    });
    assertChatProjectionReplyDidNotOpenConversationOrRemove(sentArgs, peer);

    await checkChatProjectionWriteGuards(peer, message, reply, rows);
  };
  const checkChatProjection = async () => {
    await openArea('chats-area');
    await appApi.chatsDone;
    const peer = 'd'.repeat(64);
    const other = 'e'.repeat(64);
    const message = { author: peer, text: 'hello', deleted: false, sentAtMs: 1_700_000_000_000 };
    const rows = [
      { peerId: peer, state: 'requestReceived', lastMessage: message, unreadCount: 2 },
      {
        peerId: other,
        state: 'open',
        lastMessage: { text: null, deleted: true, sentAtMs: null },
        unreadCount: 0,
      },
    ];
    await appApi.showChats((command) => {
      if (command === 'receive_mail') return Promise.reject('notConnected');
      if (command === 'chat_rows') return Promise.resolve(rows);
      if (command === 'liked_you') return Promise.reject('notConnected');
      throw new Error(`unexpected Chats command: ${command}`);
    });
    const request = document.querySelector('#chats-requests button');
    const conversation = document.querySelector('#chat-list button');
    assertChatProjectionLocalChatsRowsDisappearedWhenTheNode(request, conversation);
    let marked = false;
    await appBindings.openChat(peer, (command) => {
      if (command === 'chat_history')
        return Promise.resolve({ state: 'requestReceived', messages: [message] });
      if (command === 'mark_chat_read') {
        marked = true;
        return Promise.resolve(1);
      }
      if (command === 'chat_rows')
        return Promise.resolve([{ ...rows[0], unreadCount: 0 }, rows[1]]);
      throw new Error(`unexpected chat command: ${command}`);
    });
    assertChatProjectionChatHistoryDidNotClearLocalUnread(marked);
    await checkChatProjectionReply(peer, message, rows);
  };
  const assertFirstMessageFirstMessageDetailDidNotOpen = (found) => {
    if (
      appBindings.activeChat?.peerId !== found.peerId ||
      appBindings.activeChat.state !== 'none' ||
      appBindings.chatInput.disabled ||
      !document.getElementById('chat-clear').hidden
    ) {
      throw new Error('first-message detail did not open');
    }
  };
  const assertFirstMessageFirstTextDidNotCreateAPending = (sent, found) => {
    if (
      sent.peerId !== found.peerId ||
      sent.text !== 'first text' ||
      appBindings.activeChat.state !== 'requestSent' ||
      !appBindings.chatInput.disabled ||
      !document.querySelector('#chat-list button')
    ) {
      throw new Error('first text did not create a pending request row');
    }
  };
  const checkFirstMessage = async () => {
    await openArea('search-area');
    const found = {
      peerId: 'a'.repeat(64),
      draft: { birthYear: null, birthMonth: null, birthDay: null },
      place: '',
    };
    const card = appBindings.resultCard(found, 'first-message-profile');
    appBindings.searchResults.append(card);
    appBindings.openFound(found, card, () => Promise.resolve({ ours: false, matched: false }));
    const messageButton = document.getElementById('view-message');
    if (messageButton.disabled) throw new Error('Found profile cannot start a message');
    messageButton.click();
    await appApi.chatOpened;
    assertFirstMessageFirstMessageDetailDidNotOpen(found);
    appBindings.chatInput.value = 'first text';
    appBindings.chatInput.dispatchEvent(new Event('input'));
    let sent;
    const own = {
      author: appBindings.current.peerId,
      id: [7],
      text: 'first text',
      deleted: false,
      sentAtMs: null,
    };
    await appBindings.sendChatText((command, args) => {
      if (command === 'send_chat_text') {
        sent = args;
        return Promise.resolve([7]);
      }
      if (command === 'chat_history')
        return Promise.resolve({ state: 'requestSent', messages: [own] });
      if (command === 'mark_chat_read') return Promise.resolve(0);
      if (command === 'chat_rows')
        return Promise.resolve([
          { peerId: found.peerId, state: 'requestSent', lastMessage: own, unreadCount: 0 },
        ]);
      throw new Error(`unexpected first text command: ${command}`);
    });
    assertFirstMessageFirstTextDidNotCreateAPending(sent, found);
    let second = 0;
    await appBindings.sendChatText(() => {
      second += 1;
      throw new Error('second request sent');
    });
    if (second) throw new Error('second unanswered text was sent');
    appBindings.showArea('search-area');
    appBindings.closeFound();
  };
  const assertClearChatClearChatConfirmationMissing = () => {
    if (
      !appBindings.clearDialog.open ||
      !appBindings.clearDialog.textContent.includes(appApi.strings['chats.clearWarning'])
    ) {
      throw new Error('Clear chat confirmation missing');
    }
  };
  const assertClearChatFailedClearLostChat = () => {
    if (
      !appBindings.clearDialog.open ||
      !appBindings.clearError.textContent.includes('diskFailed') ||
      !appBindings.chatMessages.textContent.includes('private')
    )
      throw new Error('failed Clear lost chat');
  };
  const assertClearChatClearDidNotKeepTheEmptyConversation = (clearCalls) => {
    if (
      clearCalls !== 1 ||
      appBindings.clearDialog.open ||
      appBindings.chatMessages.textContent !== '' ||
      document.getElementById('chat-cleared').hidden ||
      appBindings.activeChat.state !== 'open' ||
      !document.querySelector('#chat-list button[data-peer-id]')
    ) {
      throw new Error('Clear did not keep the empty conversation');
    }
  };
  const clearChatFixture = () => {
    const peer = 'c'.repeat(64);
    const message = { author: peer, id: [3], text: 'private', deleted: false, sentAtMs: null };
    const row = { peerId: peer, state: 'open', lastMessage: message, unreadCount: 0 };
    const history = { state: 'open', messages: [message] };
    const open = (currentHistory) =>
      appBindings.openChat(peer, (command) => {
        if (command === 'chat_history') return Promise.resolve(currentHistory);
        if (command === 'mark_chat_read') return Promise.resolve(0);
        if (command === 'chat_rows') return Promise.resolve([row]);
        throw new Error(`unexpected clear setup: ${command}`);
      });
    return { peer, message, row, history, open };
  };
  const checkClearChat = async () => {
    await openArea('chats-area');
    await appApi.chatsDone;
    const { row, history, open } = clearChatFixture();
    await open(history);
    document.getElementById('chat-clear').click();
    assertClearChatClearChatConfirmationMissing();
    appBindings.clearCancel.click();
    if (appBindings.clearDialog.open || !appBindings.chatMessages.textContent.includes('private')) {
      throw new Error('Cancel cleared the chat');
    }
    document.getElementById('chat-clear').click();
    await appBindings.clearChat((command) => {
      if (command === 'clear_chat') return Promise.reject('diskFailed');
      throw new Error(`unexpected failed clear: ${command}`);
    });
    assertClearChatFailedClearLostChat();
    let clearCalls = 0;
    await appBindings.clearChat((command) => {
      if (command === 'clear_chat') {
        clearCalls += 1;
        return Promise.resolve(1);
      }
      if (command === 'chat_history') return Promise.resolve({ state: 'open', messages: [] });
      if (command === 'mark_chat_read') return Promise.resolve(0);
      if (command === 'chat_rows') return Promise.resolve([{ ...row, lastMessage: null }]);
      throw new Error(`unexpected clear command: ${command}`);
    });
    assertClearChatClearDidNotKeepTheEmptyConversation(clearCalls);
    document.getElementById('chat-clear').click();
    const finishClearCall = smoke.pendingSmokeCall();
    const oldClear = appBindings.clearChat(() => finishClearCall.call());
    const readyState = appBindings.current;
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    finishClearCall.resolve(0);
    await oldClear;
    if (appBindings.clearDialog.open || !document.getElementById('chat-cleared').hidden) {
      throw new Error('old Clear repainted new identity');
    }
  };
  const assertChatBlockRequestBlockControlMissing = () => {
    if (
      document.getElementById('request-block').hidden ||
      !document.getElementById('chat-block').hidden
    ) {
      throw new Error('request Block control missing');
    }
  };
  const assertChatBlockFailedBlockHidTheChat = () => {
    if (
      appBindings.chatDetail.hidden ||
      !appBindings.chatRequests.querySelector('button') ||
      !document.getElementById('chats-status').textContent.includes('diskFailed')
    ) {
      throw new Error('failed Block hid the chat');
    }
  };
  const assertChatBlockBlockedPeerStayedInChatsOrRevocation = () => {
    if (
      !appBindings.chatDetail.hidden ||
      appBindings.chatRequests.querySelector('button') ||
      document.getElementById('chats-status').textContent !==
        appApi.strings['chats.blocked.pending']
    ) {
      throw new Error('blocked peer stayed in Chats or revocation was overstated');
    }
  };
  const checkChatBlock = async () => {
    await openArea('chats-area');
    await appApi.chatsDone;
    const peer = 'b'.repeat(64);
    const message = { author: peer, id: [4], text: 'request', deleted: false, sentAtMs: null };
    const row = { peerId: peer, state: 'requestReceived', lastMessage: message, unreadCount: 1 };
    const open = (state) =>
      appBindings.openChat(peer, (command) => {
        if (command === 'chat_history') return Promise.resolve({ state, messages: [message] });
        if (command === 'mark_chat_read') return Promise.resolve(0);
        if (command === 'chat_rows') return Promise.resolve([row]);
        throw new Error(`unexpected block setup: ${command}`);
      });
    appBindings.renderChatRows([row]);
    await open('requestReceived');
    assertChatBlockRequestBlockControlMissing();
    await appBindings.blockChat((command) => {
      if (command === 'block_peer') return Promise.reject('diskFailed');
      throw new Error(`unexpected failed block: ${command}`);
    });
    assertChatBlockFailedBlockHidTheChat();
    await open('open');
    if (document.getElementById('chat-block').hidden) throw new Error('open chat Block missing');
    await appBindings.blockChat((command) => {
      if (command === 'block_peer')
        return Promise.resolve({ revocation: 'pending', detail: 'offline' });
      if (command === 'chat_rows') return Promise.resolve([]);
      throw new Error(`unexpected successful block: ${command}`);
    });
    assertChatBlockBlockedPeerStayedInChatsOrRevocation();
    await open('open');
    const finishBlockCall = smoke.pendingSmokeCall();
    const oldBlock = appBindings.blockChat((command) =>
      command === 'block_peer' ? finishBlockCall.call() : Promise.resolve([]),
    );
    const readyState = appBindings.current;
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    finishBlockCall.resolve({ revocation: 'confirmed' });
    await oldBlock;
    if (!appBindings.chatDetail.hidden) throw new Error('old Block repainted new identity');
  };
  smoke.uiSmokeChatList = Object.freeze({
    checkChatProjection,
    checkFirstMessage,
    checkClearChat,
    checkChatBlock,
  });
}
