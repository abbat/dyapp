// Exercise events, focus and measured layout without sending a fixture message.
window.checkChatComposer = async () => {
  await openArea('chats-area');
  const form = document.getElementById('chat-composer');
  const attach = document.getElementById('chat-attach');
  const stickers = document.getElementById('chat-stickers');
  const check = (ok, reason) => { if (!ok) throw new Error(reason); };
  const before = { chat: activeChat, value: chatInput.value, detail: chatDetail.hidden, overview: chatOverview.hidden };
  let submits = 0;
  const intercept = (event) => { event.preventDefault(); event.stopImmediatePropagation(); submits += 1; };
  form.addEventListener('submit', intercept, true);
  try {
    activeChat = { peerId: current.peerId, state: 'open', muted: false };
    chatDetail.hidden = false;
    chatOverview.hidden = true;
    chatInput.value = '';
    updateComposer();
    chatInput.focus();
    const bounds = chatInput.getBoundingClientRect();
    for (const text of ['draft', '']) {
      chatInput.value = text;
      chatInput.dispatchEvent(new Event('input', { bubbles: true }));
      const changed = chatInput.getBoundingClientRect();
      check(chatSend.hidden === !text && chatSend.disabled === !text, 'Send does not follow text');
      check(bounds.width === changed.width && bounds.left === changed.left
        && document.activeElement === chatInput, 'Send shifted composer geometry or focus');
    }
    for (const button of [attach, stickers, chatSend]) {
      check(button.getAttribute('aria-label') && button.title, 'Composer icon is unnamed');
    }
    chatInput.value = 'draft';
    chatInput.dispatchEvent(new Event('input', { bubbles: true }));
    for (const options of [{ shiftKey: true }, { isComposing: true }]) {
      const event = new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true, ...options });
      chatInput.dispatchEvent(event);
      check(!event.defaultPrevented && submits === 0, 'Modified/composing Enter submitted');
    }
    const enter = new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true });
    chatInput.dispatchEvent(enter);
    check(enter.defaultPrevented && submits === 1, 'Enter did not use form submission');
    stickers.focus();
    stickers.click();
    await window.stickersShown;
    check(document.getElementById('sticker-picker').open, 'Sticker picker did not open');
    document.getElementById('sticker-picker-cancel').click();
    check(!document.getElementById('sticker-picker').open && document.activeElement === stickers,
      'Picker Cancel did not restore composer focus');
    for (const state of ['requestSent', 'closed', 'requestReceived']) {
      activeChat.state = state;
      updateComposer();
      check(attach.disabled && stickers.disabled, 'Pre-match/closed chat enabled media');
      check(chatInput.disabled === (state !== 'requestReceived'), 'Text reply guard changed');
    }
  } finally {
    form.removeEventListener('submit', intercept, true);
    window.stickerCollection.close();
    activeChat = before.chat;
    chatInput.value = before.value;
    chatDetail.hidden = before.detail;
    chatOverview.hidden = before.overview;
    updateComposer();
  }
};
