const { checkChatProjection, checkFirstMessage, checkClearChat, checkChatBlock } =
  smoke.uiSmokeChatList;
const {
  checkChatEdit,
  checkChatDelete,
  checkNewDevice,
  checkChatDelivery,
  checkChatForwarded,
  checkChatForwardAction,
  checkChatLinkHosts,
  checkChatRead,
} = smoke.uiSmokeChatMessages;
// Mail from the other client arrives on a later Chats refresh, so live checks poll for it.
const livePoll = async (check, what) => {
  for (let attempt = 0; attempt < 30; attempt += 1) {
    if (await check()) return;
    await new Promise((resolve) => {
      setTimeout(resolve, 500);
    });
  }
  throw new Error(`timed out waiting for ${what}`);
};
const liveChats = async (ready, what) => {
  await openArea('chats-area');
  await livePoll(async () => {
    await appApi.chatsDone;
    if (ready()) return true;
    appApi.chatsDone = appApi.showChats();
    return false;
  }, what);
};
const liveFound = async (peer) => {
  await openArea('search-area');
  document.getElementById('run-search').click();
  await appApi.searchDone;
  const card = document.querySelector(`#search-results [data-peer-id="${peer}"]`);
  if (!card) throw new Error('search did not find the other client');
  card.click();
};
const liveRow = (list, peer) => document.querySelector(`#${list} [data-peer-id="${peer}"]`);
const liveOpen = async (row) => {
  row.click();
  await appApi.chatOpened;
  if (appBindings.chatDetail.hidden) throw new Error('chat did not open');
};
const liveSend = async (text, results = ['chats.sent']) => {
  appBindings.chatInput.value = text;
  appBindings.chatInput.dispatchEvent(new Event('input'));
  document.getElementById('chat-composer').requestSubmit();
  await appApi.chatSent;
  const status = appBindings.chatComposeStatus.textContent;
  if (!results.some((key) => appApi.strings[key] === status)) {
    throw new Error(`sending "${text}" shows "${status}"`);
  }
};
// Clicks Edit or Delete on the own message with this text.
const liveAction = (text, action) => {
  const message = [...appBindings.chatMessages.querySelectorAll('.msg.out')].find((item) =>
    item.textContent.includes(text),
  );
  [...message.querySelectorAll('button')]
    .find((button) => button.textContent === appApi.strings[action])
    .click();
};
const liveRefused = async (command, args, code) => {
  const error = await appBindings.invoke(command, args).then(() => null, String);
  if (!error || (code && error !== code)) throw new Error(`${command} answered ${error}`);
};
const liveHistory = async (peer) => {
  const history = await appBindings.invoke('chat_history', { peerId: peer });
  return history.messages.map((message) => message.text);
};
const liveLikes = async (peer, ours) => {
  if ((await appBindings.invoke('peer_likes', { peerId: peer })).ours !== ours) {
    throw new Error(`own like is not ${ours}`);
  }
};
const liveUnlikeShown = () =>
  livePoll(() => !document.getElementById('view-unlike').hidden, 'the stored like');
