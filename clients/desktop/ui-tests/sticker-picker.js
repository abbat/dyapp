// Callback-driven tests of the shipped picker; native IPC execution is a separate CI check.
window.checkStickerPicker = async () => {
  const el = (id) => document.getElementById(`sticker-picker-${id}`);
  const ui = window.stickerPicker;
  const check = (ok, why) => { if (!ok) throw new Error(why); };
  const deferred = () => { let resolve; const promise = new Promise((done) => { resolve = done; }); return { promise, resolve }; };
  const id = Array(32).fill(1);
  const frame = { width: 1, height: 1, pixels: [255, 100, 0, 255] };
  const calls = [];
  const call = async (command, args) => {
    calls.push({ command, args });
    if (command === 'saved_sticker_packs') return { entries: [{ id, name: '<Cats> $&' }], next: null };
    if (command === 'send_saved_sticker') return Array(16).fill(2);
    return { entries: [{ packId: id, packName: '<Cats> $&', index: args.after, emoji: '😺', frame }],
      next: args.after === 0 ? 1 : null };
  };
  const before = chatInput.value;
  let sent = 0;
  try {
    chatInput.value = 'keep this draft';
    await ui.open({ peerId: 'fixture-peer', call, onSent: () => { sent += 1; } });
    check(el('grid').children.length === 1 && el('grid').querySelector('canvas'), 'Missing safe thumbnail');
    const nav = document.getElementById('sticker-picker').querySelector('.sticker-pagination');
    const gap = parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--space-2'));
    check(el('grid').getBoundingClientRect().top - nav.getBoundingClientRect().bottom >= gap,
      'Pack paging touches the sticker grid');
    check(!el('choices').querySelector('img') && el('choices').textContent.includes('<Cats> $&'), 'Pack name interpreted as markup');
    el('next').click(); await window.stickersShown;
    check(el('next').disabled && !el('previous').disabled, 'Next/Previous cursor state');
    el('previous').click(); await window.stickersShown;
    check(calls.at(-1).args.after === 0, 'Previous did not restore cursor');
    el('choices').children[1].click(); await window.stickersShown;
    check(calls.at(-1).command === 'saved_sticker_page' && calls.at(-1).args.packId === id,
      'Saved pack choice lost its ID');
    const button = el('grid').querySelector('button'); button.click(); button.click();
    await window.stickerSent;
    check(sent === 1 && calls.filter((entry) => entry.command === 'send_saved_sticker').length === 1, 'Duplicate durable send');
    check(chatInput.value === 'keep this draft' && !document.getElementById('sticker-picker').open, 'Send cleared draft or left picker open');
    await ui.open({ peerId: 'fixture-peer', call });
    el('manage').click(); await window.stickersShown;
    check(document.getElementById('sticker-manager').open, 'Manager entry lost');
    document.getElementById('sticker-manager-cancel').click(); await window.stickersShown;
    check(document.getElementById('sticker-picker').open, 'Manager did not return to picker');
    ui.close();
    let fail = true;
    await ui.open({ peerId: 'fixture-peer', call: async (command, args) => {
      if (fail && command === 'recent_sticker_page') throw Error('read failed');
      return call(command, args);
    } });
    check(!el('retry').hidden && !el('retry').disabled, 'Missing load recovery');
    fail = false; el('retry').click(); await window.stickersShown;
    check(el('grid').children.length === 1, 'Retry did not reload the page');
    const old = deferred(), fresh = deferred();
    const first = ui.open({ peerId: 'fixture-peer', call: () => old.promise });
    ui.close();
    const second = ui.open({ peerId: 'fixture-peer', call: () => fresh.promise });
    old.resolve({ entries: [], next: null }); await first;
    check(el('manage').disabled, 'Old read unlocked reopened picker');
    fresh.resolve({ entries: [], next: null }); await second;
    check(!el('manage').disabled && !el('status').hidden, 'Empty recent recovery');
    const stale = deferred();
    const pending = ui.open({ peerId: 'fixture-peer', call: () => stale.promise });
    ui.leave('another-peer');
    stale.resolve({ entries: [], next: null }); await pending;
    check(!document.getElementById('sticker-picker').open, 'Old peer reopened picker');
    for (const bad of [{ ...frame, width: 513 }, { ...frame, pixels: [0] }, { ...frame, pixels: [0, -1, 0, 255] }]) {
      let rejected = false;
      try { window.stickerPixels(bad, 'bad'); } catch { rejected = true; }
      check(rejected, 'Malformed sticker pixels accepted');
    }
  } finally { ui.close(); chatInput.value = before; }
};
