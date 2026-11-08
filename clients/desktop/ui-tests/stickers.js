// Exercise the shipped controller with delayed replies, without writing fixture packs.
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
  const createCollectionFixture = () => {
    const ui = window.stickerCollection;
    const panel = document.getElementById('sticker-packs');
    const page = { entries: [], next: null, usedBytes: 0, unrecognizedFiles: 0 };
    const fallback = window.stickerMessageText({ emoji: '😺', packName: '$&<img>{emoji}' });
    check(
      fallback.includes('$&<img>{emoji}') && !fallback.includes('{name}'),
      'plain sticker label',
    );
    const open = (call) => {
      panel.open = true;
      return ui.open(call);
    };
    const calls = [];
    const call = async (command, args) => {
      calls.push({ command, args });
      return page;
    };
    let closed = false;
    return {
      ui,
      panel,
      page,
      open,
      calls,
      call,
      close() {
        if (closed) return;
        closed = true;
        ui.close();
      },
    };
  };
  const checkImportLimit = async (fixture) => {
    try {
      const { ui, open, calls, call } = fixture;
      await open(call);
      let read = false;
      await ui.importFile({
        size: 8 * 1024 * 1024 + 1,
        arrayBuffer: () => {
          read = true;
        },
      });
      await ui.importFile(null);
      check(!read && calls.length === 1, 'Oversized/cancelled picker read or invoked import');
      check(
        get('status').textContent === window.strings['stickers.tooLarge'],
        'Missing file limit explanation',
      );
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  const checkStaleImport = async (fixture) => {
    try {
      const { ui, open, calls, page } = fixture;
      const bytes = deferred();
      const importing = ui.importFile({ size: 1, arrayBuffer: () => bytes.promise });
      await ui.importFile({
        size: 1,
        arrayBuffer: () => {
          throw new Error('Duplicate import read');
        },
      });
      ui.close();
      const listing = deferred();
      const reopened = open(() => listing.promise);
      bytes.resolve(new ArrayBuffer(1));
      await importing;
      check(
        calls.length === 1 && get('import').disabled,
        'Old file read imported or unlocked a new view',
      );
      listing.resolve(page);
      await reopened;
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  const checkStaleListing = async (fixture) => {
    try {
      const { ui, open, call, page } = fixture;
      const old = deferred();
      const stale = ui.open(() => old.promise);
      ui.close();
      await open(call);
      old.resolve({
        ...page,
        entries: [{ id: Array(32).fill(1), name: 'Old pack', stickers: 1, bytes: 2 }],
      });
      await stale;
      check(get('list').children.length === 0, 'Old listing changed a reopened collection');
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  const checkCollectionPaging = async (fixture) => {
    try {
      const { ui, page } = fixture;
      let cursor = null;
      const next = Array(32).fill(7);
      await ui.open(async (_command, args) => {
        cursor = args.after;
        return {
          ...page,
          next: cursor ? null : next,
          entries: [{ id: next, name: '<pack>', stickers: 2, bytes: 12, error: null }],
        };
      });
      get('next').focus();
      get('next').click();
      await window.stickersShown;
      check(
        cursor === next &&
          get('list').children.length === 1 &&
          get('next').hidden &&
          document.activeElement === get('refresh'),
        'Pagination retained old pages or lost its cursor',
      );
      check(
        get('list').children[0].children[0].textContent === '<pack>',
        'Pack name is not plain text',
      );
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  const checkImportBinding = async (fixture) => {
    try {
      const { ui, call, calls } = fixture;
      await ui.open(call);
      const expected = calls[0].args.identityId;
      await ui.importFile({ size: 1, arrayBuffer: async () => new ArrayBuffer(1) });
      const imported = calls.find((item) => item.command === 'import_sticker_pack');
      check(
        imported?.args.identityId === expected && imported.args.bytes.length === 1,
        'Import lost identity or bytes',
      );
      check(
        get('status').textContent.includes(window.strings['stickers.saved']),
        'Import lacks local confirmation',
      );
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  const checkImportFailures = async (fixture) => {
    try {
      const { ui, page, panel } = fixture;
      for (const [reason, key] of [
        ['saved sticker pack quota exceeded', 'quota'],
        ['"quota exceeded": unsupported TAR entry type', 'importFailed'],
      ]) {
        await ui.open(async (command) => {
          if (command === 'import_sticker_pack') throw reason;
          return page;
        });
        await ui.importFile({ size: 1, arrayBuffer: async () => new ArrayBuffer(1) });
        check(
          get('status').textContent === window.strings[`stickers.${key}`],
          'Import reason misclassified',
        );
      }
      const pending = deferred();
      const admitted = ui.open(() => pending.promise);
      document.dispatchEvent(new Event('dyapp:identity-change'));
      pending.resolve(page);
      await admitted;
      check(
        !panel.open && get('list').children.length === 0,
        'Identity change retained collection state',
      );
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  const checkCollectionReadFailure = async (fixture) => {
    try {
      const { ui, open } = fixture;
      await open(async () => {
        throw new Error('<untrusted>');
      });
      check(
        get('reason').textContent.includes('<untrusted>') &&
          get('reason').children.length === 0 &&
          !get('details').hidden,
        'Read failure lacks text-only details',
      );
      check(
        get('status').textContent === window.strings['stickers.error'],
        'Read failure lacks retry',
      );
      get('details').open = true;
      await shot('stickers-read-error');
      ui.close();
      check(
        get('reason').textContent === '' && !get('details').open,
        'Closed collection retained error details',
      );
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  window.checkStickerCollection = async () => {
    await openArea('settings');
    const fixture = createCollectionFixture();
    try {
      await checkImportLimit(fixture);
      await checkStaleImport(fixture);
      await checkStaleListing(fixture);
      await checkCollectionPaging(fixture);
      await checkImportBinding(fixture);
      await checkImportFailures(fixture);
      await checkCollectionReadFailure(fixture);
    } finally {
      fixture.close();
    }
  };
})();
