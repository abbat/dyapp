// Contact names in chats: the rule for each kind of name, the rename dialog and identity isolation.
const checkContactNames = async () => {
  await openArea('chats-area');
  await window.chatsDone;
  const named = 'a'.repeat(64);
  const claimed = 'b'.repeat(64);
  const plain = 'c'.repeat(64);
  const row = (peerId) => ({ peerId, state: 'open', lastMessage: null, unreadCount: 0,
    activeRecently: false });
  const names = new Map([
    [named, { peerId: named, name: 'Aunt Vera', verified: null }],
    [claimed, { peerId: claimed, name: 'Bank Support', verified: false }],
    [plain, { peerId: plain, name: '', verified: null }],
  ]);
  let refuse = null;
  const call = (command, args) => {
    if (command === 'chat_rows') return Promise.resolve([named, claimed, plain].map(row));
    if (command === 'chat_history') {
      return Promise.resolve({ state: 'open', messages: [], activeRecently: false });
    }
    if (command === 'contact_names') {
      return Promise.resolve(args.peerIds.map((peerId) => names.get(peerId)));
    }
    if (command === 'set_contact_name') {
      if (refuse) return Promise.reject(refuse);
      names.set(args.peerId, { peerId: args.peerId, name: args.name.trim(), verified: null });
    }
    return Promise.resolve(0);
  };
  await window.showChats(call);
  const label = (peerId) => liveRow('chat-list', peerId).querySelector('b').textContent;
  if (label(named) !== 'Aunt Vera') throw new Error('a local name was not shown');
  const public_ = label(claimed);
  const mark = window.strings['peer.unverified'];
  if (!public_.includes('Bank Support') || !public_.includes(mark)
    || !public_.includes(shortPeerId(claimed, knownPeers))) {
    throw new Error(`a public name lost its mark or Peer ID: ${public_}`);
  }
  if (label(plain) !== shortPeerId(plain, knownPeers)) throw new Error('no name lost the Peer ID');

  await openChat(plain, call);
  const dialog = document.getElementById('chat-rename-dialog');
  const input = document.getElementById('chat-rename-input');
  const error = document.getElementById('chat-rename-error');
  const { saveRename } = globalThis.uiSmokeRename;
  document.getElementById('chat-rename').click();
  if (!dialog.open || input.value !== '') throw new Error('rename did not open empty');
  await shot('chat-rename-dialog');
  input.value = 'x';
  refuse = 'invalidName';
  await saveRename(call);
  if (!dialog.open || error.textContent !== window.strings['chats.renameInvalid']) {
    throw new Error('a refused name did not explain itself');
  }
  refuse = null;
  input.value = '  Cleo  ';
  await saveRename(call);
  if (dialog.open) throw new Error('a saved name left the dialog open');
  if (document.getElementById('chat-peer').textContent !== 'Cleo') {
    throw new Error('the header did not follow the rename');
  }
  if (label(plain) !== 'Cleo') throw new Error('the row did not follow the rename');
  document.getElementById('chat-rename').click();
  if (input.value !== 'Cleo') throw new Error('rename did not start from the local name');
  document.getElementById('chat-rename-cancel').click();
  if (dialog.open) throw new Error('Cancel left the dialog open');

  document.getElementById('chat-back').click();
  document.dispatchEvent(new Event('dyapp:identity-change'));
  if (window.peerName(plain) !== shortPeerId(plain, knownPeers)) {
    throw new Error('a name crossed to another identity');
  }
  await window.showChats(() => Promise.resolve([]));
};

