// Desktop UI smoke: remaining generation and Settings checks.
// An old inventory load must not replace the newer identity's Settings rows or error.
const checkStaleSettingsInventory = async () => {
  const readyState = appBindings.current;
  const list = document.getElementById('settings-identities');
  const error = document.getElementById('settings-error');
  const switchAndLoad = async () => {
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    await openArea('settings', 'identities');
    const items = await invokeTest('list_identities');
    await appBindings.showSettingsIdentities(() => Promise.resolve(items));
    return [...list.children].map((row) => row.dataset.peerId).join();
  };
  await openArea('settings', 'identities');
  let finishLoad;
  const oldLoad = appBindings.showSettingsIdentities(
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
  const oldFailure = appBindings.showSettingsIdentities(
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
  const readyState = appBindings.current;
  const show = document.getElementById('tray-show');
  const close = document.getElementById('tray-close');
  const note = document.getElementById('tray-note');
  await openArea('settings', 'appearance');
  await appApi.trayShown;
  let finishOld;
  const oldTray = appBindings.setTray(
    null,
    () =>
      new Promise((resolve) => {
        finishOld = resolve;
      }),
  );
  await appBindings.act(Promise.resolve({ status: 'picker' }));
  await appBindings.act(Promise.resolve(readyState));
  await openArea('settings', 'appearance');
  await appApi.trayShown;
  await appBindings.setTray(null, () =>
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
  await appBindings.setTray(null);
};
// Old removal preparation cannot open the dialog after another identity renders.
const checkStaleRemovePreparation = async () => {
  const readyState = appBindings.current;
  const items = await invokeTest('list_identities');
  let finishList;
  const oldList = appBindings.showRemove(
    () =>
      new Promise((resolve) => {
        finishList = resolve;
      }),
  );
  await appBindings.act(Promise.resolve({ status: 'picker' }));
  await appBindings.act(Promise.resolve(readyState));
  finishList(items);
  await oldList;
  if (appBindings.removeDialog.open || !appBindings.removeDialog.hidden) {
    throw new Error('old removal inventory opened new dialog');
  }

  let finishProfile, startedProfile;
  const profileStarted = new Promise((resolve) => {
    startedProfile = resolve;
  });
  const oldProfile = appBindings.showRemove((command) =>
    command === 'list_identities'
      ? Promise.resolve(items)
      : new Promise((resolve) => {
          finishProfile = resolve;
          startedProfile();
        }),
  );
  await profileStarted;
  if (!finishProfile) throw new Error('old removal profile load did not start');
  await appBindings.act(Promise.resolve({ status: 'picker' }));
  await appBindings.act(Promise.resolve(readyState));
  finishProfile({ published: 1 });
  await oldProfile;
  if (appBindings.removeDialog.open || !appBindings.removeDialog.hidden) {
    throw new Error('old removal profile opened new dialog');
  }
  await appBindings.showRemove((command) =>
    Promise.resolve(command === 'list_identities' ? items : null),
  );
  if (!appBindings.removeDialog.open) throw new Error('current removal dialog did not open');
  const oldClose = closed(appBindings.removeDialog);
  appBindings.removeDialog.close();
  await appBindings.showRemove((command) =>
    Promise.resolve(command === 'list_identities' ? items : null),
  );
  await oldClose;
  if (!appBindings.removeDialog.open || appBindings.removeDialog.hidden)
    throw Error('Old close hid a reopened removal dialog');
  const finalClose = closed(appBindings.removeDialog);
  appBindings.removeDialog.close();
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
  await appBindings.showRemove((command) =>
    Promise.resolve(command === 'list_identities' ? [] : null),
  );
  let removed = 0;
  const remove = async () => {
    removed += 1;
  };
  await appApi.deviceRevoked('another slot', remove);
  if (removed || !appBindings.removeDialog.open) throw new Error('another slot was wiped');
  await appApi.deviceRevoked(appBindings.current.slot, async () => {
    removed += 1;
    return { status: 'error' };
  });
  if (removed !== 1 || appBindings.removeDialog.open || appBindings.revokedDialog.open) {
    throw new Error('a failed wipe claimed success');
  }
  await appApi.deviceRevoked(appBindings.current.slot, remove);
  if (removed !== 2 || !appBindings.revokedDialog.open) throw new Error('a wipe did not say why');
  document.getElementById('revoked-ok').click();
  await closed(appBindings.revokedDialog);
  if (appBindings.revokedDialog.open || !appBindings.revokedDialog.hidden)
    throw new Error('the notice stayed open');
};
// An old withdrawal cannot remove the new identity or switch its dialog to Offline.
const checkStaleWithdrawal = smoke.uiSmokeLifecycleGuards.withdrawal;
// An old launch connection cannot repaint the new shell or start another dial.
const checkStaleLaunchConnection = async () => {
  const readyState = appBindings.current;
  const currentConnection = async () => {
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    await appApi.connectionShown;
    await appBindings.connectAtLaunch((command) =>
      Promise.resolve(command === 'load_network' ? { name: 'new network' } : true),
    );
  };
  const checkCurrent = (message) => {
    if (
      appBindings.connectionStatus.dataset.state !== 'connected' ||
      document.getElementById('connection-network').textContent !== 'new network'
    ) {
      throw new Error(message);
    }
  };
  let finishActive;
  let oldDials = 0;
  const oldActive = appBindings.connectAtLaunch((command) => {
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
  const oldDial = appBindings.connectAtLaunch((command) =>
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
  const readyState = appBindings.current;
  const countryInput = document.createElement('input');
  const input = document.createElement('input');
  const list = document.createElement('datalist');
  countryInput.value = 'DE';
  let finishOld;
  let calls = 0;
  const picker = appBindings.placePicker(
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
  const oldPlaces = appApi.placesShown;
  await appBindings.act(Promise.resolve({ status: 'picker' }));
  await appBindings.act(Promise.resolve(readyState));
  input.value = 'Berlin';
  input.dispatchEvent(new Event('input'));
  await appApi.placesShown;
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
  const { strings } = appApi;
  if (state.status === 'ready') {
    await openArea('settings', 'identities');
    document.getElementById('settings-remove').click();
    await waitFor('remove');
    if (!document.getElementById('remove-action').disabled) {
      throw new Error('remove enabled before confirmation');
    }
    const cancelled = closed(appBindings.removeDialog);
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
  await smoke.shot('remove');
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
  const { strings } = appApi;
  if (document.getElementById('wizard-network').hidden) await openArea('network-area');
  document.getElementById('seeds').value = rows;
  document.getElementById('check-connection').click();
  await appApi.connectionChecked;
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
  if (document.getElementById('network-status').textContent !== appApi.strings['network.active'])
    throw new Error('connection not activated');
  if (connection() !== 'connected') throw new Error(`shell shows ${connection()}`);
  if (!document.getElementById('identity').hidden) await smoke.shot('network-connected');
};
// An empty list is refused, then a refused and a malformed row activate nothing.
const checkNetwork = async () => {
  // The launch connect settles first, so it cannot overwrite the status this check sets.
  await appApi.connectionShown;
  const state = connection();
  if ((await submitCheck('')) !== 'empty') throw new Error('empty list not reported');
  await smoke.shot('network-empty');
  const rows = await submitCheck(testSeeds);
  if (rows.join() !== 'unreachable,invalidAddress') throw new Error(`rows ${rows}`);
  await smoke.shot('network-failure');
  if (
    document.getElementById('network-status').textContent !==
    appApi.strings['network.invalidAddress']
  )
    throw new Error('row problem not summarized');
  expectFailed(testSeeds.split('\n'), state);
};
// A seed that accepts and never answers keeps the check running until Cancel stops it.
const cancelNetwork = async (seed) => {
  // The launch connect settles first, so it cannot overwrite the status this check sets.
  await appApi.connectionShown;
  const state = connection();
  if (document.getElementById('wizard-network').hidden) await openArea('network-area');
  document.getElementById('seeds').value = seed;
  document.getElementById('check-connection').click();
  const cancel = document.getElementById('cancel-check');
  if (cancel.hidden) throw new Error('Cancel not offered while checking');
  // Repeated, because a Cancel that lands before the check starts stops nothing.
  let settled = false;
  appApi.connectionChecked.finally(() => {
    settled = true;
  });
  while (!settled) {
    cancel.click();
    await new Promise((resolve) => {
      setTimeout(resolve, 200);
    });
  }
  if (document.getElementById('network-status').textContent !== appApi.strings['network.cancelled'])
    throw new Error('check not cancelled');
  if (!cancel.hidden) throw new Error('cancelled check left Cancel shown');
  if (connection() !== state) throw new Error(`cancelled check left ${connection()}`);
};
// The live node under a wrong pin or another network name, then down: nothing activates.
const checkWrongNetwork = async (seed) => {
  // The launch connect settles first, so it cannot overwrite the status this check sets.
  await appApi.connectionShown;
  const state = connection();
  const name = document.getElementById('network-name');
  const otherPeer = '12D3KooW9tJMax94Lrqw7Y5Qw36viGQAS2gTEPQ5Wg1vTk7xPfQs';
  const expect = async (rows, statuses, summary) => {
    const shown = await submitCheck(rows);
    if (
      shown.join() !== statuses ||
      document.getElementById('network-status').textContent !== appApi.strings[summary]
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
      appApi.strings['network.invalidAddress'] ||
    [...document.querySelectorAll('#seed-rows li')].map((row) => row.dataset.status).join() !==
      'unreachable,invalidAddress'
  ) {
    throw new Error('switching areas lost the failed row status');
  }
  await openArea('profile-area');
};
const checkSettingsLanguage = async () => {
  const strings = appApi.strings;
  await openArea('settings');
  const expectedTitle = appApi.pseudoLocale
    ? '[Séttíngs····]'
    : appApi.locale === 'ru'
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
  await appApi.privacyShown;
  if (privacyState() !== expected || shown('privacy-error')) {
    throw new Error(`privacy settings ${privacyState()}, expected ${expected}`);
  }
};
const checkPrivacy = smoke.uiSmokeSettingsGuards.privacy;
const checkDevices = smoke.uiSmokeSettingsGuards.devices;
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
  appBindings.codeField.value = 'dyapp-link:1:code';
  await appBindings.checkCode(refuse('bad'));
  if (invalid.hidden || !code.open) throw new Error('a bad code gave no message');
  await appBindings.checkCode(refuse('expired'));
  if (!invalid.hidden || problem.textContent !== appApi.strings['devices.code.expired']) {
    throw new Error('an expired code gave no message');
  }
  const sent = [];
  await appBindings.checkCode((command) => {
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
  await appBindings.sendLink(refuse('notConnected'));
  if (
    confirm.open ||
    !code.open ||
    problem.textContent !== appApi.strings['devices.code.offline']
  ) {
    throw new Error('a failed send did not return to the code');
  }
  code.close();
  await appBindings.sendLink((command, args) => {
    sent.push(command, args.code);
    return Promise.resolve();
  });
  if (
    sent.join() !== 'check_link_code,send_link,dyapp-link:1:code' ||
    document.getElementById('devices-status').textContent !== appApi.strings['devices.sent']
  ) {
    throw new Error(`send sent ${sent}`);
  }
};
// The link page against a stub backend; two clients linking are the smoke's.
const checkLinkScreen = smoke.uiSmokeLifecycleGuards.linkScreen;
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
  await appBindings.openChat(peer, call(true));
  if (notice.hidden) throw new Error('a chat on a linked device has no history notice');
  await smoke.shot('chat-linked');
  await appBindings.openChat(peer, call(false));
  if (!notice.hidden) throw new Error('a chat on a first device says it is linked');
  appBindings.closeChat();
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
    !document
      .getElementById('link-peer')
      .textContent.includes(appApi.peerName(appBindings.linkJoined.peerId))
  ) {
    throw new Error('the link did not finish');
  }
  await smoke.shot('link-joined');
  document.getElementById('link-open').click();
  await waitFor('identity');
  await liveDevices(2);
  return appBindings.linkJoined.peerId;
};
const linkSendStep = async (code) => {
  await openArea('settings', 'devices');
  await appApi.devicesShown;
  document.getElementById('device-link').click();
  document.getElementById('device-code').value = code;
  document.getElementById('device-code-next').click();
  await appApi.codeChecked;
  document.getElementById('device-confirm-action').click();
  await appApi.linkSent;
  if (document.getElementById('devices-status').textContent !== appApi.strings['devices.sent']) {
    throw new Error('the link was not sent');
  }
};
const linkDisconnectStep = async () => {
  await openArea('settings', 'devices');
  await liveDevices(2);
  await appBindings.showDevices();
  document.getElementById('device-disconnect-1').click();
  document.getElementById('disconnect-action').click();
  const dialog = document.getElementById('disconnect');
  await appApi.disconnectDone;
  if (dialog.open) {
    throw new Error(
      `disconnect failed: ${document.getElementById('disconnect-problem').textContent}`,
    );
  }
  await closed(dialog);
  await appApi.devicesShown;
  if (document.getElementById('devices-status').textContent !== appApi.strings['devices.gone']) {
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
smoke.uiSmokeGuards = Object.freeze({
  checkLinkScreen,
  linkStep,
  checkLinkedNotice,
  checkStaleSettingsInventory,
  checkStaleNode: smoke.uiSmokeNodeGuard,
  checkStaleTray,
  checkStaleRemovePreparation,
  checkStaleWithdrawal,
  checkDeviceRevoked,
  checkDevices,
  checkLinkCode,
  checkStaleLaunchConnection,
  checkStaleNetworkForm: smoke.uiSmokeNetworkGuards.form,
  checkStaleNetworkCheck: smoke.uiSmokeNetworkGuards.requests,
  checkStaleCopy: smoke.uiSmokeCopyGuard,
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
