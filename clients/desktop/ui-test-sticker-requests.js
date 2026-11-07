// Status, generation CAS and offline staging use events/promises, never timed polling.
window.checkStickerRequests = async () => {
  const check = (ok, why) => { if (!ok) throw new Error(why); };
  const deferred = () => { let resolve; const promise = new Promise((done) => { resolve = done; }); return { promise, resolve }; };
  const ui = window.stickerRequests;
  const holder = document.createElement('div'); holder.className = 'sticker-message';
  holder.style.cssText = 'position:fixed;inset:0;z-index:100;background:var(--surface);overflow:auto';
  document.body.append(holder);
  const token = (value) => Array(16).fill(value);
  let value = { state: 'new', generation: null, failure: null }, changes = 0, reject = false;
  let started = deferred();
  const delivery = deferred();
  const actions = [];
  const call = async (command, args) => {
    if (command === 'chat_sticker_pack_status') { started.resolve(); return value; }
    if (command === 'receive_mail') return delivery.promise;
    if (command === 'saved_sticker_packs') return { entries: [], next: null, usedBytes: 0, unrecognizedFiles: 0 };
    if (command !== 'change_sticker_pack_request') throw Error('Unexpected pack command');
    actions.push(args); changes += 1;
    if (reject) { reject = false; throw 'requestChanged'; }
    const kind = args.action.kind;
    value = kind === 'stopWaiting' ? { state: 'new', generation: null, failure: null }
      : { state: kind === 'retry' ? 'downloading' : 'waiting', generation: token(changes), failure: null };
    return value;
  };
  const message = { groupId: [9], author: 'fixture-peer', id: Array(16).fill(1), sticker: { packName: '<Cats> $&' } };
  const runtimeMail = window.mailUpdated; window.mailUpdated = async () => {};
  try {
    ui.clear();
    const root = ui.controls(message, 'fixture-peer', call); holder.append(root);
    await started.promise; await window.stickerRequestsUpdated;
    check(root.textContent.includes('<Cats> $&') && !root.querySelector('cats'), 'Pack name interpreted as markup');
    const button = (key) => root.querySelector(`[data-action="${key}"]`);
    const add = button('add'); add.focus(); add.click(); add.click();
    await window.stickerRequestChanged;
    check(changes === 1 && actions[0].action.generation === null && button('stop'), 'Initial request duplicated or lost offline state');
    check(document.activeElement === button('stop'), 'Request transition lost focus');
    check(actions[0].identityId === current.peerId && actions[0].selection.author === message.author, 'Request lost owner/selection');
    button('stop').click(); await window.stickerRequestChanged;
    check(actions[1].action.generation.every((byte) => byte === 1) && button('add'), 'Stop did not use observed generation');
    value = { state: 'failed', failure: 'quota', generation: token(7) };
    await ui.refresh(0, true);
    check(button('retry') && button('manage') && !button('fresh'), 'Quota recovery exposes wrong actions');
    button('manage').click(); await window.stickerRequestChanged;
    check(document.getElementById('sticker-manager').open, 'Quota recovery lost collection manager');
    document.getElementById('sticker-manager-cancel').click(); await window.stickerRequestsUpdated;
    button('retry').click(); await window.stickerRequestChanged;
    check(actions.at(-1).action.kind === 'retry' && actions.at(-1).action.generation[0] === 7, 'Retry lost current generation');
    value = { state: 'failed', failure: 'invalid', generation: token(8) };
    await ui.refresh(0, true);
    button('fresh').click(); await window.stickerRequestChanged;
    check(actions.at(-1).action.kind === 'fresh' && actions.at(-1).action.generation[0] === 8, 'Fresh request lost generation');
    value = { state: 'waiting', generation: token(99), failure: null }; reject = true;
    const beforeConflict = changes;
    button('stop').click(); await window.stickerRequestChanged;
    check(button('stop') && !button('add') && root.textContent.includes(window.strings['stickerRequest.actionFailed']),
      'Uncertain action did not read current status before recovery');
    check(changes === beforeConflict + 1, 'Generation conflict retried itself');
    ui.clear(); holder.replaceChildren(); started = deferred();
    value = { state: 'saved', generation: null, failure: null };
    const name = 'X'.repeat(120);
    const named = ui.controls({ ...message, sticker: { packName: name } }, 'fixture-peer', call); holder.append(named);
    await started.promise; await window.stickerRequestsUpdated;
    check(named.textContent.includes(name) && named.scrollWidth <= named.clientWidth,
      'Saved pack name was clipped or interpreted as markup');
    ui.clear(); holder.replaceChildren();
    const old = deferred(); started = deferred();
    const stale = ui.controls(message, 'fixture-peer', () => { started.resolve(); return old.promise; }); holder.append(stale);
    await started.promise; const oldRead = window.stickerRequestsUpdated;
    ui.clear(); old.resolve({ state: 'new', generation: null, failure: null }); await oldRead;
    check(!stale.querySelector('button'), 'Old status enabled an action after Clear');
    holder.replaceChildren(); started = deferred();
    const malformed = ui.controls(message, 'fixture-peer', async () => {
      started.resolve(); return { state: 'waiting', generation: null, failure: null };
    }); holder.append(malformed);
    await started.promise; await window.stickerRequestsUpdated;
    check(!malformed.querySelector('[data-action="add"]') && malformed.querySelector('[data-action="refresh"]'),
      'Malformed status enabled an unbound request');
  } finally { ui.clear(); holder.remove(); delivery.resolve(0); window.mailUpdated = runtimeMail; }
};