const liveState = (state) => {
  if (appBindings.activeChat?.state !== state)
    throw new Error(`chat is ${appBindings.activeChat?.state}, not ${state}`);
};
// One step of the two-client scenario; each runs in a fresh process against the same node.
const peerSteps = {
  like: async (peer) => {
    await liveFound(peer);
    await liveLikes(peer, false);
    document.getElementById('view-like').click();
    await appApi.likeDone;
    const status = document.getElementById('view-status').textContent;
    if (
      document.getElementById('view-unlike').hidden ||
      !document.getElementById('view-like').hidden ||
      !document.getElementById('view-match').hidden ||
      status
    ) {
      throw new Error(`Like left the wrong profile actions: ${status}`);
    }
  },
  'like-restart': async (peer) => {
    await liveFound(peer);
    await liveUnlikeShown();
    await openArea('chats-area');
    await appApi.chatsDone;
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
    await appApi.likeDone;
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
    await Promise.all([appApi.chatOpened, appApi.chatsDone]);
    liveState('none');
    if (!document.getElementById('chat-clear').hidden)
      throw new Error('Clear shown without a chat');
    await liveSend('first text');
    liveState('requestSent');
    if (
      !appBindings.chatInput.disabled ||
      document.getElementById('chat-first-notice').hidden ||
      !liveRow('chat-list', peer)
    ) {
      throw new Error('the first text left no pending request');
    }
    await liveRefused('send_chat_text', { peerId: peer, text: 'second text' });
    if ((await liveHistory(peer)).join() !== 'first text')
      throw new Error('second text was stored');
  },
  'chat-reply': async (peer) => {
    await liveChats(() => liveRow('chats-requests', peer), 'the request');
    if (!liveRow('chats-requests', peer).querySelector('.meta .badge')) {
      throw new Error('request row has no unread badge');
    }
    await liveOpen(liveRow('chats-requests', peer));
    liveState('requestReceived');
    if (
      !appBindings.chatMessages.textContent.includes('first text') ||
      document.getElementById('chat-request-notice').hidden ||
      document.getElementById('request-block').hidden ||
      !document.getElementById('chat-block').hidden
    ) {
      throw new Error('request view is incomplete');
    }
    await liveSend('reply text');
    liveState('open');
    if (liveRow('chats-requests', peer) || !liveRow('chat-list', peer)) {
      throw new Error('the reply did not move the row');
    }
  },
  'chat-open': async (peer) => {
    await liveChats(
      () => liveRow('chat-list', peer)?.textContent.includes('reply text'),
      'the reply',
    );
    const privacy = await appBindings.invoke('privacy_settings');
    await appBindings.invoke('set_privacy', { privacy: { ...privacy, sendReadReceipts: true } });
    await liveOpen(liveRow('chat-list', peer));
    await appApi.chatRead;
    if ((await appBindings.invoke('send_read_receipts', { peerId: peer })) !== 0) {
      throw new Error('the read receipt is still due');
    }
    liveState('open');
    if (appBindings.chatInput.disabled) throw new Error('the reply left the composer disabled');
    await liveSend('later text');
    liveAction('later text', 'chats.edit');
    appBindings.editInput.value = 'edited text';
    appBindings.editInput.dispatchEvent(new Event('input'));
    document.getElementById('chat-edit-form').requestSubmit();
    await appApi.chatEdited;
    if (appBindings.chatComposeStatus.textContent !== appApi.strings['chats.editSent']) {
      throw new Error(`edit shows ${appBindings.chatComposeStatus.textContent}`);
    }
    await liveSend('gone text');
    liveAction('gone text', 'chats.delete');
    document.getElementById('chat-delete-confirm').click();
    await appApi.chatDeleted;
    if (appBindings.chatComposeStatus.textContent !== appApi.strings['chats.deleteSent']) {
      throw new Error(`delete shows ${appBindings.chatComposeStatus.textContent}`);
    }
  },
  // Nothing here opens Chats or asks for a pass, so the mail can only come from the runtime job.
  'catch-up': async (peer) => {
    await new Promise((resolve) => {
      const shown = appApi.mailUpdated;
      appApi.mailUpdated = (count) => {
        shown(count);
        resolve();
      };
    });
    if (!(await liveHistory(peer)).includes('edited text')) {
      throw new Error('the background pass stored no mail');
    }
  },
  'chat-clear': async (peer) => {
    const deleted = appApi.strings['chats.deleted'];
    await liveChats(() => liveRow('chat-list', peer)?.textContent.includes(deleted), 'the delete');
    await liveOpen(liveRow('chat-list', peer));
    const shownText = appBindings.chatMessages.textContent;
    if (
      !shownText.includes(`edited text (${appApi.strings['chats.edited']})`) ||
      !shownText.includes(deleted) ||
      shownText.includes('later text') ||
      shownText.includes('gone text')
    ) {
      throw new Error(`edit or delete did not arrive: ${shownText}`);
    }
    const replied = [...appBindings.chatMessages.querySelectorAll('.msg')].find((item) =>
      item.textContent.includes('reply text'),
    );
    if (!replied?.textContent.includes(appApi.strings['chats.delivery.read'])) {
      throw new Error('the read receipt left no mark on the reply');
    }
    document.getElementById('chat-clear').click();
    if (!appBindings.clearDialog.open) throw new Error('Clear asked for no confirmation');
    document.getElementById('chat-clear-confirm').click();
    await appApi.chatCleared;
    liveState('open');
    if (
      appBindings.chatMessages.querySelector('.msg') ||
      document.getElementById('chat-cleared').hidden ||
      appBindings.chatInput.disabled
    ) {
      throw new Error('Clear did not empty a writable chat');
    }
    await liveSend('after clear');
  },
  'chat-block': async (peer) => {
    // Later steps sent newer texts, so the row no longer shows this one.
    await liveChats(() => liveRow('chat-list', peer), 'the chat');
    await liveOpen(liveRow('chat-list', peer));
    const { messages } = await appBindings.invoke('chat_history', { peerId: peer });
    // The peer opened these with the default setting, and this side's setting survived a restart.
    if (
      !messages.some((message) => message.text === 'after clear') ||
      messages.some((message) => message.read) ||
      !(await appBindings.invoke('privacy_settings')).sendReadReceipts
    ) {
      throw new Error('the text after Clear, default-off read receipts or the setting went wrong');
    }
    document.getElementById('chat-block').click();
    await appApi.chatBlocked;
    const status = document.getElementById('chats-status').textContent;
    if (
      !appBindings.chatDetail.hidden ||
      liveRow('chat-list', peer) ||
      !['confirmed', 'notNeeded'].some((key) => appApi.strings[`chats.blocked.${key}`] === status)
    ) {
      throw new Error(`Block left the chat: ${status}`);
    }
    await liveRefused('send_chat_text', { peerId: peer, text: 'blocked text' }, 'blocked');
    await liveRefused('chat_history', { peerId: peer }, 'blocked');
  },
  'chat-after-block': async (peer) => {
    await liveChats(() => liveRow('chat-list', peer), 'the chat');
    await liveOpen(liveRow('chat-list', peer));
    const kept = await liveHistory(peer);
    if (!kept.includes('after clear') || kept.includes('reply text')) {
      throw new Error('Clear did not survive');
    }
    await liveSend('after block', ['chats.sent', 'chats.queued']);
  },
  'chat-blocked': async (peer) => {
    await openArea('chats-area');
    await appApi.chatsDone;
    if (liveRow('chat-list', peer) || liveRow('chats-requests', peer)) {
      throw new Error('blocked peer is back in Chats');
    }
    await appBindings.invoke('unblock_peer', { peerId: peer });
    if ((await liveHistory(peer)).includes('after block'))
      throw new Error('blocked text was stored');
  },
};
const checkPeerStep = (kind, peer) =>
  (
    peerSteps[kind] ||
    smoke.uiSmokePresence.presenceSteps[kind] ||
    smoke.uiSmokePhotos.photoSteps[kind] ||
    smoke.stickerPeerChecks.steps[kind]
  )(peer);
smoke.uiSmokeChats = Object.freeze({
  checkChatProjection,
  checkFirstMessage,
  checkClearChat,
  checkChatBlock,
  checkChatEdit,
  checkChatDelete,
  checkChatDelivery,
  checkChatForwarded,
  checkChatForwardAction,
  checkChatLinkHosts,
  checkNewDevice,
  checkChatRead,
  checkPeerStep,
});
