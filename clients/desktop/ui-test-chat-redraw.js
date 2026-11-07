// An unchanged mailbox pass must not discard a slow image or the message objects in action guards.
window.checkChatRedraw = async () => {
  const check = (ok, why) => { if (!ok) throw new Error(why); };
  const deferred = () => { let resolve; const promise = new Promise((done) => { resolve = done; }); return { promise, resolve }; };
  const peer = `12D3KooW${'a'.repeat(44)}`;
  const bytes = new Uint8Array(12); const header = new DataView(bytes.buffer);
  header.setUint32(0, 1, true); header.setUint32(4, 1, true); bytes.set([255, 50, 0, 255], 8);
  let version = 0, reads = 0, started = deferred(), reply = deferred();
  const own = { groupId: [9], author: current.peerId, id: Array(16).fill(2), sentAtMs: 1, text: 'keep actions' };
  const sticker = { groupId: [9], author: peer, id: Array(16).fill(1), sentAtMs: 1,
    sticker: { emoji: '😺', packName: 'Cats' } };
  const call = async (command) => {
    if (command === 'chat_history') return JSON.parse(JSON.stringify({
      state: 'open', muted: false, activeRecently: false,
      messages: [sticker, own, ...(version ? [{ ...own, id: Array(16).fill(3), text: String(version) }] : [])],
    }));
    if (command === 'chat_sticker_frame') { reads += 1; started.resolve(); return reply.promise; }
    if (command === 'chat_sticker_pack_status') return { state: 'new', generation: null, failure: null };
    if (command === 'chat_rows' || command === 'contact_names') return [];
    return false;
  };
  const runtimeMail = window.mailUpdated;
  window.mailUpdated = async () => {};
  try {
    await openArea('chats-area'); await openChat(peer, call);
    await started.promise;
    const first = chatMessages.firstElementChild, messages = activeChat.messages;
    await runtimeMail(0, call);
    check(chatMessages.firstElementChild === first && activeChat.messages === messages,
      'Unchanged history replaced DOM or message objects');
    reply.resolve(bytes); await window.stickerImagesLoaded;
    check(reads === 1 && first.querySelector('canvas'), 'Idle redraw repeated or lost an image read');
    openDelete(messages[1]);
    check(deleteDialog.open, 'Preserved message lost its action guard'); closeDelete();
    version = 1; started = deferred(); reply = deferred();
    await openChat(peer, call); await started.promise;
    const stale = chatMessages.firstElementChild, oldReply = reply, oldBatch = window.stickerImagesLoaded;
    version = 2; started = deferred(); reply = deferred(); reply.resolve(bytes);
    await openChat(peer, call); await started.promise; await window.stickerImagesLoaded;
    const latest = chatMessages.firstElementChild;
    oldReply.resolve(bytes); await oldBatch;
    check(latest !== stale && latest.querySelector('canvas') && !stale.querySelector('canvas'),
      'Changed history admitted a stale image reply');
    version = 3; started = deferred(); reply = deferred();
    await openChat(peer, call); await started.promise;
    const closing = chatMessages.firstElementChild, closingBatch = window.stickerImagesLoaded;
    closeChat(); reply.resolve(bytes); await closingBatch;
    check(!closing.querySelector('canvas'), 'Closed chat admitted an old image');
  } finally { closeDelete(); closeChat(); window.mailUpdated = runtimeMail; }
};
