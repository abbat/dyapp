// Active recently: the mocked smoke and the two-client steps it adds to the live scenario.
const checkChatPresence = async () => {
  await openArea('chats-area');
  await window.chatsDone;
  const peer = '6'.repeat(64);
  const mark = window.strings['chats.active'];
  const header = document.getElementById('chat-presence');
  let activeRecently = true;
  const sent = [];
  const call = (command) => {
    sent.push(command);
    if (command === 'chat_history') {
      return Promise.resolve({ state: 'open', messages: [], activeRecently });
    }
    if (command === 'chat_rows') {
      return Promise.resolve([{ peerId: peer, state: 'open', lastMessage: null, unreadCount: 0,
        activeRecently }]);
    }
    return Promise.resolve(0);
  };
  await window.mailUpdated(0, call);
  if (!liveRow('chat-list', peer)?.textContent.includes(mark)) throw new Error('row has no mark');
  await openChat(peer, call);
  if (header.hidden || header.textContent !== mark) throw new Error('chat has no presence mark');
  await shot('chat-presence');
  activeRecently = false;
  await window.mailUpdated(0, call);
  if (!header.hidden) throw new Error('an old presence stayed in the chat');
  document.getElementById('chat-back').click();
  await window.mailUpdated(0, call);
  if (liveRow('chat-list', peer).textContent.includes(mark)) {
    throw new Error('a pass with no new mail kept an old presence in the row');
  }
  await window.mailUpdated(0, () => Promise.resolve([]));

  const shown = window.announcePresence;
  const focus = async () => {
    sent.length = 0;
    let pass;
    window.announcePresence = () => { pass = shown(call); };
    window.dispatchEvent(new Event('focus'));
    window.announcePresence = shown;
    await pass;
    return sent.includes('announce_presence');
  };
  Object.defineProperty(document, 'hidden', { configurable: true, value: true });
  const hidden = await focus();
  delete document.hidden;
  if (hidden) throw new Error('a hidden window sent presence');
  if (!await focus()) throw new Error('coming to the foreground sent no presence');
  const failed = shown(() => { throw new Error('offline'); });
  await failed;
};
// A failed send that stayed stored is pending; one that left nothing stored remains a failure.
const checkLikePending = async () => {
  const peerId = '7'.repeat(64);
  const found = { peerId, draft: { attributes: {} } };
  const card = document.createElement('button');
  searchResults.append(card);
  for (const [ours, expected] of [[true, 'view.like.pending'], [false, 'view.like.notConnected']]) {
    openFound(found, card, () => Promise.resolve({ ours: false, matched: false }));
    await sendLike(true, (command) => (command === 'like_peer' ? Promise.reject('notConnected')
      : Promise.resolve({ ours, matched: false })));
    if (viewStatus.textContent !== window.strings[expected]) {
      throw new Error(`failed like shows ${viewStatus.textContent}`);
    }
  }
  closeFound();
  card.remove();
};
const liveMatched = async (peer) => (await invoke('peer_likes', { peerId: peer })).matched;
const presenceSteps = {
  'presence-like': async (peer) => {
    await liveFound(peer);
    await liveLikes(peer, false);
    document.getElementById('view-like').click();
    await window.likeDone;
  },
  'presence-pass': async (peer) => {
    await liveChats(() => liveRow('chats-liked-list', peer), 'the like to pass');
    liveRow('chats-liked-list', peer).click();
    document.getElementById('view-pass').click();
    await window.passDone;
    if (document.getElementById('view-unpass').hidden || !document.getElementById('view-like').hidden) {
      throw new Error('the pass left the profile actions unchanged');
    }
    document.getElementById('view-back').click();
    await liveChats(() => !liveRow('chats-liked-list', peer), 'the pass');
    if (document.getElementById('chats-liked-count').textContent !== '0') {
      throw new Error('Liked you still counts the passed peer');
    }
    await openArea('search-area');
    document.getElementById('run-search').click();
    await window.searchDone;
    if (document.querySelector(`#search-results [data-peer-id="${peer}"]`)) {
      throw new Error('search showed the passed peer');
    }
    await openArea('settings');
    await window.passedShown;
    document.getElementById('unpass-1').click();
    await window.passedShown;
    if (document.getElementById('passed-none').hidden) throw new Error('undo kept the pass');
    await liveChats(() => liveRow('chats-liked-list', peer), 'the undone pass');
  },
  'presence-match': async (peer) => {
    await liveChats(() => liveRow('chats-liked-list', peer), 'the second like');
    // Not through search: the other client unhid its profile, and that record waits for the window.
    liveRow('chats-liked-list', peer).click();
    document.getElementById('view-like').click();
    await window.likeDone;
    if (!await liveMatched(peer)) throw new Error('the second like made no match');
  },
  'presence-send': async (peer) => {
    await openArea('chats-area');
    await livePoll(async () => {
      window.chatsDone = window.showChats();
      await window.chatsDone;
      return liveMatched(peer);
    }, 'the match');
    // The launch or focus send may have taken it already; either way the hour is now used.
    await invoke('announce_presence');
    if (await invoke('announce_presence') !== false) throw new Error('the throttle let a resend');
  },
  'presence-shown': async (peer) => {
    const mark = window.strings['chats.active'];
    await liveChats(() => liveRow('chat-list', peer)?.textContent.includes(mark), 'the presence');
    await liveOpen(liveRow('chat-list', peer));
    if (document.getElementById('chat-presence').hidden) throw new Error('chat has no presence mark');
  },
  'match-send': async (peer) => {
    await liveFound(peer);
    document.getElementById('view-message').click();
    await Promise.all([window.chatOpened, window.chatsDone]);
    liveState('open');
    await liveSend('match text');
  },
  'match-received': async (peer) => {
    await liveChats(() => liveRow('chat-list', peer)?.textContent.includes('match text'), 'the text');
  },
};
globalThis.uiSmokePresence = Object.freeze({
  checkChatPresence, checkLikePending, presenceSteps,
});
