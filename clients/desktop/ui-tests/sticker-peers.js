// Ordered real-node steps: the other profile is a stopped process, never an IPC mock.
(() => {
  const check = (ok, why) => { if (!ok) throw new Error(why); };
  const same = (left, right) => JSON.stringify(left) === JSON.stringify(right);
  const get = (id) => document.getElementById(id);
  const selection = (peerId, message) => ({ identityId: current.peerId,
    selection: { peerId, groupId: message.groupId, author: message.author, id: message.id } });
  // Lazy image/status controllers publish through the DOM, so observe their completion.
  const shown = (read, what) => new Promise((resolve, reject) => {
    const observer = new MutationObserver(update);
    const timer = startTimer(() => { observer.disconnect(); reject(Error(`Timed out: ${what}`)); }, 30000);
    function update() {
      const value = read();
      if (value) { observer.disconnect(); stopTimer(timer); resolve(value); }
    }
    observer.observe(chatMessages, { subtree: true, childList: true, attributes: true, characterData: true });
    update();
  });
  const history = async (peer) => (await invoke('chat_history', { peerId: peer })).messages;
  const received = async (peer) => {
    const deadline = performance.now() + 30000;
    do {
      // Each completed node pass is the next observation, without a fixed interval.
      await invoke('receive_mail');
      const message = (await history(peer)).find((item) => item.author === peer && item.sticker);
      if (message) return message;
    } while (performance.now() < deadline);
    throw Error('Sticker did not reach the other client');
  };
  const metadata = (message, packId) => {
    check(message.sticker && !message.text && !message.photo && !message.deleted, 'Sticker became another message kind');
    const sticker = message.sticker;
    check(sticker.sha256.map((byte) => byte.toString(16).padStart(2, '0')).join('')
      === '83e7a26e05ed89fa9d6732336ca8cb93b0dceb591df75ebcb9bd72eece9bf82c', 'Sticker pixels identify another fixture');
    check(sticker.emoji === '😺' && sticker.packName === 'Native smoke cats' && same(sticker.packId, packId),
      'Sticker fallback or pack ID changed');
    check(Object.keys(sticker).sort().join() === 'emoji,packId,packName,sha256', 'Sticker exposed a media capability');
  };
  const open = async (peer) => {
    await openArea('chats-area'); await window.chatsDone;
    await openChat(peer);
    check(activeChat?.state === 'open', 'Sticker flow requires the established conversation');
  };
  const row = (own = false) => [...chatMessages.querySelectorAll('.sticker-message')]
    .find((item) => item.classList.contains('out') === own);
  const send = async (peer, packId) => {
    const before = (await history(peer)).filter((message) => message.sticker).length;
    chatInput.value = 'keep sticker draft'; chatInput.dispatchEvent(new Event('input'));
    get('chat-stickers').click(); await window.stickersShown;
    const pack = [...get('sticker-picker-choices').children]
      .find((button) => button.dataset.pack === JSON.stringify(packId));
    check(pack && !pack.disabled, 'Saved pack is missing from the real picker');
    pack.click(); await window.stickersShown;
    const choice = get('sticker-picker-grid').querySelector('button');
    check(choice && !choice.disabled && choice.querySelector('canvas'), 'Real picker has no validated thumbnail');
    choice.click(); await window.stickerSent;
    check(!get('sticker-picker').open && chatInput.value === 'keep sticker draft', 'Sticker send failed or cleared the text draft');
    const after = (await history(peer)).filter((message) => message.sticker);
    check(after.length === before + 1, 'Picker send did not persist exactly one sticker');
    metadata(after.at(-1), packId);
    const recent = await invoke('recent_sticker_page', { identityId: current.peerId, after: 0 });
    check(recent.entries.some((entry) => same(entry.packId, packId) && entry.emoji === '😺'), 'Sent sticker did not enter recents');
  };
  const requestState = (peer, message) => invoke('chat_sticker_pack_status', selection(peer, message));
  const steps = {
    'sticker-send': async (peer) => {
      await window.stickerFileChecks.importPack();
      const packId = await window.stickerFileChecks.readback();
      await open(peer); await send(peer, packId); await shot('stickers-peer-send');
    },
    'sticker-request': async (peer) => {
      await invoke('set_auto_download_mode', { mode: 'onTap' });
      const message = await received(peer);
      metadata(message, message.sticker.packId);
      await open(peer);
      const bubble = row(); check(bubble, 'Received sticker has no row');
      bubble.scrollIntoView({ block: 'center' });
      const load = bubble.querySelector('[data-action="load-sticker"]');
      await shown(() => !load.disabled && bubble.textContent.includes(window.strings['stickerImage.tap']), 'manual sticker load');
      load.click(); await window.stickerImageLoaded;
      check(!bubble.querySelector('canvas') && bubble.textContent.includes('😺')
        && bubble.textContent.includes('Native smoke cats')
        && bubble.textContent.includes(window.strings['stickerImage.unavailable']), 'Expired media lost its safe fallback');
      check((await requestState(peer, message)).state === 'new', 'Unexpected pack intent before Add');
      const add = await shown(() => bubble.querySelector('[data-action="add"]:not(:disabled)'), 'Add pack');
      add.click(); await window.stickerRequestChanged; await window.stickerRequestDelivered;
      const pending = await requestState(peer, message);
      check(pending.state === 'waiting' && pending.generation?.length === 16, 'Offline sender request was not retained');
      check(!(await invoke('saved_sticker_packs', { identityId: current.peerId, after: null })).entries.length,
        'Pack appeared before its offline owner answered');
      await shot('stickers-peer-waiting');
    },
    'sticker-answer': async (peer) => {
      // This is a fresh sender process: the request arrived while it was stopped.
      await invoke('receive_mail');
      await window.stickerFileChecks.readback();
      check((await history(peer)).filter((message) => message.sticker).length === 1,
        'Hidden request leaked into visible history');
    },
    'sticker-save-reply': async (peer) => {
      const message = await received(peer);
      const deadline = performance.now() + 30000;
      let status;
      do {
        await invoke('receive_mail');
        status = await requestState(peer, message);
        if (status.state === 'saved') break;
        check(status.state !== 'failed', `Pack download failed: ${status.failure}`);
      } while (performance.now() < deadline);
      check(status.state === 'saved' && status.generation === null, 'Requested pack did not save after sender restart');
      const packId = await window.stickerFileChecks.readback();
      metadata(message, packId);
      await open(peer);
      const bubble = row(); check(bubble, 'Saved sticker has no row');
      bubble.scrollIntoView({ block: 'center' });
      const canvas = await shown(() => bubble.querySelector('canvas'), 'saved-pack pixels');
      check(canvas.width > 0 && canvas.width <= 512 && canvas.height > 0 && canvas.height <= 512,
        'Saved pack did not recover bounded pixels after individual media expiry');
      await send(peer, packId); await shot('stickers-peer-saved-reply');
    },
    'sticker-reply-seen': async (peer) => {
      const reply = await received(peer);
      metadata(reply, await window.stickerFileChecks.readback());
      await open(peer);
      check(row(), 'Returned sticker has no visible row after restart');
      check((await history(peer)).filter((message) => message.sticker).length === 2,
        'Reply or hidden packet projection changed after restart');
      await shot('stickers-peer-reply-seen');
    },
  };
  window.stickerPeerChecks = { steps };
})();
