// Status, generation CAS and offline staging use events/promises, never timed polling.
(() => {
  const check = (ok, why) => {
    if (!ok) throw new Error(why);
  };
  const deferred = () => {
    let resolve;
    const promise = new Promise((done) => {
      resolve = done;
    });
    return { promise, resolve };
  };
  const token = (value) => Array(16).fill(value);
  const requestCall = async (fixture, command, args) => {
    if (command === 'chat_sticker_pack_status') {
      fixture.started.resolve();
      return fixture.value;
    }
    if (command === 'receive_mail') return fixture.delivery.promise;
    if (command === 'saved_sticker_packs')
      return { entries: [], next: null, usedBytes: 0, unrecognizedFiles: 0 };
    if (command !== 'change_sticker_pack_request') throw Error('Unexpected pack command');
    fixture.actions.push(args);
    fixture.changes += 1;
    if (fixture.reject) {
      fixture.reject = false;
      throw 'requestChanged';
    }
    const kind = args.action.kind;
    fixture.value =
      kind === 'stopWaiting'
        ? { state: 'new', generation: null, failure: null }
        : {
            state: kind === 'retry' ? 'downloading' : 'waiting',
            generation: token(fixture.changes),
            failure: null,
          };
    return fixture.value;
  };
  const createRequestFixture = () => {
    const holder = document.createElement('div');
    holder.className = 'sticker-message';
    holder.style.cssText =
      'position:fixed;inset:0;z-index:100;background:var(--surface);overflow:auto';
    document.body.append(holder);
    let closed = false;
    const fixture = {
      ui: window.stickerRequests,
      holder,
      value: { state: 'new', generation: null, failure: null },
      changes: 0,
      reject: false,
      started: deferred(),
      delivery: deferred(),
      actions: [],
      message: {
        groupId: [9],
        author: 'fixture-peer',
        id: Array(16).fill(1),
        sticker: { packName: '<Cats> $&' },
      },
      runtimeMail: window.mailUpdated,
      close() {
        if (closed) return;
        closed = true;
        fixture.ui.clear();
        holder.remove();
        fixture.delivery.resolve(0);
        window.mailUpdated = fixture.runtimeMail;
      },
    };
    fixture.call = (command, args) => requestCall(fixture, command, args);
    window.mailUpdated = async () => {};
    return fixture;
  };
  const checkInitialRequest = async (fixture) => {
    try {
      const { ui, holder, message, call } = fixture;
      ui.clear();
      const root = ui.controls(message, 'fixture-peer', call);
      holder.append(root);
      await fixture.started.promise;
      await window.stickerRequestsUpdated;
      check(
        root.textContent.includes('<Cats> $&') && !root.querySelector('cats'),
        'Pack name interpreted as markup',
      );
      const button = (key) => root.querySelector(`[data-action="${key}"]`);
      const add = button('add');
      add.focus();
      add.click();
      add.click();
      await window.stickerRequestChanged;
      check(
        fixture.changes === 1 && fixture.actions[0].action.generation === null && button('stop'),
        'Initial request duplicated or lost offline state',
      );
      check(document.activeElement === button('stop'), 'Request transition lost focus');
      check(
        fixture.actions[0].identityId === current.peerId &&
          fixture.actions[0].selection.author === message.author,
        'Request lost owner/selection',
      );
      button('stop').click();
      await window.stickerRequestChanged;
      check(
        fixture.actions[1].action.generation.every((byte) => byte === 1) && button('add'),
        'Stop did not use observed generation',
      );
      return { root, button };
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  const checkRequestRecovery = async (fixture, controls) => {
    try {
      const { ui } = fixture;
      const { button } = controls;
      fixture.value = { state: 'failed', failure: 'quota', generation: token(7) };
      await ui.refresh(0, true);
      check(
        button('retry') && button('manage') && !button('fresh'),
        'Quota recovery exposes wrong actions',
      );
      button('manage').click();
      await window.stickerRequestChanged;
      check(
        document.getElementById('sticker-manager').open,
        'Quota recovery lost collection manager',
      );
      document.getElementById('sticker-manager-cancel').click();
      await window.stickerRequestsUpdated;
      button('retry').click();
      await window.stickerRequestChanged;
      check(
        fixture.actions.at(-1).action.kind === 'retry' &&
          fixture.actions.at(-1).action.generation[0] === 7,
        'Retry lost current generation',
      );
      fixture.value = { state: 'failed', failure: 'invalid', generation: token(8) };
      await ui.refresh(0, true);
      button('fresh').click();
      await window.stickerRequestChanged;
      check(
        fixture.actions.at(-1).action.kind === 'fresh' &&
          fixture.actions.at(-1).action.generation[0] === 8,
        'Fresh request lost generation',
      );
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  const checkRequestConflict = async (fixture, controls) => {
    try {
      const { root, button } = controls;
      fixture.value = { state: 'waiting', generation: token(99), failure: null };
      fixture.reject = true;
      const beforeConflict = fixture.changes;
      button('stop').click();
      await window.stickerRequestChanged;
      check(
        button('stop') &&
          !button('add') &&
          root.textContent.includes(window.strings['stickerRequest.actionFailed']),
        'Uncertain action did not read current status before recovery',
      );
      check(fixture.changes === beforeConflict + 1, 'Generation conflict retried itself');
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  const checkSavedRequestName = async (fixture) => {
    try {
      const { ui, holder, message, call } = fixture;
      ui.clear();
      holder.replaceChildren();
      fixture.started = deferred();
      fixture.value = { state: 'saved', generation: null, failure: null };
      const name = 'X'.repeat(120);
      const named = ui.controls({ ...message, sticker: { packName: name } }, 'fixture-peer', call);
      holder.append(named);
      await fixture.started.promise;
      await window.stickerRequestsUpdated;
      check(
        named.textContent.includes(name) && named.scrollWidth <= named.clientWidth,
        'Saved pack name was clipped or interpreted as markup',
      );
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  const checkOldRequestStatus = async (fixture) => {
    try {
      const { ui, holder, message } = fixture;
      ui.clear();
      holder.replaceChildren();
      const old = deferred();
      fixture.started = deferred();
      const stale = ui.controls(message, 'fixture-peer', () => {
        fixture.started.resolve();
        return old.promise;
      });
      holder.append(stale);
      await fixture.started.promise;
      const oldRead = window.stickerRequestsUpdated;
      ui.clear();
      old.resolve({ state: 'new', generation: null, failure: null });
      await oldRead;
      check(!stale.querySelector('button'), 'Old status enabled an action after Clear');
      holder.replaceChildren();
      fixture.started = deferred();
      const malformed = ui.controls(message, 'fixture-peer', async () => {
        fixture.started.resolve();
        return { state: 'waiting', generation: null, failure: null };
      });
      holder.append(malformed);
      await fixture.started.promise;
      await window.stickerRequestsUpdated;
      check(
        !malformed.querySelector('[data-action="add"]') &&
          malformed.querySelector('[data-action="refresh"]'),
        'Malformed status enabled an unbound request',
      );
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  window.checkStickerRequests = async () => {
    const fixture = createRequestFixture();
    try {
      const controls = await checkInitialRequest(fixture);
      await checkRequestRecovery(fixture, controls);
      await checkRequestConflict(fixture, controls);
      await checkSavedRequestName(fixture);
      await checkOldRequestStatus(fixture);
    } finally {
      fixture.close();
    }
  };
})();
