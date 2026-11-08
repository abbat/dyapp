// Contact-name smoke phases, preserving labels, rename errors and identity isolation.
(() => {
  const createFixture = () => {
    const named = 'a'.repeat(64);
    const claimed = 'b'.repeat(64);
    const plain = 'c'.repeat(64);
    const row = (peerId) => ({
      peerId,
      state: 'open',
      lastMessage: null,
      unreadCount: 0,
      activeRecently: false,
    });
    const names = new Map([
      [named, { peerId: named, name: 'Aunt Vera', verified: null }],
      [claimed, { peerId: claimed, name: 'Bank Support', verified: false }],
      [plain, { peerId: plain, name: '', verified: null }],
    ]);
    const fixture = { refuse: null };
    const call = (command, args) => {
      if (command === 'chat_rows') return Promise.resolve([named, claimed, plain].map(row));
      if (command === 'chat_history') {
        return Promise.resolve({ state: 'open', messages: [], activeRecently: false });
      }
      if (command === 'contact_names') {
        return Promise.resolve(args.peerIds.map((peerId) => names.get(peerId)));
      }
      if (command === 'set_contact_name') {
        if (fixture.refuse) return Promise.reject(fixture.refuse);
        names.set(args.peerId, { peerId: args.peerId, name: args.name.trim(), verified: null });
      }
      return Promise.resolve(0);
    };
    const label = (peerId) => liveRow('chat-list', peerId).querySelector('b').textContent;
    return Object.assign(fixture, { named, claimed, plain, call, label });
  };
  const checkLabels = async (fixture) => {
    const { named, claimed, plain, call, label } = fixture;
    await window.showChats(call);
    if (label(named) !== 'Aunt Vera') throw new Error('a local name was not shown');
    const public_ = label(claimed);
    const mark = window.strings['peer.unverified'];
    if (
      !public_.includes('Bank Support') ||
      !public_.includes(mark) ||
      !public_.includes(shortPeerId(claimed, knownPeers))
    ) {
      throw new Error(`a public name lost its mark or Peer ID: ${public_}`);
    }
    if (label(plain) !== shortPeerId(plain, knownPeers))
      throw new Error('no name lost the Peer ID');
  };
  const checkRename = async (fixture) => {
    const { plain, call, label } = fixture;
    await openChat(plain, call);
    const dialog = document.getElementById('chat-rename-dialog');
    const input = document.getElementById('chat-rename-input');
    const error = document.getElementById('chat-rename-error');
    const { saveRename } = globalThis.uiSmokeRename;
    document.getElementById('chat-rename').click();
    if (!dialog.open || input.value !== '') throw new Error('rename did not open empty');
    await shot('chat-rename-dialog');
    input.value = 'x';
    fixture.refuse = 'invalidName';
    await saveRename(call);
    if (!dialog.open || error.textContent !== window.strings['chats.renameInvalid']) {
      throw new Error('a refused name did not explain itself');
    }
    fixture.refuse = null;
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
  };
  const checkIdentityReset = async (fixture) => {
    const { plain } = fixture;
    document.getElementById('chat-back').click();
    document.dispatchEvent(new Event('dyapp:identity-change'));
    if (window.peerName(plain) !== shortPeerId(plain, knownPeers)) {
      throw new Error('a name crossed to another identity');
    }
    await window.showChats(() => Promise.resolve([]));
  };
  window.uiSmokeContactNames = async () => {
    await openArea('chats-area');
    await window.chatsDone;
    const fixture = createFixture();
    await checkLabels(fixture);
    await checkRename(fixture);
    await checkIdentityReset(fixture);
  };
})();
