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
globalThis.uiSmokeNames = Object.freeze({ checkContactNames, checkPublicName, checkChatMute });
