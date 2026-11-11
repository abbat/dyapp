// Active recently: the mocked smoke and the two-client steps it adds to the live scenario.
const checkChatPresence = async () => {
  await openArea('chats-area');
  await appApi.chatsDone;
  const peer = '6'.repeat(64);
  const mark = appApi.strings['chats.active'];
  const header = document.getElementById('chat-presence');
  let activeRecently = true;
  const sent = [];
  const call = (command) => {
    sent.push(command);
    if (command === 'chat_history') {
      return Promise.resolve({ state: 'open', messages: [], activeRecently });
    }
    if (command === 'chat_rows') {
      return Promise.resolve([
        { peerId: peer, state: 'open', lastMessage: null, unreadCount: 0, activeRecently },
      ]);
    }
    return Promise.resolve(0);
  };
  await appApi.mailUpdated(0, call);
  if (!liveRow('chat-list', peer)?.textContent.includes(mark)) throw new Error('row has no mark');
  await appBindings.openChat(peer, call);
  if (header.hidden || header.textContent !== mark) throw new Error('chat has no presence mark');
  await smoke.shot('chat-presence');
  activeRecently = false;
  await appApi.mailUpdated(0, call);
  if (!header.hidden) throw new Error('an old presence stayed in the chat');
  document.getElementById('chat-back').click();
  await appApi.mailUpdated(0, call);
  if (liveRow('chat-list', peer).textContent.includes(mark)) {
    throw new Error('a pass with no new mail kept an old presence in the row');
  }
  await appApi.mailUpdated(0, () => Promise.resolve([]));

  const shown = appApi.announcePresence;
  const focus = async () => {
    sent.length = 0;
    let pass;
    appApi.announcePresence = () => {
      pass = shown(call);
    };
    window.dispatchEvent(new Event('focus'));
    appApi.announcePresence = shown;
    await pass;
    return sent.includes('announce_presence');
  };
  Object.defineProperty(document, 'hidden', { configurable: true, value: true });
  const hidden = await focus();
  delete document.hidden;
  if (hidden) throw new Error('a hidden window sent presence');
  if (!(await focus())) throw new Error('coming to the foreground sent no presence');
  const failed = shown(() => {
    throw new Error('offline');
  });
  await failed;
};
// A failed send that stayed stored is pending; one that left nothing stored remains a failure.
const checkLikePending = async () => {
  const peerId = '7'.repeat(64);
  const found = { peerId, draft: { attributes: {} } };
  const card = document.createElement('button');
  appBindings.searchResults.append(card);
  for (const [ours, expected] of [
    [true, 'view.like.pending'],
    [false, 'view.like.notConnected'],
  ]) {
    appBindings.openFound(found, card, () => Promise.resolve({ ours: false, matched: false }));
    await appBindings.sendLike(true, (command) =>
      command === 'like_peer'
        ? Promise.reject('notConnected')
        : Promise.resolve({ ours, matched: false }),
    );
    if (appBindings.viewStatus.textContent !== appApi.strings[expected]) {
      throw new Error(`failed like shows ${appBindings.viewStatus.textContent}`);
    }
  }
  appBindings.closeFound();
  card.remove();
};
const liveMatched = async (peer) =>
  (await appBindings.invoke('peer_likes', { peerId: peer })).matched;
// One more than the old mailbox limit of 100, so the last text only arrives past that limit.
const FLOOD_TEXTS = 102;
const FLOOD_STEP_MS = 60000;
const floodTexts = async (peer) =>
  (await liveHistory(peer)).filter((text) => text?.startsWith('flood '));
