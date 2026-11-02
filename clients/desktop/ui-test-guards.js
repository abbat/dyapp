// Desktop UI smoke: remaining generation and Settings checks.
// An old inventory load must not replace the newer identity's Settings rows or error.
const checkStaleSettingsInventory = async () => {
  const readyState = current;
  const list = document.getElementById('settings-identities');
  const error = document.getElementById('settings-error');
  const switchAndLoad = async () => {
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    await openArea('settings');
    const items = await invokeTest('list_identities');
    await showSettingsIdentities(() => Promise.resolve(items));
    return [...list.children].map((row) => row.dataset.peerId).join();
  };
  await openArea('settings');
  let finishLoad;
  const oldLoad = showSettingsIdentities(() => new Promise((resolve) => { finishLoad = resolve; }));
  const rows = await switchAndLoad();
  if (!rows) throw new Error('new Settings inventory is empty');
  finishLoad([]);
  await oldLoad;
  if ([...list.children].map((row) => row.dataset.peerId).join() !== rows || !error.hidden) {
    throw new Error('old inventory load changed new settings');
  }

  let failLoad;
  const oldFailure = showSettingsIdentities(() => new Promise((_, reject) => { failLoad = reject; }));
  const newerRows = await switchAndLoad();
  failLoad(new Error('old inventory'));
  await oldFailure;
  if ([...list.children].map((row) => row.dataset.peerId).join() !== newerRows
    || !error.hidden) throw new Error('old inventory error changed new settings');
};
// Node Settings has two load waits and a save completion tied to one identity.
const checkStaleNode = async () => {
  const readyState = current;
  const node = document.getElementById('node');
  const save = document.getElementById('node-save');
  const saved = { enabled: false, diskMb: 40, maxConnections: 2 };
  const switchAndShow = async () => {
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    await openArea('settings');
    await window.nodeShown;
    await showNode((command) => Promise.resolve(command === 'node_status'
      ? { status: 'off', peerId: '' } : saved));
  };
  const checkNew = (message) => {
    if (node.hidden || node.dataset.status !== 'off'
      || document.getElementById('node-enabled').checked
      || document.getElementById('node-disk').value !== '40'
      || document.getElementById('node-connections').value !== '2') throw new Error(message);
  };
  await openArea('settings');
  let finishStatus;
  const oldStatus = showNode(() => new Promise((resolve) => { finishStatus = resolve; }));
  await switchAndShow();
  finishStatus({ status: 'running', peerId: 'old' });
  await oldStatus;
  checkNew('old node status changed new form');

  let finishSettings;
  const oldSettings = showNode((command) => command === 'node_status'
    ? Promise.resolve({ status: 'off', peerId: '' })
    : new Promise((resolve) => { finishSettings = resolve; }));
  await new Promise((resolve) => { setTimeout(resolve, 0); });
  if (!finishSettings) throw new Error('old node settings load did not start');
  await switchAndShow();
  finishSettings({ enabled: true, diskMb: 100, maxConnections: 10 });
  await oldSettings;
  checkNew('old node settings changed new form');

  let failSettings;
  const oldSettingsFailure = showNode((command) => command === 'node_status'
    ? Promise.resolve({ status: 'off', peerId: '' })
    : new Promise((_, reject) => { failSettings = reject; }));
  await new Promise((resolve) => { setTimeout(resolve, 0); });
  await switchAndShow();
  failSettings(new Error('old settings'));
  await oldSettingsFailure;
  checkNew('old node settings error changed new form');

  let finishOldSave;
  const oldSave = saveNodeSettings(saved,
    () => new Promise((resolve) => { finishOldSave = resolve; }));
  await switchAndShow();
  let finishNewSave;
  const newSave = saveNodeSettings({ ...saved, enabled: true },
    () => new Promise((resolve) => { finishNewSave = resolve; }));
  finishOldSave({ status: 'running', peerId: 'old' });
  await oldSave;
  if (!save.disabled || node.dataset.status !== 'starting') {
    throw new Error('old node save changed new status or unlocked Save');
  }
  finishNewSave({ status: 'off', peerId: '' });
  await newSave;
  if (save.disabled || node.dataset.status !== 'off') throw new Error('new node save did not finish');

  let failOldSave;
  const oldSaveFailure = saveNodeSettings(saved,
    () => new Promise((_, reject) => { failOldSave = reject; }));
  await switchAndShow();
  failOldSave(new Error('old save'));
  await oldSaveFailure;
  checkNew('old node save error changed new status');
  await showNode();
};
// A tray result from an old Settings view cannot replace the new controls.
const checkStaleTray = async () => {
  const readyState = current;
  const show = document.getElementById('tray-show');
  const close = document.getElementById('tray-close');
  const note = document.getElementById('tray-note');
  await openArea('settings');
  await window.trayShown;
  let finishOld;
  const oldTray = setTray(null, () => new Promise((resolve) => { finishOld = resolve; }));
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  await openArea('settings');
  await window.trayShown;
  await setTray(null, () => Promise.resolve({
    available: true, show: true, closeToTray: true,
  }));
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
  const oldList = showRemove(() => new Promise((resolve) => { finishList = resolve; }));
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  finishList(items);
  await oldList;
  if (removeDialog.open || !removeDialog.hidden) {
    throw new Error('old removal inventory opened new dialog');
  }

  let finishProfile;
  const oldProfile = showRemove((command) => command === 'list_identities'
    ? Promise.resolve(items) : new Promise((resolve) => { finishProfile = resolve; }));
  await new Promise((resolve) => { setTimeout(resolve, 0); });
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
  removeDialog.close();
};
// A revocation of another slot wipes nothing; the active slot's closes dialogs and says why.
const checkDeviceRevoked = async () => {
  await showRemove((command) => Promise.resolve(command === 'list_identities' ? [] : null));
  let removed = 0;
  const remove = async () => { removed += 1; };
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
  if (revokedDialog.open || !revokedDialog.hidden) throw new Error('the notice stayed open');
};
// An old withdrawal cannot remove the new identity or switch its dialog to Offline.
const checkStaleWithdrawal = async () => {
  const readyState = current;
  let removed = 0;
  const remove = () => { removed += 1; };
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
  const oldSuccess = withdrawThenRemove(() => new Promise((resolve) => { finishOld = resolve; }), remove);
  await switchAndSetDialog();
  finishOld();
  await oldSuccess;
  if (removed || removeAsk.hidden || !removeOffline.hidden) {
    throw new Error('old withdrawal success removed new identity');
  }

  let failOld;
  const oldFailure = withdrawThenRemove(() => new Promise((_, reject) => { failOld = reject; }), remove);
  await switchAndSetDialog();
  failOld(new Error('old withdrawal'));
  await oldFailure;
  if (removed || removeAsk.hidden || !removeOffline.hidden
    || !removeRetry.disabled || !removeAnyway.disabled) {
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
    await connectAtLaunch((command) => Promise.resolve(command === 'load_network'
      ? { name: 'new network' } : true));
  };
  const checkCurrent = (message) => {
    if (connectionStatus.dataset.state !== 'connected'
      || document.getElementById('connection-network').textContent !== 'new network') {
      throw new Error(message);
    }
  };
  let finishActive;
  let oldDials = 0;
  const oldActive = connectAtLaunch((command) => {
    if (command === 'load_network') return new Promise((resolve) => { finishActive = resolve; });
    oldDials += 1;
    return Promise.resolve(true);
  });
  await currentConnection();
  finishActive({ name: 'old network' });
  await oldActive;
  checkCurrent('old active network changed new connection');
  if (oldDials) throw new Error('old active network started a new dial');

  let finishDial;
  const oldDial = connectAtLaunch((command) => command === 'load_network'
    ? Promise.resolve({ name: 'old network' })
    : new Promise((resolve) => { finishDial = resolve; }));
  await new Promise((resolve) => { setTimeout(resolve, 0); });
  if (!finishDial) throw new Error('old connection dial did not start');
  await currentConnection();
  finishDial(false);
  await oldDial;
  checkCurrent('old dial changed new connection');
};
// Old network loads and draft-save errors must leave the new form intact.
const checkStaleNetworkForm = async () => {
  const readyState = current;
  const switchAndShow = async () => {
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    await showNetwork((_, args) => Promise.resolve(args.file === 'active'
      ? { seeds: ['new-active'], name: 'new' }
      : { seeds: ['new-draft'], name: 'new draft' }));
  };
  const checkNew = (message) => {
    if (seeds.value !== 'new-draft' || networkName.value !== 'new draft'
      || document.getElementById('network-status').textContent
        !== window.strings['network.active']) throw new Error(message);
  };
  let finishActive;
  let oldDraftCalls = 0;
  const oldActive = showNetwork((_, args) => {
    if (args.file === 'active') return new Promise((resolve) => { finishActive = resolve; });
    oldDraftCalls += 1;
    return Promise.resolve(null);
  });
  await switchAndShow();
  finishActive({ seeds: ['old'], name: 'old' });
  await oldActive;
  checkNew('old active load changed new network form');
  if (oldDraftCalls) throw new Error('old active load continued to draft');

  let finishDraft;
  const oldDraft = showNetwork((_, args) => args.file === 'active'
    ? Promise.resolve({ seeds: ['old'], name: 'old' })
    : new Promise((resolve) => { finishDraft = resolve; }));
  await new Promise((resolve) => { setTimeout(resolve, 0); });
  if (!finishDraft) throw new Error('old draft load did not start');
  await switchAndShow();
  finishDraft({ seeds: ['old-draft'], name: 'old draft' });
  await oldDraft;
  checkNew('old draft load changed new network form');

  let failSave;
  const oldSave = saveNetworkDraft(() => new Promise((_, reject) => { failSave = reject; }));
  await switchAndShow();
  failSave(new Error('old draft save'));
  await oldSave;
  checkNew('old draft-save error changed new network status');
};
// Old network checks cannot activate the new identity or unlock its busy form.
const checkStaleNetworkCheck = async () => {
  const readyState = current;
  let finishOldCheck;
  let oldActivations = 0;
  const oldCheck = check((command) => {
    if (command === 'check_connection') {
      return new Promise((resolve) => { finishOldCheck = resolve; });
    }
    oldActivations += 1;
    return Promise.resolve();
  });
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  if (checkButton.disabled || !cancelButton.hidden) {
    throw new Error('new network form inherited old busy controls');
  }
  let finishNewCheck;
  const newCheck = check((command) => command === 'check_connection'
    ? new Promise((resolve) => { finishNewCheck = resolve; }) : Promise.resolve());
  finishOldCheck([{ status: 'connected' }]);
  await oldCheck;
  if (oldActivations || !checkButton.disabled || cancelButton.hidden
    || document.getElementById('network-status').textContent
      !== window.strings['network.checking']) {
    throw new Error('old network check changed new busy form or activated');
  }
  finishNewCheck([{ status: 'unreachable' }]);
  await newCheck;
  if (checkButton.disabled || !cancelButton.hidden) {
    throw new Error('current network check did not release controls');
  }

  let finishActivation;
  let oldApply = 0;
  const oldActivation = check((command) => {
    if (command === 'check_connection') return Promise.resolve([{ status: 'connected' }]);
    if (command === 'activate_connection') {
      return new Promise((resolve) => { finishActivation = resolve; });
    }
    oldApply += 1;
    return Promise.resolve();
  });
  await new Promise((resolve) => { setTimeout(resolve, 0); });
  if (!finishActivation) throw new Error('old network activation did not start');
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  const status = document.getElementById('network-status').textContent;
  finishActivation();
  await oldActivation;
  if (oldApply || document.getElementById('network-status').textContent !== status
    || checkButton.disabled || !cancelButton.hidden) {
    throw new Error('old activation changed new network or applied node');
  }

  let failOldCheck;
  const oldFailure = check(() => new Promise((_, reject) => { failOldCheck = reject; }));
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  const newerStatus = document.getElementById('network-status').textContent;
  failOldCheck(new Error('old check'));
  await oldFailure;
  if (document.getElementById('network-status').textContent !== newerStatus
    || checkButton.disabled || !cancelButton.hidden) {
    throw new Error('old network check error changed new form');
  }
};
// A delayed copy cannot overwrite the next identity's Peer ID, label or selection.
const checkStaleCopy = async () => {
  const readyState = current;
  const source = document.getElementById('peer-id');
  const button = document.getElementById('copy-peer-id');
  const clipboard = navigator.clipboard;
  if (!clipboard) throw new Error('no clipboard API for stale copy check');
  const switchAndMark = async () => {
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    source.textContent = 'b'.repeat(64);
    button.textContent = 'new copy';
    window.getSelection().removeAllRanges();
  };
  try {
    let rejectOld;
    clipboard.writeText = () => new Promise((_, reject) => { rejectOld = reject; });
    button.click();
    await new Promise((resolve) => { setTimeout(resolve, 0); });
    if (!rejectOld) throw new Error('old clipboard write did not start');
    const oldFailure = window.copied;
    await switchAndMark();
    rejectOld(new Error('old clipboard'));
    await oldFailure;
    if (source.textContent !== 'b'.repeat(64) || button.textContent !== 'new copy'
      || window.getSelection().rangeCount) throw new Error('old copy fallback changed new identity');

    let finishOld;
    clipboard.writeText = () => new Promise((resolve) => { finishOld = resolve; });
    button.click();
    await new Promise((resolve) => { setTimeout(resolve, 0); });
    if (!finishOld) throw new Error('old clipboard success did not start');
    const oldSuccess = window.copied;
    await switchAndMark();
    finishOld();
    await oldSuccess;
    if (button.textContent !== 'new copy') throw new Error('old copy success changed new label');

    const originalTimeout = globalThis.setTimeout;
    let oldReset;
    globalThis.setTimeout = (callback, delay, ...args) => {
      if (delay === 2000) { oldReset = callback; return 0; }
      return originalTimeout(callback, delay, ...args);
    };
    try {
      clipboard.writeText = () => Promise.resolve();
      button.click();
      await window.copied;
    } finally { globalThis.setTimeout = originalTimeout; }
    if (!oldReset) throw new Error('copy label reset was not scheduled');
    await switchAndMark();
    oldReset();
    if (button.textContent !== 'new copy') throw new Error('old copy timer changed new label');
  } finally {
    delete clipboard.writeText;
    source.textContent = readyState.peerId;
    button.textContent = window.strings['action.copy'];
  }
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
  const picker = placePicker(countryInput, input, list, () => countryInput.value, () => {
    calls += 1;
    return calls === 1 ? new Promise((resolve) => { finishOld = resolve; })
      : Promise.resolve([{ id: 2, name: 'Berlin' }]);
  });
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
  if (picker.id() !== '2' || !input.validity.valid
    || list.querySelector('option')?.value !== 'Berlin') {
    throw new Error('old place suggestion replaced new identity');
  }
};
// Removes the opened identity through both confirmations, then opens the one left.
const removeOpened = async (state, mode) => {
  const { strings } = window;
  if (state.status === 'ready') {
    await openArea('settings');
    document.getElementById('settings-remove').click();
    await waitFor('remove');
    if (!document.getElementById('remove-action').disabled) {
      throw new Error('remove enabled before confirmation');
    }
    document.getElementById('remove-cancel').click();
    if (document.getElementById('remove').open || document.getElementById('settings').hidden) {
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
  if (document.getElementById('remove-peer').textContent
    !== (state.peerId || strings[`recovery.${state.reason}`])) throw new Error('wrong Peer ID');
  if (state.peerId && document.getElementById('remove-copies').textContent
    !== strings['remove.lastCopy']) throw new Error('last copy not shown');
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
  return status === strings['network.empty'] ? 'empty'
    : [...document.querySelectorAll('#seed-rows li')].map((row) => row.dataset.status);
};
const connection = () => document.getElementById('connection-status').dataset.state;
// A failed check shows the error with every row in its details and keeps the shell status.
const expectFailed = (rows, state) => {
  const details = document.getElementById('network-error-details').textContent;
  if (document.getElementById('network-error').hidden
    || rows.some((row) => !details.includes(row))) throw new Error(`error details ${details}`);
  if (connection() !== state) throw new Error(`failed check left ${connection()}`);
};
// A node that answers activates the list at once and the shell shows Connected.
const connectNetwork = async (seed) => {
  const rows = await submitCheck(seed);
  if (rows.join() !== 'connected') throw new Error(`rows ${rows}`);
  if (document.getElementById('network-status').textContent
    !== window.strings['network.active']) throw new Error('connection not activated');
  if (connection() !== 'connected') throw new Error(`shell shows ${connection()}`);
  if (!document.getElementById('identity').hidden) await shot('network-connected');
};
// An empty list is refused, then a refused and a malformed row activate nothing.
const checkNetwork = async () => {
  // The launch connect settles first, so it cannot overwrite the status this check sets.
  await window.connectionShown;
  const state = connection();
  if (await submitCheck('') !== 'empty') throw new Error('empty list not reported');
  await shot('network-empty');
  const rows = await submitCheck(testSeeds);
  if (rows.join() !== 'unreachable,invalidAddress') throw new Error(`rows ${rows}`);
  await shot('network-failure');
  if (document.getElementById('network-status').textContent
    !== window.strings['network.invalidAddress']) throw new Error('row problem not summarized');
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
  window.connectionChecked.finally(() => { settled = true; });
  while (!settled) {
    cancel.click();
    await new Promise((resolve) => { setTimeout(resolve, 200); });
  }
  if (document.getElementById('network-status').textContent
    !== window.strings['network.cancelled']) throw new Error('check not cancelled');
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
    if (shown.join() !== statuses || document.getElementById('network-status').textContent
      !== window.strings[summary]) throw new Error(`${rows}: rows ${shown}`);
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
  if (document.getElementById('network-status').textContent
    !== window.strings['network.invalidAddress']
    || [...document.querySelectorAll('#seed-rows li')].map((row) => row.dataset.status)
      .join() !== 'unreachable,invalidAddress') {
    throw new Error('switching areas lost the failed row status');
  }
  await openArea('profile-area');
};
const checkSettingsLanguage = async () => {
  const strings = window.strings;
  await openArea('settings');
  const expectedTitle = window.pseudoLocale ? '[Séttíngs····]'
    : window.locale === 'ru' ? 'Настройки' : 'Settings';
  if (document.querySelector('#settings h1').textContent !== expectedTitle) {
    throw new Error('Settings title uses the wrong language');
  }
  for (const [id, key] of [
    ['settings-create', 'identity.add'], ['settings-switch', 'identity.switch'],
    ['settings-remove', 'identity.remove'], ['node-save', 'node.save'],
  ]) {
    if (document.getElementById(id).textContent !== strings[key]) {
      throw new Error(`Settings control ${id} is not localized`);
    }
  }
  await checkNetwork();
  if (document.getElementById('network-status').textContent
      !== strings['network.invalidAddress']) {
    throw new Error('Network status is not localized');
  }
  await openArea('profile-area');
};
// Settings → Privacy starts at the defaults, saves each change, and Appearance switches the theme.
const privacyIds = ['send-views', 'send-read-receipts', 'send-typing', 'presence-matches',
  'message-notifications'];
const privacyState = () => privacyIds.map((id) => document.getElementById(id).checked).join();
const expectPrivacy = async (expected) => {
  await openArea('settings');
  await window.privacyShown;
  if (privacyState() !== expected || shown('privacy-error')) {
    throw new Error(`privacy settings ${privacyState()}, expected ${expected}`);
  }
};
const checkPrivacy = async (restarted) => {
  await openArea('settings');
  await window.privacyShown;
  await window.blockedShown;
  const expected = restarted ? 'true,true,false,false,false' : 'false,false,true,true,true';
  if (privacyState() !== expected || shown('privacy-error')
    || document.getElementById('presence-nobody').checked !== restarted) {
    throw new Error(`privacy settings ${privacyState()}, expected ${expected}`);
  }
  if (!shown('blocked-none') || document.getElementById('blocked-list').children.length) {
    throw new Error('blocked list not empty');
  }
  const noticeChoice = document.getElementById('notice-content');
  if (noticeChoice.value !== (restarted ? 'text' : 'fixed') || noticeChoice.disabled !== restarted) {
    throw new Error(`notice content ${noticeChoice.value}, disabled ${noticeChoice.disabled}`);
  }
  if (restarted) return;
  noticeChoice.value = 'text';
  noticeChoice.dispatchEvent(new Event('change', { bubbles: true }));
  await window.privacySaved;
  for (const id of ['send-views', 'send-read-receipts', 'send-typing', 'presence-nobody',
    'message-notifications']) {
    document.getElementById(id).click();
    await window.privacySaved;
  }
  if (shown('privacy-error')) throw new Error('privacy settings not saved');
  const root = document.documentElement;
  document.getElementById('theme-dark').click();
  if (root.dataset.theme !== 'dark') throw new Error('dark theme not applied');
  document.getElementById('theme-system').click();
  if ('theme' in root.dataset) throw new Error('system theme not restored');
  const blockedPeer = 'f'.repeat(64);
  const blocked = await invokeTest('block_peer', { peerId: blockedPeer });
  if (blocked.revocation !== 'notNeeded') throw new Error(`unexpected block state: ${blocked.revocation}`);
  await openArea('profile-area');
  await openArea('settings');
  await window.blockedShown;
  const retry = document.getElementById('retry-revocation-1');
  if (!retry || !document.getElementById('unblock-1')
    || document.getElementById('blocked-revoke-status').textContent) {
    throw new Error('blocked peer has no retry and unblock actions');
  }
  retry.click();
  await window.revocationRetried;
  if (document.getElementById('blocked-revoke-status').textContent
    !== window.strings['settings.blocked.notNeeded']) {
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
  const rows = [{ key: [1, 2, 255], this: false }, { key: [10, 11, 12], this: true }];
  const stale = showDevices(() => new Promise((resolve) => { setTimeout(() => resolve([rows[0]])); }));
  const list = (command) => {
    if (command !== 'list_devices') throw new Error(`unexpected ${command}`);
    return Promise.resolve(structuredClone(rows));
  };
  await showDevices(() => Promise.resolve([]));
  if (!card.hidden) throw new Error('an unpublished identity shows Devices');
  await openArea('settings');
  await showDevices(list);
  await stale;
  const shownKeys = [...document.querySelectorAll('#devices-list code')].map((key) => key.textContent);
  if (card.hidden || shownKeys.join() !== '0a0b0c,0102ff'
    || document.querySelectorAll('#devices-list button').length !== 1) {
    throw new Error(`wrong device rows: ${shownKeys}`);
  }
  document.getElementById('device-disconnect-1').click();
  if (!dialog.open || document.activeElement.id !== 'disconnect-cancel'
    || document.getElementById('disconnect-device').textContent !== '0102ff') {
    throw new Error('disconnect dialog did not open on the row');
  }
  await disconnectNow(() => Promise.reject(new Error('notConnected')));
  if (document.getElementById('disconnect-problem').hidden
    || document.getElementById('disconnect-retry').hidden
    || !document.getElementById('disconnect-action').hidden) {
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
  await new Promise((resolve) => { setTimeout(resolve); });
  if (document.getElementById('devices-status').textContent !== window.strings['devices.gone']) {
    throw new Error('disconnect did not report its result');
  }
};
globalThis.uiSmokeGuards = Object.freeze({
  checkStaleSettingsInventory, checkStaleNode, checkStaleTray, checkStaleRemovePreparation,
  checkStaleWithdrawal, checkDeviceRevoked, checkDevices, checkStaleLaunchConnection, checkStaleNetworkForm,
  checkStaleNetworkCheck, checkStaleCopy, checkStalePlaceSuggestions, removeOpened,
  testSeeds, connection, connectNetwork, checkNetwork, cancelNetwork, checkWrongNetwork,
  checkNetworkInSettings, checkSettingsLanguage, expectPrivacy, checkPrivacy,
});
