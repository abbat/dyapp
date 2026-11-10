(() => {
  const cacheCall = async (fixture, check, command, args) => {
    check(args.identityId === appBindings.current.peerId, 'Cache IPC lacks viewed identity');
    if (command === 'set_media_cache') {
      fixture.writes.push(args);
      fixture.config = { ...fixture.config, enabled: args.enabled, limitBytes: args.limitBytes };
    } else if (command === 'clear_media_cache') {
      fixture.writes.push(args);
      fixture.config = { ...fixture.config, usedBytes: 0 };
    } else check(command === 'media_cache', 'Unexpected cache IPC');
    return { ...fixture.config };
  };
  const createCacheFixture = (check) => {
    const fixture = { config: { enabled: true, limitBytes: 393216123, usedBytes: 16 }, writes: [] };
    fixture.call = (command, args) => cacheCall(fixture, check, command, args);
    return fixture;
  };
  const checkCacheBudgets = async (fixture, controls) => {
    const { call, writes } = fixture;
    const { el, dialog, cancel, confirm, edit, check } = controls;
    await appApi.showMediaCache(call);
    check(Number(el('limit').value) * 1048576 === 393216123, 'Byte budget lost precision');
    edit('512');
    await appApi.mediaCacheSaved;
    check(
      !dialog.open && fixture.config.limitBytes === 512 * 1048576,
      'Larger numeric budget not saved',
    );
    edit('384');
    check(dialog.open && writes.length === 1, 'Lower budget did not ask');
    cancel.click();
    check(
      fixture.config.limitBytes === 512 * 1048576 && el('limit').value === '512',
      'Cancel changed budget',
    );
    edit('384');
    confirm.click();
    await appApi.mediaCacheSaved;
    check(fixture.config.limitBytes === 384 * 1048576, 'Confirmed lower budget not saved');
    el('enabled').click();
    check(dialog.open && fixture.config.enabled, 'Disabling cache did not ask');
    confirm.click();
    await appApi.mediaCacheSaved;
    check(
      !fixture.config.enabled &&
        el('limit').disabled &&
        fixture.config.limitBytes === 384 * 1048576,
      'Disabling lost quota',
    );
    el('enabled').click();
    await appApi.mediaCacheSaved;
    check(fixture.config.enabled && !el('limit').disabled, 'Enabling did not retain numeric quota');
    el('clear').click();
    check(dialog.open && fixture.config.usedBytes === 16, 'Clear did not ask');
    cancel.click();
    el('clear').click();
    confirm.click();
    await appApi.mediaCacheSaved;
    check(
      fixture.config.usedBytes === 0 && el('clear').disabled,
      'Clear did not refresh measured usage',
    );
    const count = writes.length;
    for (const value of ['', '0', '10241']) edit(value);
    check(writes.length === count && !dialog.open, 'Invalid budget reached IPC');
    return count;
  };
  const checkCacheStaleResponses = async (fixture, controls) => {
    const { call } = fixture;
    const { el, edit, check } = controls;
    let finishOld;
    const old = appApi.showMediaCache(
      () =>
        new Promise((resolve) => {
          finishOld = resolve;
        }),
    );
    await appApi.showMediaCache(call);
    finishOld({ enabled: false, limitBytes: 1048576, usedBytes: 99 });
    await old;
    check(el('enabled').checked && el('limit').value === '384', 'Old read repainted new cache');
    let finishWrite;
    await appApi.showMediaCache((command) =>
      command === 'media_cache'
        ? Promise.resolve(fixture.config)
        : new Promise((resolve) => {
            finishWrite = resolve;
          }),
    );
    edit('1024');
    const oldWrite = appApi.mediaCacheSaved;
    check(el('enabled').disabled && el('clear').disabled, 'Busy cache accepted more actions');
    await appApi.showMediaCache(call);
    finishWrite({ enabled: false, limitBytes: 1048576, usedBytes: 0 });
    await oldWrite;
    check(el('enabled').checked && el('limit').value === '384', 'Old write repainted new cache');
  };
  const checkCacheFailure = async (fixture, controls, count) => {
    const { call, writes } = fixture;
    const { el, dialog, confirm, edit, check } = controls;
    await appApi.showMediaCache((command) =>
      command === 'media_cache'
        ? Promise.resolve(fixture.config)
        : Promise.reject(Error('uncertain write')),
    );
    edit('1024');
    await appApi.mediaCacheSaved;
    check(
      el('enabled').disabled && el('limit').disabled && !el('refresh').disabled,
      'Uncertain write did not require fresh read',
    );
    await appApi.showMediaCache(call);
    edit('256');
    const finished = closed(dialog);
    document.dispatchEvent(new Event('dyapp:identity-change'));
    check(!dialog.open && el('enabled').disabled, 'Identity change retained destructive dialog');
    confirm.click();
    check(writes.length === count, 'Stale confirmation wrote cache');
    await finished;
  };
  smoke.checkMediaCache = async () => {
    await openArea('settings', 'photos');
    await appApi.mediaCacheShown;
    const el = (name) => document.getElementById(`media-cache-${name}`);
    const dialog = document.getElementById('photo-keep-dialog');
    const cancel = document.getElementById('photo-keep-cancel');
    const confirm = document.getElementById('photo-keep-confirm');
    const check = (ok, reason) => {
      if (!ok) throw Error(reason);
    };
    const fixture = createCacheFixture(check);
    const edit = (value) => {
      el('limit').value = value;
      el('limit').dispatchEvent(new Event('change', { bubbles: true }));
    };
    const controls = { el, dialog, cancel, confirm, edit, check };
    const count = await checkCacheBudgets(fixture, controls);
    await checkCacheStaleResponses(fixture, controls);
    await checkCacheFailure(fixture, controls, count);
  };
})();
