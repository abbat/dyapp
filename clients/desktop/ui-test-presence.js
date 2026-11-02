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
const liveMatched = async (peer) => (await invoke('peer_likes', { peerId: peer })).matched;
const presenceSteps = {
  'presence-like': async (peer) => {
    await liveFound(peer);
    await liveLikes(peer, false);
    document.getElementById('view-like').click();
    await window.likeDone;
  },
  'presence-match': async (peer) => {
    await liveChats(() => liveRow('chats-liked-list', peer), 'the second like');
    await liveFound(peer);
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
};
globalThis.uiSmokePresence = Object.freeze({ checkChatPresence, presenceSteps });
