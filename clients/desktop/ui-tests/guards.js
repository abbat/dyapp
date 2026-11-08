// Desktop UI smoke: remaining generation and Settings checks.
// An old inventory load must not replace the newer identity's Settings rows or error.
const checkStaleSettingsInventory = async () => {
  const readyState = current;
  const list = document.getElementById('settings-identities');
  const error = document.getElementById('settings-error');
  const switchAndLoad = async () => {
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    await openArea('settings', 'identities');
    const items = await invokeTest('list_identities');
    await showSettingsIdentities(() => Promise.resolve(items));
    return [...list.children].map((row) => row.dataset.peerId).join();
  };
  await openArea('settings', 'identities');
  let finishLoad;
  const oldLoad = showSettingsIdentities(
    () =>
      new Promise((resolve) => {
        finishLoad = resolve;
      }),
  );
  const rows = await switchAndLoad();
  if (!rows) throw new Error('new Settings inventory is empty');
  finishLoad([]);
  await oldLoad;
  if ([...list.children].map((row) => row.dataset.peerId).join() !== rows || !error.hidden) {
    throw new Error('old inventory load changed new settings');
  }

  let failLoad;
  const oldFailure = showSettingsIdentities(
    () =>
      new Promise((_, reject) => {
        failLoad = reject;
      }),
  );
  const newerRows = await switchAndLoad();
  failLoad(new Error('old inventory'));
  await oldFailure;
  if ([...list.children].map((row) => row.dataset.peerId).join() !== newerRows || !error.hidden)
    throw new Error('old inventory error changed new settings');
};
// A tray result from an old Settings view cannot replace the new controls.
const checkStaleTray = async () => {
  const readyState = current;
  const show = document.getElementById('tray-show');
  const close = document.getElementById('tray-close');
  const note = document.getElementById('tray-note');
  await openArea('settings', 'appearance');
  await window.trayShown;
  let finishOld;
  const oldTray = setTray(
    null,
    () =>
      new Promise((resolve) => {
        finishOld = resolve;
      }),
  );
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  await openArea('settings', 'appearance');
  await window.trayShown;
  await setTray(null, () =>
    Promise.resolve({
      available: true,
      show: true,
      closeToTray: true,
    }),
  );
  finishOld({ available: false, show: false, closeToTray: false });
  await oldTray;
  if (!show.checked || show.disabled || !close.checked || close.disabled || !note.hidden) {
    throw new Error('old tray result changed new Settings controls');
  }
  await setTray(null);
};
// Old removal preparation cannot open the dialog after another identity renders.
const checkStaleRemovePreparation = async () => {
  const readyState = current;
  const items = await invokeTest('list_identities');
  let finishList;
  const oldList = showRemove(
    () =>
      new Promise((resolve) => {
        finishList = resolve;
      }),
  );
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  finishList(items);
  await oldList;
  if (removeDialog.open || !removeDialog.hidden) {
    throw new Error('old removal inventory opened new dialog');
  }

  let finishProfile, startedProfile;
  const profileStarted = new Promise((resolve) => {
    startedProfile = resolve;
  });
  const oldProfile = showRemove((command) =>
    command === 'list_identities'
      ? Promise.resolve(items)
      : new Promise((resolve) => {
          finishProfile = resolve;
          startedProfile();
        }),
  );
  await profileStarted;
  if (!finishProfile) throw new Error('old removal profile load did not start');
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  finishProfile({ published: 1 });
  await oldProfile;
  if (removeDialog.open || !removeDialog.hidden) {
    throw new Error('old removal profile opened new dialog');
  }
  await showRemove((command) => Promise.resolve(command === 'list_identities' ? items : null));
  if (!removeDialog.open) throw new Error('current removal dialog did not open');
  const oldClose = closed(removeDialog);
  removeDialog.close();
  await showRemove((command) => Promise.resolve(command === 'list_identities' ? items : null));
  await oldClose;
  if (!removeDialog.open || removeDialog.hidden)
    throw Error('Old close hid a reopened removal dialog');
  const finalClose = closed(removeDialog);
  removeDialog.close();
  await finalClose;
};
// The close event, which the app handles first, comes on a later task in no fixed order with
// timers, so a check waits for the event itself.
const closed = (dialog) =>
  new Promise((resolve, reject) => {
    const finish = () => {
      clearTimeout(deadline);
      resolve();
    };
    const deadline = setTimeout(() => {
      dialog.removeEventListener('close', finish);
      reject(Error(`${dialog.id} close deadline`));
    }, 10000);
    dialog.addEventListener('close', finish, { once: true });
  });
