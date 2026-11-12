// Exercise events, focus and measured layout without sending a fixture message.
(() => {
  const checkComposerInput = async (attach, stickers, submits, check) => {
    appBindings.chatInput.focus();
    const bounds = appBindings.chatInput.getBoundingClientRect();
    for (const text of ['draft', '']) {
      appBindings.chatInput.value = text;
      appBindings.chatInput.dispatchEvent(new Event('input', { bubbles: true }));
      await appApi.composerShown;
      const changed = appBindings.chatInput.getBoundingClientRect();
      check(
        appBindings.chatSend.hidden === !text && appBindings.chatSend.disabled === !text,
        'Send does not follow text',
      );
      check(
        bounds.width === changed.width &&
          bounds.left === changed.left &&
          document.activeElement === appBindings.chatInput,
        'Send shifted composer geometry or focus',
      );
    }
    for (const button of [attach, stickers, appBindings.chatSend]) {
      check(button.getAttribute('aria-label') && button.title, 'Composer icon is unnamed');
    }
    appBindings.chatInput.value = 'draft';
    appBindings.chatInput.dispatchEvent(new Event('input', { bubbles: true }));
    await appApi.composerShown;
    for (const options of [{ shiftKey: true }, { isComposing: true }]) {
      const event = new KeyboardEvent('keydown', {
        key: 'Enter',
        bubbles: true,
        cancelable: true,
        ...options,
      });
      appBindings.chatInput.dispatchEvent(event);
      check(!event.defaultPrevented && submits() === 0, 'Modified/composing Enter submitted');
    }
    const enter = new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true });
    appBindings.chatInput.dispatchEvent(enter);
    await appApi.composerShown;
    check(enter.defaultPrevented && submits() === 1, 'Enter did not use form submission');
    stickers.focus();
    stickers.click();
    await appApi.stickersShown;
    check(document.getElementById('sticker-picker').open, 'Sticker picker did not open');
    document.getElementById('sticker-picker-cancel').click();
    check(
      !document.getElementById('sticker-picker').open && document.activeElement === stickers,
      'Picker Cancel did not restore composer focus',
    );
  };
  const checkEmptySend = async () => {
    appBindings.activeChat = null;
    const { sendInChat } = await import(new URL('chats-ui.js', document.baseURI).href);
    const unexpected = () => {
      throw new Error('Send without an open chat reached an operation');
    };
    await sendInChat(unexpected, unexpected, unexpected, unexpected);
  };
  smoke.checkChatComposer = async () => {
    await openArea('chats-area');
    const form = document.getElementById('chat-composer');
    const attach = document.getElementById('chat-attach');
    const stickers = document.getElementById('chat-stickers');
    const check = (ok, reason) => {
      if (!ok) throw new Error(reason);
    };
    const before = {
      chat: appBindings.activeChat,
      value: appBindings.chatInput.value,
      detail: appBindings.chatDetail.hidden,
      overview: appBindings.chatOverview.hidden,
    };
    let submits = 0;
    const intercept = (event) => {
      event.preventDefault();
      event.stopImmediatePropagation();
      submits += 1;
    };
    form.addEventListener('submit', intercept, true);
    try {
      await checkEmptySend();
      appBindings.activeChat = { peerId: appBindings.current.peerId, state: 'open', muted: false };
      appBindings.chatDetail.hidden = false;
      appBindings.chatOverview.hidden = true;
      appBindings.chatInput.value = '';
      appBindings.updateComposer();
      await checkComposerInput(attach, stickers, () => submits, check);
      for (const state of ['requestSent', 'closed', 'requestReceived']) {
        appBindings.activeChat.state = state;
        appBindings.updateComposer();
        check(attach.disabled && stickers.disabled, 'Pre-match/closed chat enabled media');
        check(
          appBindings.chatInput.disabled === (state !== 'requestReceived'),
          'Text reply guard changed',
        );
      }
    } finally {
      form.removeEventListener('submit', intercept, true);
      appApi.stickerCollection.close();
      appBindings.activeChat = before.chat;
      appBindings.chatInput.value = before.value;
      appBindings.chatDetail.hidden = before.detail;
      appBindings.chatOverview.hidden = before.overview;
      appBindings.updateComposer();
    }
  };
})();
