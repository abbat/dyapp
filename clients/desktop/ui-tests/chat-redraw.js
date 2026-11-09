// An unchanged mailbox pass must not discard a slow image or the message objects in action guards.
(() => {
  const deferred = () => {
    let resolve;
    const promise = new Promise((done) => {
      resolve = done;
    });
    return { promise, resolve };
  };
  const redrawCall = async (fixture, command) => {
    if (command === 'chat_history') {
      if (fixture.historyGate) {
        fixture.historyStarted.resolve();
        await fixture.historyGate.promise;
      }
      return JSON.parse(
        JSON.stringify({
          state: fixture.state,
          muted: false,
          activeRecently: false,
          messages: [
            fixture.sticker,
            fixture.own,
            ...(fixture.version
              ? [{ ...fixture.own, id: Array(16).fill(3), text: String(fixture.version) }]
              : []),
          ],
        }),
      );
    }
    if (command === 'chat_sticker_frame') {
      fixture.reads += 1;
      fixture.started.resolve();
      return fixture.reply.promise;
    }
    if (command === 'chat_sticker_pack_status')
      return { state: 'new', generation: null, failure: null };
    if (command === 'chat_rows' || command === 'contact_names') return [];
    return false;
  };
  const createRedrawFixture = () => {
    const peer = `12D3KooW${'a'.repeat(44)}`;
    const bytes = new Uint8Array(12);
    const header = new DataView(bytes.buffer);
    header.setUint32(0, 1, true);
    header.setUint32(4, 1, true);
    bytes.set([255, 50, 0, 255], 8);
    const fixture = {
      peer,
      bytes,
      version: 0,
      reads: 0,
      started: deferred(),
      reply: deferred(),
      historyGate: null,
      historyStarted: null,
      state: 'open',
      own: {
        groupId: [9],
        author: appBindings.current.peerId,
        id: Array(16).fill(2),
        sentAtMs: 1,
        text: 'keep actions',
      },
      sticker: {
        groupId: [9],
        author: peer,
        id: Array(16).fill(1),
        sentAtMs: 1,
        sticker: { emoji: '😺', packName: 'Cats' },
      },
    };
    fixture.call = (command) => redrawCall(fixture, command);
    return fixture;
  };
  const checkUnchangedRedraw = async (fixture, runtimeMail, check) => {
    await openArea('chats-area');
    await appBindings.openChat(fixture.peer, fixture.call);
    await fixture.started.promise;
    const first = appBindings.chatMessages.firstElementChild,
      messages = appBindings.activeChat.messages;
    appBindings.chatInput.value = 'draft survives refresh';
    appBindings.chatInput.dispatchEvent(new Event('input', { bubbles: true }));
    appBindings.chatInput.focus();
    fixture.historyGate = deferred();
    fixture.historyStarted = deferred();
    const refreshing = runtimeMail(0, fixture.call);
    await fixture.historyStarted.promise;
    check(
      !appBindings.chatInput.disabled &&
        !appBindings.chatSend.disabled &&
        document.activeElement === appBindings.chatInput &&
        appBindings.chatInput.value === 'draft survives refresh',
      'Transparent refresh disabled or blurred the composer',
    );
    fixture.historyGate.resolve();
    fixture.historyGate = null;
    await refreshing;
    check(
      document.activeElement === appBindings.chatInput &&
        appBindings.chatInput.value === 'draft survives refresh',
      'Refresh lost the draft or focus',
    );
    check(
      appBindings.chatMessages.firstElementChild === first &&
        appBindings.activeChat.messages === messages,
      'Unchanged history replaced DOM or message objects',
    );
    fixture.reply.resolve(fixture.bytes);
    await appApi.stickerImagesLoaded;
    check(
      fixture.reads === 1 && first.querySelector('canvas'),
      'Idle redraw repeated or lost an image read',
    );
    appBindings.openDelete(messages[1]);
    check(appBindings.deleteDialog.open, 'Preserved message lost its action guard');
    appBindings.closeDelete();
  };
  const checkChangedRedraw = async (fixture, check) => {
    fixture.version = 1;
    fixture.started = deferred();
    fixture.reply = deferred();
    await appBindings.openChat(fixture.peer, fixture.call);
    await fixture.started.promise;
    const stale = appBindings.chatMessages.firstElementChild,
      oldReply = fixture.reply,
      oldBatch = appApi.stickerImagesLoaded;
    fixture.version = 2;
    fixture.started = deferred();
    fixture.reply = deferred();
    fixture.reply.resolve(fixture.bytes);
    await appBindings.openChat(fixture.peer, fixture.call);
    await fixture.started.promise;
    await appApi.stickerImagesLoaded;
    const latest = appBindings.chatMessages.firstElementChild;
    oldReply.resolve(fixture.bytes);
    await oldBatch;
    check(
      latest !== stale && latest.querySelector('canvas') && !stale.querySelector('canvas'),
      'Changed history admitted a stale image reply',
    );
  };
  const checkClosedRedraw = async (fixture, runtimeMail, check) => {
    fixture.state = 'closed';
    await runtimeMail(0, fixture.call);
    check(
      appBindings.chatInput.disabled &&
        appBindings.chatSend.disabled &&
        appBindings.chatInput.value === 'draft survives refresh',
      'Closed snapshot did not enforce the composer policy',
    );
    fixture.state = 'open';
    fixture.version = 3;
    fixture.started = deferred();
    fixture.reply = deferred();
    await appBindings.openChat(fixture.peer, fixture.call);
    await fixture.started.promise;
    const closing = appBindings.chatMessages.firstElementChild,
      closingBatch = appApi.stickerImagesLoaded;
    check(!appBindings.chatInput.disabled, 'Open snapshot did not restore writable state');
    fixture.historyGate = deferred();
    fixture.historyStarted = deferred();
    const switching = appBindings.openChat('another-peer', fixture.call);
    await fixture.historyStarted.promise;
    check(
      appBindings.chatInput.disabled &&
        appBindings.chatSend.disabled &&
        appBindings.chatInput.value === '',
      'Peer switch retained the old draft or writable composer',
    );
    appBindings.closeChat();
    fixture.historyGate.resolve();
    fixture.historyGate = null;
    await switching;
    fixture.reply.resolve(fixture.bytes);
    await closingBatch;
    check(!closing.querySelector('canvas'), 'Closed chat admitted an old image');
  };
  smoke.checkChatRedraw = async () => {
    const check = (ok, why) => {
      if (!ok) throw new Error(why);
    };
    const fixture = createRedrawFixture();
    const runtimeMail = appApi.mailUpdated;
    appApi.mailUpdated = async () => {};
    try {
      await checkUnchangedRedraw(fixture, runtimeMail, check);
      await checkChangedRedraw(fixture, check);
      await checkClosedRedraw(fixture, runtimeMail, check);
    } finally {
      appBindings.closeDelete();
      appBindings.closeChat();
      appApi.mailUpdated = runtimeMail;
    }
  };
})();