// A revocation of another slot wipes nothing; the active slot's closes dialogs and says why.
const checkDeviceRevoked = async () => {
  await showRemove((command) => Promise.resolve(command === 'list_identities' ? [] : null));
  let removed = 0;
  const remove = async () => {
    removed += 1;
  };
  await window.deviceRevoked('another slot', remove);
  if (removed || !removeDialog.open) throw new Error('another slot was wiped');
  await window.deviceRevoked(current.slot, async () => {
    removed += 1;
    return { status: 'error' };
  });
  if (removed !== 1 || removeDialog.open || revokedDialog.open) {
    throw new Error('a failed wipe claimed success');
  }
  await window.deviceRevoked(current.slot, remove);
  if (removed !== 2 || !revokedDialog.open) throw new Error('a wipe did not say why');
  document.getElementById('revoked-ok').click();
  await closed(revokedDialog);
  if (revokedDialog.open || !revokedDialog.hidden) throw new Error('the notice stayed open');
};
// An old withdrawal cannot remove the new identity or switch its dialog to Offline.
const checkStaleWithdrawal = async () => {
  const readyState = current;
  let removed = 0;
  const remove = () => {
    removed += 1;
  };
  const switchAndSetDialog = async () => {
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    removeAsk.hidden = false;
    removeOffline.hidden = true;
    removeRetry.disabled = true;
    removeAnyway.disabled = true;
  };
  removePublished = true;
  let finishOld;
  const oldSuccess = withdrawThenRemove(
    () =>
      new Promise((resolve) => {
        finishOld = resolve;
      }),
    remove,
  );
  await switchAndSetDialog();
  finishOld();
  await oldSuccess;
  if (removed || removeAsk.hidden || !removeOffline.hidden) {
    throw new Error('old withdrawal success removed new identity');
  }

  let failOld;
  const oldFailure = withdrawThenRemove(
    () =>
      new Promise((_, reject) => {
        failOld = reject;
      }),
    remove,
  );
  await switchAndSetDialog();
  failOld(new Error('old withdrawal'));
  await oldFailure;
  if (
    removed ||
    removeAsk.hidden ||
    !removeOffline.hidden ||
    !removeRetry.disabled ||
    !removeAnyway.disabled
  ) {
    throw new Error('old withdrawal failure changed new dialog');
  }
  await withdrawThenRemove(() => Promise.resolve(), remove);
  if (removed !== 1) throw new Error('current withdrawal did not continue to removal');
  await withdrawThenRemove(() => Promise.reject(new Error('offline')), remove);
  if (!removeAsk.hidden || removeOffline.hidden || removeRetry.disabled || removeAnyway.disabled) {
    throw new Error('current withdrawal failure did not show Offline');
  }
  removePublished = false;
  removeAsk.hidden = false;
  removeOffline.hidden = true;
};
// An old launch connection cannot repaint the new shell or start another dial.
const checkStaleLaunchConnection = async () => {
  const readyState = current;
  const currentConnection = async () => {
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    await window.connectionShown;
    await connectAtLaunch((command) =>
      Promise.resolve(command === 'load_network' ? { name: 'new network' } : true),
    );
  };
  const checkCurrent = (message) => {
    if (
      connectionStatus.dataset.state !== 'connected' ||
      document.getElementById('connection-network').textContent !== 'new network'
    ) {
      throw new Error(message);
    }
  };
  let finishActive;
  let oldDials = 0;
  const oldActive = connectAtLaunch((command) => {
    if (command === 'load_network')
      return new Promise((resolve) => {
        finishActive = resolve;
      });
    oldDials += 1;
    return Promise.resolve(true);
  });
  await currentConnection();
  finishActive({ name: 'old network' });
  await oldActive;
  checkCurrent('old active network changed new connection');
  if (oldDials) throw new Error('old active network started a new dial');

  let finishDial;
  const oldDial = connectAtLaunch((command) =>
    command === 'load_network'
      ? Promise.resolve({ name: 'old network' })
      : new Promise((resolve) => {
          finishDial = resolve;
        }),
  );
  await new Promise((resolve) => {
    setTimeout(resolve, 0);
  });
  if (!finishDial) throw new Error('old connection dial did not start');
  await currentConnection();
  finishDial(false);
  await oldDial;
  checkCurrent('old dial changed new connection');
};
// The same place text on B must keep B's suggestion ID when A responds late.
const checkStalePlaceSuggestions = async () => {
  const readyState = current;
  const countryInput = document.createElement('input');
  const input = document.createElement('input');
  const list = document.createElement('datalist');
  countryInput.value = 'DE';
  let finishOld;
  let calls = 0;
  const picker = placePicker(
    countryInput,
    input,
    list,
    () => countryInput.value,
    () => {
      calls += 1;
      return calls === 1
        ? new Promise((resolve) => {
            finishOld = resolve;
          })
        : Promise.resolve([{ id: 2, name: 'Berlin' }]);
    },
  );
  input.value = 'Berlin';
  input.dispatchEvent(new Event('input'));
  const oldPlaces = window.placesShown;
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  input.value = 'Berlin';
  input.dispatchEvent(new Event('input'));
  await window.placesShown;
  if (picker.id() !== '2' || !input.validity.valid) {
    throw new Error('current place suggestion did not render');
  }
  finishOld([{ id: 1, name: 'Oldtown' }]);
  await oldPlaces;
  if (
    picker.id() !== '2' ||
    !input.validity.valid ||
    list.querySelector('option')?.value !== 'Berlin'
  ) {
    throw new Error('old place suggestion replaced new identity');
  }
};
// Removes the opened identity through both confirmations, then opens the one left.
const removeOpened = async (state, mode) => {
  const { strings } = window;
  if (state.status === 'ready') {
    await openArea('settings', 'identities');
    document.getElementById('settings-remove').click();
    await waitFor('remove');
    if (!document.getElementById('remove-action').disabled) {
      throw new Error('remove enabled before confirmation');
    }
    const cancelled = closed(removeDialog);
    document.getElementById('remove-cancel').click();
    await cancelled;
    if (
      document.getElementById('remove').open ||
      !document.getElementById('remove').hidden ||
      document.getElementById('settings').hidden
    ) {
      throw new Error('Cancel did not return to Settings');
    }
    document.getElementById('settings-remove').click();
  } else {
    document.getElementById('remove-identity').click();
  }
  await waitFor('remove');
  if (document.activeElement.id !== 'remove-cancel') {
    throw new Error('Cancel is not focused in the removal dialog');
  }
  await shot('remove');
  const action = document.getElementById('remove-action');
  if (
    document.getElementById('remove-peer').textContent !==
    (state.peerId || strings[`recovery.${state.reason}`])
  )
    throw new Error('wrong Peer ID');
  if (
    state.peerId &&
    document.getElementById('remove-copies').textContent !== strings['remove.lastCopy']
  )
    throw new Error('last copy not shown');
  // The loss warning shows only when this removal erases the last copy of a key.
  if (document.getElementById('remove-warning').hidden === Boolean(state.peerId)) {
    throw new Error('loss warning shown when no key is lost, or missing for the last copy');
  }
  if (!action.disabled) throw new Error('remove enabled before confirmation');
  document.getElementById('remove-confirm').click();
  if (action.disabled) throw new Error('confirmation did not enable remove');
  action.click();
  await waitFor('picker');
  await pickReady(mode);
  await checkSettingsInventory(document.getElementById('peer-id').textContent, mode);
};
// Nothing listens on port 1, so that row is refused at once; `seed` has no port.
const testSeeds = '127.0.0.1:1\nseed';
const submitCheck = async (rows) => {
  const { strings } = window;
  if (document.getElementById('wizard-network').hidden) await openArea('network-area');
  document.getElementById('seeds').value = rows;
  document.getElementById('check-connection').click();
  await window.connectionChecked;
  const status = document.getElementById('network-status').textContent;
  return status === strings['network.empty']
    ? 'empty'
    : [...document.querySelectorAll('#seed-rows li')].map((row) => row.dataset.status);
};
const connection = () => document.getElementById('connection-status').dataset.state;
// A failed check shows the error with every row in its details and keeps the shell status.
const expectFailed = (rows, state) => {
  const details = document.getElementById('network-error-details').textContent;
  if (document.getElementById('network-error').hidden || rows.some((row) => !details.includes(row)))
    throw new Error(`error details ${details}`);
  if (connection() !== state) throw new Error(`failed check left ${connection()}`);
};
// A node that answers activates the list at once and the shell shows Connected.
const connectNetwork = async (seed) => {
  const rows = await submitCheck(seed);
  if (rows.join() !== 'connected') throw new Error(`rows ${rows}`);
  if (document.getElementById('network-status').textContent !== window.strings['network.active'])
    throw new Error('connection not activated');
  if (connection() !== 'connected') throw new Error(`shell shows ${connection()}`);
  if (!document.getElementById('identity').hidden) await shot('network-connected');
};
// An empty list is refused, then a refused and a malformed row activate nothing.
const checkNetwork = async () => {
  // The launch connect settles first, so it cannot overwrite the status this check sets.
  await window.connectionShown;
  const state = connection();
  if ((await submitCheck('')) !== 'empty') throw new Error('empty list not reported');
  await shot('network-empty');
  const rows = await submitCheck(testSeeds);
  if (rows.join() !== 'unreachable,invalidAddress') throw new Error(`rows ${rows}`);
  await shot('network-failure');
  if (
    document.getElementById('network-status').textContent !==
    window.strings['network.invalidAddress']
  )
    throw new Error('row problem not summarized');
  expectFailed(testSeeds.split('\n'), state);
};
// A seed that accepts and never answers keeps the check running until Cancel stops it.
const cancelNetwork = async (seed) => {
  // The launch connect settles first, so it cannot overwrite the status this check sets.
  await window.connectionShown;
  const state = connection();
  if (document.getElementById('wizard-network').hidden) await openArea('network-area');
  document.getElementById('seeds').value = seed;
  document.getElementById('check-connection').click();
  const cancel = document.getElementById('cancel-check');
  if (cancel.hidden) throw new Error('Cancel not offered while checking');
  // Repeated, because a Cancel that lands before the check starts stops nothing.
  let settled = false;
  window.connectionChecked.finally(() => {
    settled = true;
  });
  while (!settled) {
    cancel.click();
    await new Promise((resolve) => {
      setTimeout(resolve, 200);
    });
  }
  if (document.getElementById('network-status').textContent !== window.strings['network.cancelled'])
    throw new Error('check not cancelled');
  if (!cancel.hidden) throw new Error('cancelled check left Cancel shown');
  if (connection() !== state) throw new Error(`cancelled check left ${connection()}`);
};
// The live node under a wrong pin or another network name, then down: nothing activates.
const checkWrongNetwork = async (seed) => {
  // The launch connect settles first, so it cannot overwrite the status this check sets.
  await window.connectionShown;
  const state = connection();
  const name = document.getElementById('network-name');
  const otherPeer = '12D3KooW9tJMax94Lrqw7Y5Qw36viGQAS2gTEPQ5Wg1vTk7xPfQs';
  const expect = async (rows, statuses, summary) => {
    const shown = await submitCheck(rows);
    if (
      shown.join() !== statuses ||
      document.getElementById('network-status').textContent !== window.strings[summary]
    )
      throw new Error(`${rows}: rows ${shown}`);
    expectFailed([rows], state);
  };
  await expect(`${seed}/${otherPeer}`, 'wrongPeer', 'network.wrong');
  name.value = 'city';
  await expect(seed, 'wrongNetwork', 'network.wrong');
  // The saved draft goes back to the node's network, so the next launch retries it.
  name.value = '';
  await expect('127.0.0.1:1', 'unreachable', 'network.unreachable');
};
// Leaving the Network area and coming back keeps the draft and the checked row status.
const checkNetworkInSettings = async () => {
  const seeds = document.getElementById('seeds');
  await openArea('network-area');
  seeds.value = testSeeds;
  seeds.dispatchEvent(new Event('input', { bubbles: true }));
  await openArea('profile-area');
  await openArea('network-area');
  if (seeds.value !== testSeeds) throw new Error('switching areas lost the network draft');
  await checkNetwork();
  await openArea('profile-area');
  await openArea('network-area');
  if (
    document.getElementById('network-status').textContent !==
      window.strings['network.invalidAddress'] ||
    [...document.querySelectorAll('#seed-rows li')].map((row) => row.dataset.status).join() !==
      'unreachable,invalidAddress'
  ) {
    throw new Error('switching areas lost the failed row status');
  }
  await openArea('profile-area');
};
const checkSettingsLanguage = async () => {
  const strings = window.strings;
  await openArea('settings');
  const expectedTitle = window.pseudoLocale
    ? '[Séttíngs····]'
    : window.locale === 'ru'
      ? 'Настройки'
      : 'Settings';
  if (document.querySelector('#settings h1').textContent !== expectedTitle) {
    throw new Error('Settings title uses the wrong language');
  }
  for (const [id, key] of [
    ['settings-create', 'identity.add'],
    ['settings-switch', 'identity.switch'],
    ['settings-remove', 'identity.remove'],
    ['node-save', 'node.save'],
  ]) {
    if (document.getElementById(id).textContent !== strings[key]) {
      throw new Error(`Settings control ${id} is not localized`);
    }
  }
  await checkNetwork();
  if (document.getElementById('network-status').textContent !== strings['network.invalidAddress']) {
    throw new Error('Network status is not localized');
  }
  await openArea('profile-area');
};
// Settings → Privacy starts at the defaults, saves each change, and Appearance switches the theme.
const privacyIds = [
  'send-views',
  'send-read-receipts',
  'send-typing',
  'presence-matches',
  'message-notifications',
];
const privacyState = () => privacyIds.map((id) => document.getElementById(id).checked).join();
const expectPrivacy = async (expected) => {
  await openArea('settings', 'privacy');
  await window.privacyShown;
  if (privacyState() !== expected || shown('privacy-error')) {
    throw new Error(`privacy settings ${privacyState()}, expected ${expected}`);
  }
};
const checkPrivacy = async (restarted) => {
  await openArea('settings', 'privacy');
  await window.privacyShown;
  await window.blockedShown;
  const expected = restarted ? 'true,true,false,false,false' : 'false,false,true,true,true';
  if (
    privacyState() !== expected ||
    shown('privacy-error') ||
    document.getElementById('presence-nobody').checked !== restarted
  ) {
    throw new Error(`privacy settings ${privacyState()}, expected ${expected}`);
  }
  if (!shown('blocked-none') || document.getElementById('blocked-list').children.length) {
    throw new Error('blocked list not empty');
  }
  const noticeChoice = document.getElementById('notice-content');
  if (
    noticeChoice.value !== (restarted ? 'text' : 'fixed') ||
    noticeChoice.disabled !== restarted
  ) {
    throw new Error(`notice content ${noticeChoice.value}, disabled ${noticeChoice.disabled}`);
  }
  if (restarted) return;
  openSettings('notifications');
  noticeChoice.value = 'text';
  noticeChoice.dispatchEvent(new Event('change', { bubbles: true }));
  await window.privacySaved;
  for (const id of [
    'send-views',
    'send-read-receipts',
    'send-typing',
    'presence-nobody',
    'message-notifications',
  ]) {
    openSettings(id === 'message-notifications' ? 'notifications' : 'privacy');
    document.getElementById(id).click();
    await window.privacySaved;
  }
  if (shown('privacy-error')) throw new Error('privacy settings not saved');
  const root = document.documentElement;
  openSettings('appearance');
  document.getElementById('theme-dark').click();
  if (root.dataset.theme !== 'dark') throw new Error('dark theme not applied');
  document.getElementById('theme-system').click();
  if ('theme' in root.dataset) throw new Error('system theme not restored');
  const blockedPeer = 'f'.repeat(64);
  const blocked = await invokeTest('block_peer', { peerId: blockedPeer });
  if (blocked.revocation !== 'notNeeded')
    throw new Error(`unexpected block state: ${blocked.revocation}`);
  await openArea('profile-area');
  await openArea('settings', 'blocked');
  await window.blockedShown;
  const retry = document.getElementById('retry-revocation-1');
  if (
    !retry ||
    !document.getElementById('unblock-1') ||
    document.getElementById('blocked-revoke-status').textContent
  ) {
    throw new Error('blocked peer has no retry and unblock actions');
  }
  retry.click();
  await window.revocationRetried;
  if (
    document.getElementById('blocked-revoke-status').textContent !==
    window.strings['settings.blocked.notNeeded']
  ) {
    throw new Error('retry did not report token revocation state');
  }
  document.getElementById('unblock-1').click();
  await window.blockedShown;
  if (!shown('blocked-none') || document.getElementById('blocked-list').children.length) {
    throw new Error('unblock did not clear the blocked list');
  }
};
// Devices with stubbed commands: rows, this computer first, a failed and a good disconnect.
const checkDevices = async () => {
  const card = document.getElementById('devices');
  const dialog = document.getElementById('disconnect');
  const rows = [
    { key: [1, 2, 255], this: false },
    { key: [10, 11, 12], this: true },
  ];
  const stale = showDevices(
    () =>
      new Promise((resolve) => {
        setTimeout(() => resolve([rows[0]]));
      }),
  );
  const list = (command) => {
    if (command !== 'list_devices') throw new Error(`unexpected ${command}`);
    return Promise.resolve(structuredClone(rows));
  };
  await showDevices(() => Promise.resolve([]));
  if (!card.hidden) throw new Error('an unpublished identity shows Devices');
  await openArea('settings', 'devices');
  await showDevices(list);
  await stale;
  const shownKeys = [...document.querySelectorAll('#devices-list code')].map(
    (key) => key.textContent,
  );
  if (
    card.hidden ||
    shownKeys.join() !== '0a0b0c,0102ff' ||
    document.querySelectorAll('#devices-list button').length !== 1
  ) {
    throw new Error(`wrong device rows: ${shownKeys}`);
  }
  document.getElementById('device-disconnect-1').click();
  if (
    !dialog.open ||
    document.activeElement.id !== 'disconnect-cancel' ||
    document.getElementById('disconnect-device').textContent !== '0102ff'
  ) {
    throw new Error('disconnect dialog did not open on the row');
  }
  await disconnectNow(() => Promise.reject(new Error('notConnected')));
  if (
    document.getElementById('disconnect-problem').hidden ||
    document.getElementById('disconnect-retry').hidden ||
    !document.getElementById('disconnect-action').hidden
  ) {
    throw new Error('failed disconnect does not offer Retry');
  }
  const sent = [];
  await disconnectNow((command, args) => {
    sent.push([command, args.deviceKey.join()]);
    return Promise.resolve();
  });
  if (dialog.open || sent.join() !== 'revoke_device,1,2,255') {
    throw new Error(`disconnect sent ${sent}`);
  }
  await closed(dialog);
  if (document.getElementById('devices-status').textContent !== window.strings['devices.gone']) {
    throw new Error('disconnect did not report its result');
  }
};
// A pasted code with stubbed commands: nothing is sent before the confirmation.
const checkLinkCode = async () => {
  const code = document.getElementById('device-code-dialog');
  const confirm = document.getElementById('device-confirm-dialog');
  const problem = document.getElementById('device-code-problem');
  const invalid = document.getElementById('device-code-invalid');
  const refuse = (reason) => () => Promise.reject(reason);
  document.getElementById('device-link').click();
  if (!code.open || document.activeElement.id !== 'device-code') {
    throw new Error('link code dialog did not open');
  }
  codeField.value = 'dyapp-link:1:code';
  await checkCode(refuse('bad'));
  if (invalid.hidden || !code.open) throw new Error('a bad code gave no message');
  await checkCode(refuse('expired'));
  if (!invalid.hidden || problem.textContent !== window.strings['devices.code.expired']) {
    throw new Error('an expired code gave no message');
  }
  const sent = [];
  await checkCode((command) => {
    sent.push(command);
    return Promise.resolve();
  });
  if (
    code.open ||
    !confirm.open ||
    document.activeElement.id !== 'device-confirm-cancel' ||
    sent.join() !== 'check_link_code'
  ) {
    throw new Error(`code did not reach the confirmation: ${sent}`);
  }
  await sendLink(refuse('notConnected'));
  if (
    confirm.open ||
    !code.open ||
    problem.textContent !== window.strings['devices.code.offline']
  ) {
    throw new Error('a failed send did not return to the code');
  }
  code.close();
  await sendLink((command, args) => {
    sent.push(command, args.code);
    return Promise.resolve();
  });
  if (
    sent.join() !== 'check_link_code,send_link,dyapp-link:1:code' ||
    document.getElementById('devices-status').textContent !== window.strings['devices.sent']
  ) {
    throw new Error(`send sent ${sent}`);
  }
};
// The link page against a stub backend; two clients linking are the smoke's.
const checkLinkScreen = async () => {
  const seen = (...ids) => ids.every((id) => document.getElementById(id).checkVisibility());
  const status = () => document.getElementById('link-status').textContent;
  let polls = [];
  const calls = [];
  const call = (command) => {
    calls.push(command);
    if (command === 'start_link') {
      return Promise.resolve({
        code: 'dyapp-link:1:test',
        qr: { size: 21, modules: Array(441).fill(true) },
      });
    }
    if (command === 'link_pending') return Promise.resolve(polls.shift());
    if (command !== 'poll_link') return Promise.resolve(null);
    const next = polls.shift();
    return next instanceof Error ? Promise.reject(next.message) : Promise.resolve(next);
  };
  polls = [null];
  await startLink(call);
  if (
    !seen('link', 'link-qr', 'link-code', 'copy-link-code', 'link-cancel') ||
    seen('link-new') ||
    document.getElementById('link-code').textContent !== 'dyapp-link:1:test' ||
    status() !== window.strings['link.waiting']
  )
    throw new Error('link page does not show a code');
  await shot('link-show');
  clearTimeout(linkTimer);
  polls = [new Error('notConnected')];
  await pollLink(linkRun, call);
  if (status() !== window.strings['link.offline'] || !seen('link-qr', 'link-cancel')) {
    throw new Error('link page does not say that no node answers');
  }
  clearTimeout(linkTimer);
  polls = [new Error('expired')];
  await pollLink(linkRun, call);
  if (
    status() !== window.strings['link.expired'] ||
    !seen('link-new', 'link-cancel') ||
    seen('link-qr')
  ) {
    throw new Error('an expired link keeps its code or offers no new one');
  }
  await shot('link-expired');
  polls = [null];
  await startLink(call);
  if (seen('link-new') || !seen('link-qr'))
    throw new Error('a new code does not replace the expired one');

  const stale = linkRun;
  await stopLink(call);
  if (calls.at(-1) !== 'cancel_link') throw new Error('Cancel leaves the link waiting');
  polls = [null];
  await pollLink(stale, call);
  if (calls.at(-1) !== 'poll_link' || status() !== window.strings['link.waiting']) {
    throw new Error('a cancelled poll changed the page');
  }
  // A record that won the race with Cancel still ends on the identity it opened.
  const state = { status: 'ready', peerId: 'ab'.repeat(32), mode: 'file' };
  polls = [state, true];
  await pollLink(stale, call);
  if (
    !seen('link-done', 'link-peer', 'link-pending', 'link-open') ||
    seen('link-wait') ||
    !calls.includes('receive_mail') ||
    document.activeElement.id !== 'link-open' ||
    !document.getElementById('link-peer').textContent.includes(window.peerName(state.peerId))
  ) {
    throw new Error('a received link does not name its identity');
  }
  await shot('link-done-pending');
  polls = [state, false];
  await pollLink(linkRun, call);
  if (seen('link-pending') || !seen('link-done'))
    throw new Error('a finished link says it is pending');

  wizardStep('link');
  show(wizardNetwork, 'status.ready');
  if (
    !seen('link-network-cancel', 'wizard-link-step') ||
    ['skip-network', 'wizard-peer', 'wizard-step'].some((id) => seen(id))
  ) {
    throw new Error('wrong link network step');
  }
  document.getElementById('link-network-cancel').click();
  if (
    !identity.checkVisibility() ||
    seen('link-network-cancel') ||
    !document.getElementById('skip-network').hidden
  )
    throw new Error('link network step stayed');
  await openArea('settings', 'identities');
  if (
    !seen('device-join') ||
    document.getElementById('devices').contains(document.getElementById('device-join'))
  ) {
    throw new Error('an unpublished identity cannot link this computer');
  }
};
// Every chat of a linked device says where its history starts; no chat of a first device does.
const checkLinkedNotice = async () => {
  const notice = document.getElementById('chat-linked-notice');
  const peer = 'cd'.repeat(32);
  const call = (linked) => (command) => {
    if (command === 'chat_history') return Promise.resolve({ state: 'open', messages: [] });
    if (command === 'linked_device') return Promise.resolve(linked);
    return Promise.resolve(false);
  };
  await openArea('chats-area');
  await openChat(peer, call(true));
  if (notice.hidden) throw new Error('a chat on a linked device has no history notice');
  await shot('chat-linked');
  await openChat(peer, call(false));
  if (!notice.hidden) throw new Error('a chat on a first device says it is linked');
  closeChat();
};
// Linking two real clients: each half is a step of its own process, the node carries the record.
const linkUntil = async (check, what, ms = 60000) => {
  const end = Date.now() + ms;
  for (;;) {
    const value = await check();
    if (value) return value;
    if (Date.now() > end) throw new Error(`no ${what}`);
    await new Promise((resolve) => {
      setTimeout(resolve, 500);
    });
  }
};
const liveDevices = (count) =>
  linkUntil(
    async () => (await invokeTest('list_devices', { live: true })).length === count,
    `${count} devices`,
  );
