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
    const ui = appApi.stickerCollection;
    const panel = document.getElementById('sticker-packs');
    const page = {
      entries: [],
      next: null,
      usedBytes: 0,
      unrecognizedFiles: 0,
      maxArchiveBytes: 7,
    };
    const fallback = appApi.stickerMessageText({ emoji: '😺', packName: '$&<img>{emoji}' });
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
        size: fixture.page.maxArchiveBytes + 1,
        arrayBuffer: () => {
          read = true;
        },
      });
      await ui.importFile(null);
      check(!read && calls.length === 1, 'Oversized/cancelled picker read or invoked import');
      check(
        get('status').textContent === appApi.strings['stickers.tooLarge'],
        'Missing file limit explanation',
      );
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  const checkDecodedImportLimit = async (fixture) => {
    const { ui, page, calls } = fixture;
    const before = calls.length;
    await ui.importFile({
      size: 1,
      arrayBuffer: async () => new ArrayBuffer(page.maxArchiveBytes + 1),
    });
    check(calls.length === before, 'Decoded oversized archive reached native import');
    check(
      get('status').textContent === appApi.strings['stickers.tooLarge'],
      'Missing decoded limit explanation',
    );
    await ui.importFile({
      size: page.maxArchiveBytes,
      arrayBuffer: async () => new ArrayBuffer(page.maxArchiveBytes),
    });
    check(
      calls.slice(before).filter(({ command }) => command === 'import_sticker_pack').length === 1,
      'Exact native boundary did not reach native validation',
    );
  };
  const checkMissingImportLimit = async (fixture) => {
    const { ui, open, page, call } = fixture;
    let reads = 0;
    const file = {
      size: 1,
      arrayBuffer: async () => {
        reads += 1;
        return new ArrayBuffer(1);
      },
    };
    for (const maxArchiveBytes of [undefined, 0, -1, 1.5, NaN]) {
      await open(async () => ({ ...page, maxArchiveBytes }));
      check(
        get('import').disabled && !get('refresh').disabled,
        'Invalid metadata enabled import or blocked refresh',
      );
      await ui.importFile(file);
      check(reads === 0, 'Unadmitted file read');
    }
    const listing = deferred();
    const opening = open(() => listing.promise);
    check(get('import').disabled, 'Pending metadata enabled import');
    await ui.importFile(file);
    check(reads === 0, 'Premature file read');
    listing.resolve(page);
    await opening;
    check(!get('import').disabled, 'Ready native metadata did not enable import');
    await open(call);
  };
  const checkStaleImport = async (fixture) => {
    try {
      const { ui, open, calls, page } = fixture;
      const before = calls.length;
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
        calls.length === before && get('import').disabled,
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
      await appApi.stickersShown;
      check(
        cursor === next &&
          get('list').children.length === 1 &&
          get('next').hidden &&
          document.activeElement === get('refresh'),
        'Pagination retained old pages or lost its cursor',
      );
      const row = get('list').children[0];
      check(row.children[0].textContent === '<pack>', 'Pack name is not plain text');
      const id = row.querySelector('code');
      check(
        id?.textContent === '07070707' &&
          id.getAttribute('aria-label') === appApi.strings['stickers.packId'] &&
          id.previousElementSibling?.textContent === appApi.strings['stickers.packId'],
        'Pack ID lacks its own label or still looks like a peer',
      );
      const removeLabel = row.querySelector('button').getAttribute('aria-label');
      check(
        removeLabel.includes('<pack>') && removeLabel.includes('07070707'),
        'Remove lost its unambiguous pack target',
      );
      const summary = fixture.panel.querySelector('summary');
      summary.focus();
      summary.classList.add('focus');
      check(
        document.activeElement === summary &&
          Number.parseFloat(getComputedStyle(summary).outlineOffset) === 0,
        `Saved packs focus ring overlaps its disclosure marker: active=${document.activeElement === summary}, offset=${getComputedStyle(summary).outlineOffset}`,
      );
      await smoke.shot('stickers-collection-focus');
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
      const before = calls.length;
      await ui.importFile({ size: 1, arrayBuffer: async () => new ArrayBuffer(1) });
      const imported = calls.slice(before).find((item) => item.command === 'import_sticker_pack');
      check(
        imported?.args.identityId === expected && imported.args.bytes.length === 1,
        'Import lost identity or bytes',
      );
      check(
        get('status').textContent.includes(appApi.strings['stickers.saved']) && get('hint').hidden,
        'Import lacks a single local confirmation',
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
          get('status').textContent === appApi.strings[`stickers.${key}`],
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
        get('reason').textContent === '' && get('details').hidden,
        'Read failure exposes a redacted placeholder',
      );
      check(
        get('status').textContent === appApi.strings['stickers.error'],
        'Read failure lacks retry',
      );
      await smoke.shot('stickers-read-error');
      await open(async () => {
        throw new Error('permission denied');
      });
      check(
        get('reason').textContent.includes('permission denied') &&
          get('reason').children.length === 0 &&
          !get('details').hidden,
        'Read failure lost actionable text-only details',
      );
      get('details').open = true;
      await smoke.shot('stickers-read-error-details');
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
  smoke.checkStickerCollection = async () => {
    await openArea('settings');
    const fixture = createCollectionFixture();
    try {
      await checkImportLimit(fixture);
      await checkDecodedImportLimit(fixture);
      await checkMissingImportLimit(fixture);
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
