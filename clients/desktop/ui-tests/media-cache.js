(() => {
  const budget = (mib) => ({ kind: 'budget', mib });
  const retention = (enabled) => ({ kind: 'retention', enabled });
  const clearEdit = { kind: 'clear' };
  const closeCacheDialog = async (dialog, button) => {
    const finished = closed(dialog);
    button.click();
    await smoke.waitForSignal('cache confirmation dialog close', finished);
  };
  const load = (call) => smoke.waitForSignal('cache state read', appApi.showMediaCache(call));
  const createCacheFixture = (check) => {
    const fixture = {
      config: {
        enabled: true,
        limitBytes: 393216123,
        usedBytes: 16,
        minBytes: 2 * 1048576,
        maxBytes: 1024 * 1048576,
        limitMib: '375.00011730194091796875',
        minMib: '2',
        maxMib: '1024',
        confirmationRequired: false,
      },
      writes: [],
      steps: [],
    };
    // Script native verdicts; this fixture does not parse MiB or duplicate destructive policy.
    fixture.reply = (change, confirmed, patch = {}, ask = false, error = null) =>
      fixture.steps.push({ change, confirmed, patch, ask, error });
    fixture.call = async (command, args) => {
      check(args.identityId === appBindings.current.peerId, 'Cache IPC lacks viewed identity');
      if (command === 'media_cache') return { ...fixture.config };
      check(command === 'change_media_cache', 'Unexpected cache IPC');
      const step = fixture.steps.shift();
      check(Boolean(step), 'Unscripted cache change');
      const keys = Object.keys(step.change);
      check(
        args.confirmed === step.confirmed &&
          Object.keys(args.change).length === keys.length &&
          keys.every((key) => args.change[key] === step.change[key]),
        'Cache request changed its owner-bound single-field intent',
      );
      if (step.error) return Promise.reject(step.error);
      if (!step.ask) {
        fixture.writes.push(args);
        fixture.config = { ...fixture.config, ...step.patch };
      }
      return { ...fixture.config, confirmationRequired: step.ask };
    };
    return fixture;
  };
  const checkCacheBounds = async (fixture, { el, dialog, edit, check }) => {
    const expected = appApi.strings['photos.cache.invalid']
      .replace('{min}', fixture.config.minMib)
      .replace('{max}', fixture.config.maxMib);
    for (const value of ['1', '1025']) {
      fixture.reply(budget(value), false, {}, false, 'invalidLimit');
      await edit(value);
      check(
        fixture.writes.length === 0 &&
          !dialog.open &&
          document.getElementById('media-cache-status').textContent === expected,
        'Core refusal did not retain the actual range and settings',
      );
    }
    check(el('limit').min === '2' && el('limit').max === '1024', 'Input bounds not from IPC');
  };
  const checkCacheBudgets = async (fixture, controls) => {
    const { el, dialog, cancel, confirm, edit, check, saved } = controls;
    await load(fixture.call);
    check(el('limit').value === fixture.config.limitMib, 'Exact MiB display lost byte precision');
    await checkCacheBounds(fixture, controls);
    fixture.reply(budget('512'), false, { limitBytes: 512 * 1048576, limitMib: '512' });
    await edit('512');
    check(!dialog.open && fixture.config.limitBytes === 512 * 1048576, 'Larger budget not applied');
    fixture.reply(budget('384'), false, {}, true);
    await edit('384');
    check(dialog.open && fixture.writes.length === 1, 'Core confirmation was bypassed');
    await closeCacheDialog(dialog, cancel);
    check(
      fixture.config.limitBytes === 512 * 1048576 && el('limit').value === '512',
      'Cancel changed budget',
    );
    fixture.reply(budget('384'), false, {}, true);
    await edit('384');
    fixture.reply(budget('384'), true, { limitBytes: 384 * 1048576, limitMib: '384' });
    await closeCacheDialog(dialog, confirm);
    await saved('confirmed cache budget');
    check(fixture.config.limitBytes === 384 * 1048576, 'Confirmed budget not applied');
  };
  const checkCacheRetention = async (fixture, { el, dialog, confirm, check, saved }) => {
    fixture.reply(retention(false), false, {}, true);
    el('enabled').click();
    await saved('cache retention admission');
    check(dialog.open && fixture.config.enabled, 'Disabling cache did not ask');
    fixture.reply(retention(false), true, { enabled: false, usedBytes: 0 });
    await closeCacheDialog(dialog, confirm);
    await saved('confirmed retention change');
    check(
      !fixture.config.enabled &&
        el('limit').disabled &&
        fixture.config.limitBytes === 384 * 1048576,
      'Disabling lost remembered quota',
    );
    fixture.reply(retention(true), false, { enabled: true });
    el('enabled').click();
    await saved('enable cache retention');
    check(
      fixture.config.enabled &&
        !el('limit').disabled &&
        fixture.config.limitBytes === 384 * 1048576,
      'Enabling lost quota',
    );
  };
  const checkCacheClear = async (fixture, { el, dialog, cancel, confirm, check, saved }) => {
    fixture.reply(clearEdit, false, {}, true);
    el('clear').click();
    await saved('cache clear admission');
    check(dialog.open && fixture.config.usedBytes === 16, 'Clear did not ask');
    await closeCacheDialog(dialog, cancel);
    fixture.reply(clearEdit, false, {}, true);
    el('clear').click();
    await saved('second cache clear admission');
    fixture.reply(clearEdit, true, { usedBytes: 0 });
    await closeCacheDialog(dialog, confirm);
    await saved('confirmed cache clear');
    check(fixture.config.usedBytes === 0 && el('clear').disabled, 'Clear lost measured usage');
  };
  const checkCacheInvalidInput = async (fixture, { dialog, edit, check }) => {
    const count = fixture.writes.length;
    for (const value of ['', '0', '10241']) {
      fixture.reply(budget(value), false, {}, false, 'invalidLimit');
      await edit(value);
    }
    check(fixture.writes.length === count && !dialog.open, 'Rejected input mutated cache');
    return count;
  };
  const checkCacheStaleResponses = async (fixture, { el, edit, check }) => {
    const heldRead = smoke.pendingSmokeCall();
    const old = appApi.showMediaCache(() => heldRead.call());
    await smoke.waitForSignal('old cache read admission', heldRead.started);
    await load(fixture.call);
    heldRead.resolve({
      ...fixture.config,
      enabled: false,
      limitBytes: 1048576,
      limitMib: '1',
      usedBytes: 99,
    });
    await smoke.waitForSignal('old cache read completion', old);
    check(el('enabled').checked && el('limit').value === '384', 'Old read repainted new cache');
    const heldWrite = smoke.pendingSmokeCall();
    await load((command) => {
      if (command === 'media_cache') return Promise.resolve(fixture.config);
      check(command === 'change_media_cache', 'Unexpected held mutation');
      return heldWrite.call();
    });
    const oldWrite = edit('1024');
    await smoke.waitForSignal('old cache write admission', heldWrite.started);
    check(el('enabled').disabled && el('clear').disabled, 'Busy cache accepted more actions');
    await load(fixture.call);
    heldWrite.resolve({ ...fixture.config, enabled: false, limitBytes: 1048576, limitMib: '1' });
    await oldWrite;
    check(el('enabled').checked && el('limit').value === '384', 'Old write repainted new cache');
  };
  const checkCacheFailure = async (fixture, controls, count) => {
    const { el, dialog, confirm, edit, check } = controls;
    await load((command) =>
      command === 'media_cache'
        ? Promise.resolve(fixture.config)
        : Promise.reject(Error('uncertain write')),
    );
    await edit('1024');
    check(
      el('enabled').disabled &&
        el('limit').disabled &&
        !el('refresh').disabled &&
        el('limit').value === '1024',
      'Uncertain write lost draft or enabled another mutation',
    );
    await load(fixture.call);
    fixture.reply(budget('256'), false, {}, true);
    await edit('256');
    const finished = closed(dialog);
    document.dispatchEvent(new Event('dyapp:identity-change'));
    check(!dialog.open && el('enabled').disabled, 'Identity change retained destructive dialog');
    confirm.click();
    check(fixture.writes.length === count, 'Stale confirmation wrote cache');
    await smoke.waitForSignal('retired cache dialog close', finished);
  };
  smoke.checkMediaCache = async () => {
    await openArea('settings', 'photos');
    await smoke.waitForSignal('initial cache state', appApi.mediaCacheShown);
    const el = (name) => document.getElementById(`media-cache-${name}`);
    const dialog = document.getElementById('photo-keep-dialog');
    const cancel = document.getElementById('photo-keep-cancel');
    const confirm = document.getElementById('photo-keep-confirm');
    const check = (ok, reason) => {
      if (!ok) throw Error(reason);
    };
    const fixture = createCacheFixture(check);
    const saved = (phase) => smoke.waitForSignal(phase, appApi.mediaCacheSaved);
    const edit = (value) => {
      el('limit').value = value;
      el('limit').dispatchEvent(new Event('change', { bubbles: true }));
      return saved('cache budget admission');
    };
    const controls = { el, dialog, cancel, confirm, edit, check, saved };
    await checkCacheBudgets(fixture, controls);
    await checkCacheRetention(fixture, controls);
    // A later fetched original becomes visible through a fresh native readback.
    fixture.config.usedBytes = 16;
    await load(fixture.call);
    await checkCacheClear(fixture, controls);
    const count = await checkCacheInvalidInput(fixture, controls);
    await checkCacheStaleResponses(fixture, controls);
    await checkCacheFailure(fixture, controls, count);
    check(fixture.steps.length === 0, 'Native cache verdict was not exercised');
  };
})();