const linkWaitStep = async (mode, seed) => {
  document.getElementById(`${mode}-choice`).click();
  document.getElementById('link-identity').click();
  await waitFor('wizard-network');
  const rows = await submitCheck(seed);
  if (rows.join() !== 'connected') throw new Error(`link network rows ${rows}`);
  const code = await linkUntil(() => document.getElementById('link-code').textContent, 'link code');
  await invokeTest('ui_test_link_code', { code });
  await linkUntil(() => shown('link-done'), 'linked identity', 120000);
  if (
    shown('link-pending') ||
    !document.getElementById('link-peer').textContent.includes(window.peerName(linkJoined.peerId))
  ) {
    throw new Error('the link did not finish');
  }
  await shot('link-joined');
  document.getElementById('link-open').click();
  await waitFor('identity');
  await liveDevices(2);
  return linkJoined.peerId;
};
const linkSendStep = async (code) => {
  await openArea('settings', 'devices');
  await window.devicesShown;
  document.getElementById('device-link').click();
  document.getElementById('device-code').value = code;
  document.getElementById('device-code-next').click();
  await window.codeChecked;
  document.getElementById('device-confirm-action').click();
  await window.linkSent;
  if (document.getElementById('devices-status').textContent !== window.strings['devices.sent']) {
    throw new Error('the link was not sent');
  }
};
const linkDisconnectStep = async () => {
  await openArea('settings', 'devices');
  await liveDevices(2);
  await showDevices();
  document.getElementById('device-disconnect-1').click();
  document.getElementById('disconnect-action').click();
  const dialog = document.getElementById('disconnect');
  await window.disconnectDone;
  if (dialog.open) {
    throw new Error(
      `disconnect failed: ${document.getElementById('disconnect-problem').textContent}`,
    );
  }
  await closed(dialog);
  await window.devicesShown;
  if (document.getElementById('devices-status').textContent !== window.strings['devices.gone']) {
    throw new Error('the device was not disconnected');
  }
  await liveDevices(1);
};
// The catch-up of the launch may already have wiped it, so a failed pass here means nothing.
const linkWipedStep = async () => {
  await linkUntil(async () => {
    await invokeTest('receive_mail').catch(() => {});
    return shown('revoked');
  }, 'wipe');
  document.getElementById('revoked-ok').click();
  await waitFor('setup');
};
// Returns the peer id the smoke compares, empty for a client that lost its identity.
const linkStep = async (step, mode, state) => {
  const value = step.slice(step.indexOf(':') + 1);
  if (step.startsWith('link-wait:')) return linkWaitStep(mode, value);
  if (step === 'link-wiped') {
    await linkWipedStep();
    return '';
  }
  await (step === 'link-disconnect' ? linkDisconnectStep() : linkSendStep(value));
  return state.peerId;
};
globalThis.uiSmokeGuards = Object.freeze({
  checkLinkScreen,
  linkStep,
  checkLinkedNotice,
  checkStaleSettingsInventory,
  checkStaleNode: window.uiSmokeNodeGuard,
  checkStaleTray,
  checkStaleRemovePreparation,
  checkStaleWithdrawal,
  checkDeviceRevoked,
  checkDevices,
  checkLinkCode,
  checkStaleLaunchConnection,
  checkStaleNetworkForm: window.uiSmokeNetworkGuards.form,
  checkStaleNetworkCheck: window.uiSmokeNetworkGuards.requests,
  checkStaleCopy: window.uiSmokeCopyGuard,
  checkStalePlaceSuggestions,
  removeOpened,
  testSeeds,
  connection,
  connectNetwork,
  checkNetwork,
  cancelNetwork,
  checkWrongNetwork,
  checkNetworkInSettings,
  checkSettingsLanguage,
  expectPrivacy,
  checkPrivacy,
});