// The public name: the editor, the publish list and how a second client's card and view show it.
const checkPublicName = async () => {
  await openArea('profile-area');
  const input = document.getElementById('profile-name');
  const before = input.value;
  input.value = 'Anna';
  if (profileDraft().name !== 'Anna') throw new Error('the draft lost the public name');
  input.value = before;
  await shot('profile-public-name');
  const fields = publicFields({ name: 'Anna', birthYear: null, birthMonth: null, birthDay: null });
  if (fields[0] !== `${window.strings['profile.name']}: Anna`) {
    throw new Error('the publish list left out the public name');
  }
  let refused = '';
  await saveProfile(() => Promise.reject('invalidName')).catch((error) => { refused = error; });
  if (refused !== window.strings['profile.nameInvalid']) {
    throw new Error(`a refused public name was not explained: ${refused}`);
  }
  await saveProfile(() => Promise.reject('invalidHeight')).catch((error) => { refused = error; });
  if (refused !== window.strings['profile.heightInvalid']) {
    throw new Error(`a refused height was not explained: ${refused}`);
  }
  await saveProfile(() => Promise.reject('invalidAbout')).catch((error) => { refused = error; });
  if (refused !== window.strings['profile.aboutInvalid']) {
    throw new Error(`a refused about was not explained: ${refused}`);
  }

  await openArea('search-area');
  const peer = 'd'.repeat(64);
  let verified = false;
  const found = { peerId: peer, version: 1, place: '',
    draft: { name: 'Anna', birthYear: null, birthMonth: null, birthDay: null, country: '' } };
  const call = (command, args) => {
    if (command === 'search_profiles') {
      return Promise.resolve({ profiles: [found], offered: [], partial: false, failures: [] });
    }
    if (command === 'contact_names') {
      return Promise.resolve(args.peerIds.map((peerId) => ({ peerId, name: 'Anna', verified })));
    }
    if (command === 'confirm_contact') { verified = true; return Promise.resolve(found); }
    if (command === 'unverify_contact') { verified = false; return Promise.resolve(); }
    return Promise.resolve(0);
  };
  await runSearch(call);
  const card = liveRow('search-results', peer);
  const marked = (mark) => card.textContent.includes(`Anna · ${window.strings[mark]}`);
  if (!marked('peer.unverified')) throw new Error(`no unverified card: ${card.textContent}`);
  if (card.querySelector('.avatar').textContent !== shortPeerId(peer, knownPeers).slice(1, 3)) {
    throw new Error('the avatar left the short Peer ID');
  }
  await shot('search-public-name');
  openFound(found, card, call);
  const title = document.getElementById('view-title');
  const peerShort = shortPeerId(peer, knownPeers);
  if (!title.textContent.startsWith('Anna') || !title.textContent.endsWith(peerShort)
    || document.getElementById('view-peer').textContent !== peerShort) {
    throw new Error(`the view lost the name or the Peer ID: ${title.textContent}`);
  }
  await confirmContact(call);
  if (!marked('peer.verified')) throw new Error('the card did not follow the mark');
  await removeMark(call);
  if (!marked('peer.unverified')) throw new Error('the card did not follow the removed mark');
  document.getElementById('view-back').click();
  clearSearch();
  document.dispatchEvent(new Event('dyapp:identity-change'));
};
// Mute: the action and the row follow the stored flag, and a refusal leaves both alone.
const checkChatMute = async () => {
  await openArea('chats-area');
  await window.chatsDone;
  const peer = 'd'.repeat(64);
  let muted = false;
  let refuse = null;
  const call = (command, args) => {
    if (command === 'chat_rows') {
      return Promise.resolve([{ peerId: peer, state: 'open', lastMessage: null, unreadCount: 1,
        activeRecently: false, muted }]);
    }
    if (command === 'chat_history') {
      return Promise.resolve({ state: 'open', messages: [], activeRecently: false, muted });
    }
    if (command === 'contact_names') {
      return Promise.resolve([{ peerId: peer, name: '', verified: null }]);
    }
    if (command === 'set_muted') {
      if (refuse) return Promise.reject(refuse);
      ({ muted } = args);
    }
    return Promise.resolve(0);
  };
  await window.showChats(call);
  await openChat(peer, call);
  const button = document.getElementById('chat-mute');
  const shows = (key, mark) => button.textContent === window.strings[key]
    && liveRow('chat-list', peer).textContent.includes(window.strings['chats.muted']) === mark;
  if (!shows('chats.mute', false)) throw new Error('an unmuted chat looks muted');
  await muteChat(call);
  if (!muted || !shows('chats.unmute', true)) throw new Error('Mute did not show');
  refuse = 'disk full';
  await muteChat(call);
  if (!muted || !shows('chats.unmute', true) || !chatComposeStatus.textContent.includes(refuse)) {
    throw new Error('a refused unmute did not explain itself');
  }
  refuse = null;
  await muteChat(call);
  if (muted || !shows('chats.mute', false)) throw new Error('Unmute did not show');
  document.getElementById('chat-back').click();
  document.dispatchEvent(new Event('dyapp:identity-change'));
};
// A shared profile link renders as a card that opens the page; Share sends one to an open chat.
const checkProfileShare = async () => {
  await openArea('chats-area');
  await window.chatsDone;
  const chatPeer = '7'.repeat(64);
  const sharedPeer = '8'.repeat(64);
  const requestPeer = '9'.repeat(64);
  const link = { author: chatPeer, groupId: [4], id: [5], text: `dyapp-profile:1:${sharedPeer}`,
    sharedPeer, forwarded: false, deleted: false, edited: false, sentAtMs: null };
  const rows = [
    { peerId: chatPeer, state: 'open', lastMessage: link, unreadCount: 0 },
    { peerId: sharedPeer, state: 'open', lastMessage: null, unreadCount: 0 },
    { peerId: requestPeer, state: 'requestReceived', lastMessage: null, unreadCount: 0 },
  ];
  const found = { peerId: sharedPeer, draft: { birthYear: null, birthMonth: null, birthDay: null } };
  let shareError = null;
  let fetchError = null;
  let sharedArgs;
  const call = (command, args) => {
    if (command === 'chat_history') return Promise.resolve({ state: 'open', messages: [link] });
    if (command === 'mark_chat_read') return Promise.resolve(0);
    if (command === 'chat_rows') return Promise.resolve(rows);
    if (command === 'shared_profile') {
      if (args.peerId !== sharedPeer) throw new Error('card fetched another peer');
      return fetchError ? Promise.reject(fetchError) : Promise.resolve(found);
    }
    if (command === 'contact_verified') return Promise.resolve(false);
    if (command === 'peer_likes') return Promise.resolve({ ours: false, matched: false });
    if (command === 'share_profile') {
      sharedArgs = args;
      return shareError ? Promise.reject(shareError) : Promise.resolve([6]);
    }
    return Promise.reject('notConnected');
  };
  renderChatRows(rows);
  const text = `${window.strings['chats.profileCard']} ${window.shortPeer(sharedPeer)}`;
  if (!document.querySelector(`#chat-list [data-peer-id="${chatPeer}"]`)?.textContent.includes(text)) {
    throw new Error('chat preview did not name the shared profile');
  }
  await openChat(chatPeer, call);
  const card = chatMessages.querySelector(`button[data-peer-id="${sharedPeer}"]`);
  if (!card || card.textContent !== text || chatMessages.textContent.includes('dyapp-profile:')) {
    throw new Error('profile link did not render as a card');
  }
  fetchError = 'unpublished';
  card.click();
  await window.sharedOpened;
  if (!chatComposeStatus.textContent.includes(window.strings['chats.profileFailed'])
    || !chatComposeStatus.textContent.includes('unpublished')) {
    throw new Error('failed profile card was silent');
  }
  fetchError = 'blocked';
  card.click();
  await window.sharedOpened;
  if (!profileView.hidden || !chatComposeStatus.textContent.includes('blocked')) {
    throw new Error('a blocked shared profile opened or failed silently');
  }
  fetchError = null;
  card.click();
  await window.sharedOpened;
  if (profileView.hidden || viewed?.found.peerId !== sharedPeer) {
    throw new Error('profile card did not open the page');
  }
  if (!document.getElementById('view-share')) throw new Error('profile page has no Share');
  await openShare(sharedPeer, call);
  if (!forwardDialog.open || forwardSelect.options.length !== 1 || forwardSelect.value !== chatPeer
    || document.getElementById('chat-forward-title').textContent !== window.strings['chats.shareTitle']
    || forwardConfirm.textContent !== window.strings['chats.share']) {
    throw new Error('Share picker included a request or the shared peer');
  }
  shareError = 'nodeFailed';
  await shareProfile(call);
  if (!forwardDialog.open || forwardConfirm.disabled
    || !forwardError.textContent.includes(window.strings['chats.shareFailed'])) {
    throw new Error('failed Share closed the picker');
  }
  shareError = null;
  await shareProfile(call);
  if (sharedArgs?.peerId !== sharedPeer || sharedArgs.targetPeerId !== chatPeer || forwardDialog.open
    || activeChat?.peerId !== chatPeer || chatComposeStatus.textContent !== window.strings['chats.shareSent']) {
    throw new Error('Share did not send the profile to the chosen chat');
  }
  closeFound();
  document.getElementById('chat-back').click();
};
const checkInterests = async () => {
  await openArea('profile-area');
  const input = document.getElementById('profile-interests');
  const before = input.value;
  input.value = '  Музыка  \nJazz, blues\n\n';
  const tags = profileDraft().interests;
  if (tags.join('|') !== 'Музыка|Jazz, blues') throw new Error('interest lines lost tags or punctuation');
  input.value = Array(33).fill('music').join('\n');
  input.dispatchEvent(new Event('input'));
  if (input.checkValidity()) throw new Error('too many interests passed the form');
  window.showProfileInterests(['Музыка', 'Jazz, blues']);
  if (!input.checkValidity()) throw new Error('loaded interests retained an old error');
  const canonical = ['музыка', 'jazz, blues'];
  await saveProfile(() => Promise.resolve({ draft: { ...profileDraft(), interests: canonical }, ages: null }));
  if (input.value !== canonical.join('\n')) throw new Error('save did not show canonical interests');
  const fields = publicFields({ ...profileDraft(), interests: canonical });
  if (!fields.includes(`${window.strings['profile.interests']}: ${canonical.join('; ')}`)) {
    throw new Error('public preview lost canonical interests');
  }
  await shot('profile-interests');
  let refusal;
  await saveProfile(() => Promise.reject('invalidInterests')).catch((error) => { refusal = error; });
  if (refusal !== window.strings['profile.interestsInvalid']) throw new Error('interest error not explained');
  window.showProfileInterests([]);
  if (profileDraft().interests.length) throw new Error('clearing interests retained tags');
  input.value = before;
  await openArea('search-area');
  const search = document.getElementById('search-interest');
  showSearchFilters([19]);
  search.value = ' Музыка ';
  const filter = searchQuery().conditions.find((condition) => condition.field === 19);
  if (filter?.op !== 'eq' || filter.strings[0] !== 'Музыка') throw new Error('interest filter was not sent');
  await shot('search-interest');
  showSearchFilters([]);
  if (searchQuery().conditions.some((condition) => condition.field === 19)) {
    throw new Error('unsupported interest filter leaked into query');
  }
  search.value = '';
};
const checkFavoriteAction = async () => {
  await openArea('search-area');
  const card = document.createElement('button');
  document.getElementById('search-results').append(card);
  const found = { peerId: '6'.repeat(64), draft: { birthYear: null, birthMonth: null, birthDay: null } };
  let saved = false;
  let readError = true;
  let changeError = null;
  let hold = false;
  let finish;
  const writes = [];
  const call = (command, args) => {
    if (command === 'favorite_state') return readError ? Promise.reject('disk') : Promise.resolve(saved);
    if (command === 'set_favorite') {
      writes.push(args);
      if (changeError) return Promise.reject(changeError);
      if (hold) return new Promise((resolve) => { finish = () => { saved = args.favorite; resolve(); }; });
      saved = args.favorite;
      return Promise.resolve();
    }
    if (command === 'like_peer') throw new Error('favorite sent a like');
    if (command === 'peer_likes') return Promise.resolve({ ours: false, matched: false });
    return Promise.resolve(false);
  };
  openFound(found, card, call);
  await window.favoriteShown;
  const button = document.getElementById('view-favorite-toggle');
  const status = document.getElementById('view-favorite-status');
  if (button.textContent !== window.strings['favorites.retry']) throw new Error('unknown saved state cannot retry');
  await window.toggleFavorite();
  if (writes.length) throw new Error('retrying unknown state mutated a favorite');
  readError = false;
  await window.toggleFavorite();
  changeError = 'disk';
  await window.toggleFavorite();
  if (saved || button.disabled || status.textContent !== window.strings['favorites.changeFailed']) {
    throw new Error('failed favorite change lost retry or claimed success');
  }
  changeError = null;
  hold = true;
  const pending = window.toggleFavorite();
  await window.toggleFavorite();
  if (writes.length !== 2 || !button.disabled) throw new Error('favorite allowed duplicate writes');
  finish();
  await pending;
  if (!saved || button.textContent !== window.strings['favorites.remove']) throw new Error('saved favorite not shown');
  await shot('profile-favorite-saved');
  hold = false;
  await window.toggleFavorite();
  if (saved || button.textContent !== window.strings['favorites.add']) throw new Error('favorite removal failed');

  hold = true;
  const closedWrite = window.toggleFavorite();
  closeFound();
  finish();
  await closedWrite;
  if (!document.getElementById('view-favorite').hidden) throw new Error('late write reopened favorite controls');
  saved = false;
  hold = false;
  let release;
  openFound(found, card, (command, args) => command === 'favorite_state'
    ? new Promise((resolve) => { release = resolve; }) : call(command, args));
  const stale = window.favoriteShown;
  const next = { ...found, peerId: '5'.repeat(64) };
  openFound(next, card, call);
  await window.favoriteShown;
  release(true);
  await stale;
  if (button.textContent !== window.strings['favorites.add']) throw new Error('stale read changed another profile');
  openFound(found, card, (command, args) => command === 'favorite_state'
    ? new Promise((resolve) => { release = resolve; }) : call(command, args));
  const switched = window.favoriteShown;
  document.dispatchEvent(new Event('dyapp:identity-change'));
  release(true);
  await switched;
  if (!document.getElementById('view-favorite').hidden) throw new Error('favorite state crossed identity change');
  closeFound();
  card.remove();
};
globalThis.uiSmokeNames = Object.freeze({
  checkContactNames, checkPublicName, checkChatMute, checkProfileShare, checkInterests, checkFavoriteAction,
});