const presenceSteps = {
  'presence-like': async (peer) => {
    await liveFound(peer);
    await liveLikes(peer, false);
    document.getElementById('view-like').click();
    await appApi.likeDone;
  },
  'presence-pass': async (peer) => {
    await liveChats(() => liveRow('chats-liked-list', peer), 'the like to pass');
    liveRow('chats-liked-list', peer).click();
    document.getElementById('view-pass').click();
    await appApi.passDone;
    if (
      document.getElementById('view-unpass').hidden ||
      !document.getElementById('view-like').hidden
    ) {
      throw new Error('the pass left the profile actions unchanged');
    }
    document.getElementById('view-back').click();
    await liveChats(() => !liveRow('chats-liked-list', peer), 'the pass');
    if (document.getElementById('chats-liked-count').textContent !== '0') {
      throw new Error('Liked you still counts the passed peer');
    }
    await openArea('search-area');
    document.getElementById('run-search').click();
    await appApi.searchDone;
    if (document.querySelector(`#search-results [data-peer-id="${peer}"]`)) {
      throw new Error('search showed the passed peer');
    }
    await openArea('settings', 'passed');
    await appApi.passedShown;
    document.getElementById('unpass-1').click();
    await appApi.passedShown;
    if (document.getElementById('passed-none').hidden) throw new Error('undo kept the pass');
    await liveChats(() => liveRow('chats-liked-list', peer), 'the undone pass');
  },
  'presence-match': async (peer) => {
    await liveChats(() => liveRow('chats-liked-list', peer), 'the second like');
    // Not through search: the other client unhid its profile, and that record waits for the window.
    liveRow('chats-liked-list', peer).click();
    document.getElementById('view-like').click();
    await appApi.likeDone;
    if (!(await liveMatched(peer))) throw new Error('the second like made no match');
  },
  'presence-send': async (peer) => {
    await openArea('chats-area');
    await livePoll(async () => {
      appApi.chatsDone = appApi.showChats();
      await appApi.chatsDone;
      return liveMatched(peer);
    }, 'the match');
    // The launch or focus send may have taken it already; either way the hour is now used.
    await appBindings.invoke('announce_presence');
    if ((await appBindings.invoke('announce_presence')) !== false)
      throw new Error('the throttle let a resend');
  },
  'presence-shown': async (peer) => {
    const mark = appApi.strings['chats.active'];
    await liveChats(() => liveRow('chat-list', peer)?.textContent.includes(mark), 'the presence');
    await liveOpen(liveRow('chat-list', peer));
    if (document.getElementById('chat-presence').hidden)
      throw new Error('chat has no presence mark');
  },
  'match-send': async (peer) => {
    await liveFound(peer);
    document.getElementById('view-message').click();
    await Promise.all([appApi.chatOpened, appApi.chatsDone]);
    liveState('open');
    await liveSend('match text');
  },
  'match-received': async (peer) => {
    await liveChats(
      () => liveRow('chat-list', peer)?.textContent.includes('match text'),
      'the text',
    );
  },
  // Stops inside the check's limit: the next run of this step goes on from the stored history, so
  // the per-send cost decides how many runs it takes, not whether the step fails.
  'flood-send': async (peer) => {
    const stop = Date.now() + FLOOD_STEP_MS;
    let sent = (await floodTexts(peer)).length;
    while (sent < FLOOD_TEXTS && Date.now() < stop) {
      await appBindings.invoke('send_chat_text', { peerId: peer, text: `flood ${sent}` });
      sent += 1;
    }
  },
  'flood-received': async (peer) => {
    const last = `flood ${FLOOD_TEXTS - 1}`;
    await liveChats(() => liveRow('chat-list', peer)?.textContent.includes(last), 'the last text');
    const stored = (await floodTexts(peer)).length;
    if (stored !== FLOOD_TEXTS)
      throw new Error(`${stored} of ${FLOOD_TEXTS} pending texts arrived`);
  },
  'notice-send': (peer) =>
    appBindings.invoke('send_chat_text', { peerId: peer, text: '<b>notice</b> text' }),
  // desktop-smoke.py runs fake-notify.py here and reads the notice it kept. The window comes back
  // only through the click, or ui_test_hide fails.
  'notice-click': async () => {
    await openArea('settings', 'notifications');
    await appApi.showPrivacy();
    const content = document.getElementById('notice-content');
    content.value = 'text';
    content.dispatchEvent(new Event('change', { bubbles: true }));
    await appApi.privacySaved;
    // Notices go only to an unfocused window, so mail is received once the hide has completed.
    await invokeTest('ui_test_hide', { close: false, restore: false });
    await appBindings.invoke('receive_mail');
    await invokeTest('ui_test_restored');
  },
};
smoke.uiSmokePresence = Object.freeze({
  checkChatPresence,
  checkLikePending,
  presenceSteps,
});
