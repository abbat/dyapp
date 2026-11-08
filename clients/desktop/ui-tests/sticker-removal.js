(() => {
  const get = (id) => document.getElementById(`sticker-${id}`);
  const deferred = () => {
    let resolve;
    const promise = new Promise((done) => {
      resolve = done;
    });
    return { promise, resolve };
  };
  const check = (ok, reason) => {
    if (!ok) throw new Error(reason);
  };
  const createRemovalFixture = () => {
    const ui = window.stickerCollection;
    const panel = document.getElementById('sticker-packs');
    const entry = { id: Array(32).fill(7), name: '<Cats>', stickers: 1, bytes: 12, error: null };
    const page = { entries: [entry], next: null, usedBytes: 12, unrecognizedFiles: 0 };
    // Failed phases close synchronously; outer finally shares that cleanup.
    let closed = false;
    const fixture = {
      ui,
      close() {
        if (closed) return;
        closed = true;
        ui.close();
      },
      panel,
      entry,
      page,
      removes: 0,
      open(call) {
        panel.open = true;
        return ui.open(call);
      },
      button: () => get('list').querySelectorAll('button')[0],
    };
    fixture.call = async (command) => {
      if (command === 'remove_saved_sticker_pack') ++fixture.removes;
      return page;
    };
    return fixture;
  };
  const checkRemovalCancellation = async (fixture) => {
    try {
      const { ui, panel, open, button, call } = fixture;
      await open(call);
      button().click();
      check(
        get('remove-dialog').open && document.activeElement === get('remove-cancel'),
        'Cancel is not initial focus',
      );
      check(
        get('remove-name').textContent === '<Cats>' &&
          get('remove-id').textContent === '07'.repeat(32),
        'Confirmation lost pack name or full ID',
      );
      get('remove-cancel').click();
      check(!get('remove-dialog').open && fixture.removes === 0, 'Cancel removed a pack');
      button().click();
      get('remove-dialog').dispatchEvent(new Event('cancel', { cancelable: true }));
      check(!get('remove-dialog').open && fixture.removes === 0, 'Escape removed a pack');
      button().click();
      await ui.open(call);
      get('remove-confirm').click();
      check(fixture.removes === 0 && !get('remove-dialog').open, 'Old dialog survived a new view');
      button().click();
      document.dispatchEvent(
        Object.assign(new Event('dyapp:area-change'), { detail: 'profile-area' }),
      );
      get('remove-confirm').click();
      check(fixture.removes === 0 && !panel.open, 'Old dialog survived navigation');
      await open(call);
      button().click();
      document.dispatchEvent(new Event('dyapp:identity-change'));
      get('remove-confirm').click();
      check(fixture.removes === 0 && !panel.open, 'Old dialog survived identity change');
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  const checkRemovalOwnership = async (fixture) => {
    try {
      const { ui, open, button, entry, page } = fixture;
      const pending = deferred();
      let sent = null;
      await open(async (command, args) => {
        if (command === 'remove_saved_sticker_pack') {
          ++fixture.removes;
          sent = args;
          return pending.promise;
        }
        return page;
      });
      button().click();
      get('remove-confirm').click();
      const removing = window.stickerRemoved;
      get('remove-confirm').click();
      check(
        fixture.removes === 1 && sent.packId.join() === entry.id.join() && sent.identityId,
        'Removal was duplicated or lost its target',
      );
      ui.close();
      const listing = deferred();
      const reopened = open(() => listing.promise);
      pending.resolve(true);
      await removing;
      check(get('import').disabled, 'Old removal unlocked new listing');
      listing.resolve(page);
      await reopened;
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  const checkRemovalOutcome = async (fixture) => {
    try {
      const { ui, button, entry, page } = fixture;
      entry.error = 'unreadable';
      await ui.open(async (command) => {
        if (command === 'remove_saved_sticker_pack') throw 'outcome unknown';
        return page;
      });
      button().click();
      check(
        get('remove-name').textContent === window.strings['stickers.unreadable'],
        'Unreadable pack cannot be selected',
      );
      get('remove-confirm').click();
      await window.stickerRemoved;
      check(
        get('status').textContent === window.strings['stickers.removeError'],
        'Removal error promises a known outcome',
      );
      check(document.activeElement === get('refresh'), 'Removal lost keyboard focus');
      // Use a fresh fixture whose first read has one row, then deletion leaves no rows.
      let deleted = false;
      const refreshed = deferred();
      await ui.open(async (command) => {
        if (command === 'remove_saved_sticker_pack') {
          deleted = true;
          return true;
        }
        return deleted ? refreshed.promise : page;
      });
      button().click();
      get('remove-confirm').click();
      await Promise.resolve();
      check(
        get('status').textContent === window.strings['stickers.loading'],
        'Refresh was confirmed before reading',
      );
      refreshed.resolve({ ...page, entries: [] });
      await window.stickerRemoved;
      check(
        get('list').children.length === 0 &&
          get('status').textContent.includes(window.strings['stickers.removed']),
        'Successful removal did not refresh and confirm',
      );
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  window.checkStickerRemoval = async () => {
    await openArea('settings');
    const fixture = createRemovalFixture();
    try {
      await checkRemovalCancellation(fixture);
      await checkRemovalOwnership(fixture);
      await checkRemovalOutcome(fixture);
    } finally {
      fixture.close();
    }
  };
})();
