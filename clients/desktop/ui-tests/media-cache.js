window.checkMediaCache = async () => {
  await openArea('settings', 'photos');
  await window.mediaCacheShown;
  const el = (name) => document.getElementById(`media-cache-${name}`);
  const dialog = document.getElementById('photo-keep-dialog');
  const cancel = document.getElementById('photo-keep-cancel');
  const confirm = document.getElementById('photo-keep-confirm');
  const check = (ok, reason) => {
    if (!ok) throw Error(reason);
  };
  let config = { enabled: true, limitBytes: 393216123, usedBytes: 16 };
  const writes = [];
  const call = async (command, args) => {
    check(args.identityId === current.peerId, 'Cache IPC lacks viewed identity');
    if (command === 'set_media_cache') {
      writes.push(args);
      config = { ...config, enabled: args.enabled, limitBytes: args.limitBytes };
    } else if (command === 'clear_media_cache') {
      writes.push(args);
      config = { ...config, usedBytes: 0 };
    } else check(command === 'media_cache', 'Unexpected cache IPC');
    return { ...config };
  };
  const edit = (value) => {
    el('limit').value = value;
    el('limit').dispatchEvent(new Event('change', { bubbles: true }));
  };
  await window.showMediaCache(call);
  check(Number(el('limit').value) * 1048576 === 393216123, 'Byte budget lost precision');
  edit('512');
  await window.mediaCacheSaved;
  check(!dialog.open && config.limitBytes === 512 * 1048576, 'Larger numeric budget not saved');
  edit('384');
  check(dialog.open && writes.length === 1, 'Lower budget did not ask');
  cancel.click();
  check(
    config.limitBytes === 512 * 1048576 && el('limit').value === '512',
    'Cancel changed budget',
  );
  edit('384');
  confirm.click();
  await window.mediaCacheSaved;
  check(config.limitBytes === 384 * 1048576, 'Confirmed lower budget not saved');
  el('enabled').click();
  check(dialog.open && config.enabled, 'Disabling cache did not ask');
  confirm.click();
  await window.mediaCacheSaved;
  check(
    !config.enabled && el('limit').disabled && config.limitBytes === 384 * 1048576,
    'Disabling lost quota',
  );
  el('enabled').click();
  await window.mediaCacheSaved;
  check(config.enabled && !el('limit').disabled, 'Enabling did not retain numeric quota');
  el('clear').click();
  check(dialog.open && config.usedBytes === 16, 'Clear did not ask');
  cancel.click();
  el('clear').click();
  confirm.click();
  await window.mediaCacheSaved;
  check(config.usedBytes === 0 && el('clear').disabled, 'Clear did not refresh measured usage');
  const count = writes.length;
  for (const value of ['', '0', '10241']) edit(value);
  check(writes.length === count && !dialog.open, 'Invalid budget reached IPC');
  let finishOld;
  const old = window.showMediaCache(
    () =>
      new Promise((resolve) => {
        finishOld = resolve;
      }),
  );
  await window.showMediaCache(call);
  finishOld({ enabled: false, limitBytes: 1048576, usedBytes: 99 });
  await old;
  check(el('enabled').checked && el('limit').value === '384', 'Old read repainted new cache');
  let finishWrite;
  await window.showMediaCache((command) =>
    command === 'media_cache'
      ? Promise.resolve(config)
      : new Promise((resolve) => {
          finishWrite = resolve;
        }),
  );
  edit('1024');
  const oldWrite = window.mediaCacheSaved;
  check(el('enabled').disabled && el('clear').disabled, 'Busy cache accepted more actions');
  await window.showMediaCache(call);
  finishWrite({ enabled: false, limitBytes: 1048576, usedBytes: 0 });
  await oldWrite;
  check(el('enabled').checked && el('limit').value === '384', 'Old write repainted new cache');
  await window.showMediaCache((command) =>
    command === 'media_cache' ? Promise.resolve(config) : Promise.reject(Error('uncertain write')),
  );
  edit('1024');
  await window.mediaCacheSaved;
  check(
    el('enabled').disabled && el('limit').disabled && !el('refresh').disabled,
    'Uncertain write did not require fresh read',
  );
  await window.showMediaCache(call);
  edit('256');
  document.dispatchEvent(new Event('dyapp:identity-change'));
  check(!dialog.open && el('enabled').disabled, 'Identity change retained destructive dialog');
  confirm.click();
  check(writes.length === count, 'Stale confirmation wrote cache');
};
