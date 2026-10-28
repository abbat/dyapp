// Runs in the actual Tauri webview after the bundled page loads.
const invokeTest = window.__TAURI__.core.invoke;
const fail = (error) => invokeTest('ui_test_result', {
  passed: false, peerId: '', reason: String(error),
});
// Saves screenshots of the current screen when the run asks for them (a no-op otherwise).
const shot = (name) => invokeTest('ui_test_shot', { name }).catch(() => {});
const waitFor = (id) => new Promise((resolve, reject) => {
  const section = document.getElementById(id);
  if (!section.hidden) { resolve(); return; }
  const observer = new MutationObserver(() => {
    if (!section.hidden) { observer.disconnect(); clearTimeout(timeout); resolve(); }
  });
  observer.observe(section, { attributes: true, attributeFilter: ['hidden'] });
  const timeout = setTimeout(() => { observer.disconnect(); reject(new Error(`${id} not shown`)); }, 10000);
});
// The hidden attribute must remove an element from the layout, whatever its display rule.
const hiddenRendered = () => [...document.querySelectorAll('[hidden]')]
  .filter((element) => getComputedStyle(element).display !== 'none')
  .map((element) => element.id || element.tagName);
// First run: no identity actions, the recommended available store preselected, and a storage
// choice alone creates nothing until Create.
const chooseStorage = async (mode) => {
  const choice = document.getElementById(`${mode}-choice`);
  if (!choice || choice.disabled || choice.hidden) throw new Error('storage choice unavailable');
  const rendered = hiddenRendered();
  if (rendered.length) throw new Error(`hidden but rendered: ${rendered}`);
  if (!document.getElementById('switch-identity').hidden
    || !document.getElementById('remove-identity').hidden) throw new Error('identity actions on setup');
  const system = document.getElementById('system-choice');
  if (!document.getElementById(system.disabled ? 'file-choice' : 'system-choice').checked) {
    throw new Error('recommended storage not preselected');
  }
  if (system.disabled && !document.getElementById('system-reason').textContent) {
    throw new Error('system storage reason missing');
  }
  await shot('setup');
  const before = (await invokeTest('list_identities')).length;
  choice.click();
  if ((await invokeTest('list_identities')).length !== before
    || document.getElementById('setup').hidden) throw new Error('storage choice created an identity');
  document.getElementById('create-identity').click();
  await waitFor('identity');
};
const pickReady = async (mode) => {
  const pick = document.querySelector(
    `#identities button[data-status="ready"][data-mode="${mode}"]`);
  if (!pick) throw new Error('no ready identity in the picker');
  await shot('picker');
  pick.click();
  await waitFor('identity');
};
const checkSettingsInventory = async (peerId, mode) => {
  document.getElementById('settings-open').click();
  await waitFor('settings');
  const expected = await invokeTest('list_identities');
  const list = document.getElementById('settings-identities');
  await new Promise((resolve, reject) => {
    if (list.children.length === expected.length) { resolve(); return; }
    const observer = new MutationObserver(() => {
      if (list.children.length === expected.length) {
        observer.disconnect(); clearTimeout(timeout); resolve();
      }
    });
    observer.observe(list, { childList: true });
    const timeout = setTimeout(() => {
      observer.disconnect(); reject(new Error('Settings inventory did not load'));
    }, 10000);
  });
  await shot('settings');
  const active = [...list.children].filter((item) => item.dataset.active === 'true');
  if (active.length !== 1 || !active[0].textContent.includes(peerId)
      || !active[0].textContent.includes(window.strings[`identity.mode.${mode}`])) {
    throw new Error('Settings marked the wrong active identity or storage mode');
  }
  document.getElementById('settings-back').click();
  await waitFor('identity');
};
const switchFromSettings = async (mode) => {
  document.getElementById('settings-open').click();
  await waitFor('settings');
  document.getElementById('settings-switch').click();
  await waitFor('picker');
  await pickReady(mode);
};
// Removes the opened identity through both confirmations, then opens the one left.
const removeOpened = async (state, mode) => {
  const { strings } = window;
  if (state.status === 'ready') {
    document.getElementById('settings-open').click();
    await waitFor('settings');
    document.getElementById('settings-remove').click();
    await waitFor('remove');
    if (!document.getElementById('remove-action').disabled) {
      throw new Error('remove enabled before confirmation');
    }
    document.getElementById('remove-cancel').click();
    await waitFor('settings');
    document.getElementById('settings-remove').click();
  } else {
    document.getElementById('remove-identity').click();
  }
  await waitFor('remove');
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
  document.getElementById('seeds').value = rows;
  document.getElementById('check-connection').click();
  await window.connectionChecked;
  const status = document.getElementById('network-status').textContent;
  return status === strings['network.empty'] ? 'empty'
    : [...document.querySelectorAll('#seed-rows li')].map((row) => row.dataset.status);
};
// A reachable node enables Continue, which activates the checked draft.
const connectNetwork = async (seed) => {
  const rows = await submitCheck(seed);
  if (rows.join() !== 'connected') throw new Error(`rows ${rows}`);
  const next = document.getElementById('continue-connection');
  if (next.disabled) throw new Error('continue disabled after a connected check');
  next.click();
  await window.connectionActivated;
  if (document.getElementById('network-status').textContent
    !== window.strings['network.active']) throw new Error('connection not activated');
  await shot('network-connected');
};
// An empty list is refused, then a refused and a malformed row keep Continue disabled.
const checkNetwork = async () => {
  if (await submitCheck('') !== 'empty') throw new Error('empty list not reported');
  await shot('network-empty');
  const rows = await submitCheck(testSeeds);
  if (rows.join() !== 'unreachable,invalidAddress') throw new Error(`rows ${rows}`);
  await shot('network-failure');
  if (document.getElementById('network-status').textContent
    !== window.strings['network.invalidAddress']) throw new Error('row problem not summarized');
  if (!document.getElementById('continue-connection').disabled) {
    throw new Error('continue enabled without a connected node');
  }
};
const checkNetworkInSettings = async () => {
  const editor = document.getElementById('network');
  const seeds = document.getElementById('seeds');
  document.getElementById('settings-open').click();
  await waitFor('settings');
  if (!document.getElementById('settings').contains(editor)) {
    throw new Error('Settings did not show the network editor');
  }
  seeds.value = testSeeds;
  seeds.dispatchEvent(new Event('input', { bubbles: true }));
  document.getElementById('settings-back').click();
  await waitFor('identity');
  if (!document.getElementById('identity').contains(editor) || seeds.value !== testSeeds) {
    throw new Error('Back lost the network draft');
  }
  document.getElementById('settings-open').click();
  await waitFor('settings');
  await checkNetwork();
  document.getElementById('settings-back').click();
  await waitFor('identity');
  document.getElementById('settings-open').click();
  await waitFor('settings');
  if (document.getElementById('network-status').textContent
    !== window.strings['network.invalidAddress']
    || [...document.querySelectorAll('#seed-rows li')].map((row) => row.dataset.status)
      .join() !== 'unreachable,invalidAddress') {
    throw new Error('Settings lost the failed row status');
  }
  document.getElementById('settings-back').click();
  await waitFor('identity');
};
const checkSettingsLanguage = async () => {
  const strings = window.strings;
  document.getElementById('settings-open').click();
  await waitFor('settings');
  const expectedTitle = window.pseudoLocale ? '[Séttíngs····]'
    : window.locale === 'ru' ? 'Настройки' : 'Settings';
  if (document.querySelector('#settings h2').textContent !== expectedTitle) {
    throw new Error('Settings title uses the wrong language');
  }
  for (const [id, key] of [
    ['settings-create', 'identity.add'], ['settings-switch', 'identity.switch'],
    ['settings-remove', 'identity.remove'], ['settings-back', 'settings.back'],
    ['node-save', 'node.save'],
  ]) {
    if (document.getElementById(id).textContent !== strings[key]) {
      throw new Error(`Settings control ${id} is not localized`);
    }
  }
  await checkNetwork();
  if (document.getElementById('network-status').textContent
      !== strings['network.invalidAddress']) {
    throw new Error('Settings network status is not localized');
  }
  document.getElementById('settings-back').click();
  await waitFor('identity');
};
// Saves the node form and returns the status it ends on.
const saveNode = async (enabled, disk) => {
  document.getElementById('node-enabled').checked = enabled;
  document.getElementById('node-disk').value = disk;
  window.nodeApplied = null;
  document.getElementById('node-save').click();
  await window.nodeApplied;
  return document.getElementById('node').dataset.status;
};
// The node starts on the loopback network, then stops; bad limits never reach the app.
const checkNode = async () => {
  const { strings } = window;
  document.getElementById('settings-open').click();
  await waitFor('settings');
  await window.nodeShown;
  const form = document.getElementById('node');
  if (form.hidden || document.getElementById('node-enabled').checked
    || document.getElementById('node-disk').value !== '1024'
    || document.getElementById('node-connections').value !== '64'
    || form.dataset.status !== 'off'
    || document.getElementById('node-status').textContent !== strings['node.off']
    || document.querySelector('#node h2').textContent !== strings['node.title']) {
    throw new Error('node form not shown with its defaults');
  }
  if (await saveNode(true, '3') !== 'invalidLimits') throw new Error('tiny disk limit accepted');
  const running = await saveNode(true, '64');
  const text = document.getElementById('node-status').textContent;
  if (running !== 'running' || !text.startsWith(strings['node.running'])
    || text.length <= strings['node.running'].length + 1) {
    throw new Error(`node did not start: ${text}`);
  }
  if (await saveNode(false, '64') !== 'off') throw new Error('node did not stop');
  document.getElementById('settings-back').click();
  await waitFor('identity');
};
const checkEmptyProfilePreview = async () => {
  await shot('profile-edit');
  document.getElementById('profile-preview-action').click();
  await waitFor('profile-preview');
  await shot('profile-preview');
  if (document.getElementById('profile-public-fields').textContent
    !== window.strings['profile.empty']) throw new Error('unset fields appeared in preview');
  if (document.getElementById('profile-age').textContent
    !== window.strings['profile.ageUnknown']) throw new Error('unknown age not shown');
  if (!document.getElementById('profile-date-warning').hidden) {
    throw new Error('full-date warning shown without a birth date');
  }
  if (document.getElementById('profile-attested').checked
    || document.getElementById('profile-disclosure').checked) {
    throw new Error('publish confirmations were preselected');
  }
  const blocked = waitForProfileStatus('profile.problem.notAcknowledged');
  document.getElementById('profile-attested').checked = true;
  document.getElementById('profile-publish').click();
  await blocked;
  if (!document.getElementById('profile-cancel').hidden) {
    throw new Error('puzzle started before the public-data warning was acknowledged');
  }
  document.getElementById('profile-attested').checked = true;
  document.getElementById('profile-disclosure').checked = true;
  const month = document.getElementById('birth-month');
  month.value = '2';
  month.dispatchEvent(new Event('input', { bubbles: true }));
  if (!document.getElementById('profile-preview').hidden
    || document.getElementById('profile-attested').checked
    || document.getElementById('profile-disclosure').checked) {
    throw new Error('editing kept a stale public-data confirmation');
  }
};
const waitForProfileStatus = (key, detail = '', ms = 10000) => new Promise((resolve, reject) => {
  const status = document.getElementById('profile-status');
  const expected = `${window.strings[key]}${detail ? ` ${detail}` : ''}`;
  const observer = new MutationObserver(() => {
    if (status.textContent === expected) {
      observer.disconnect(); clearTimeout(timeout); resolve();
    }
  });
  observer.observe(status, { childList: true });
  const timeout = setTimeout(() => { observer.disconnect(); reject(new Error(`profile status: ${status.textContent}`)); }, ms);
});
// Picks Germany and the first suggestion for "Berl", the largest match: Berlin.
const pickPlace = async (form = 'profile') => {
  const country = document.getElementById(`${form}-country`);
  const place = document.getElementById(`${form}-place`);
  country.value = 'DE';
  country.dispatchEvent(new Event('input', { bubbles: true }));
  place.value = 'Berl';
  place.dispatchEvent(new Event('input', { bubbles: true }));
  await window.placesShown;
  const label = document.querySelector(`#${form}-places option`)?.value;
  if (!label?.startsWith(window.locale === 'ru' ? 'Берлин, ' : 'Berlin, ')) {
    throw new Error(`place suggestion missing: ${label}`);
  }
  place.value = label;
  place.dispatchEvent(new Event('input', { bubbles: true }));
  if (!place.checkValidity()) throw new Error('picked place rejected');
};
const saveProfileForRestart = async () => {
  document.getElementById('profile-gender').value = '1';
  document.getElementById('birth-year').value = '1990';
  document.getElementById('birth-month').value = '2';
  const place = document.getElementById('profile-place');
  place.value = 'Nowhere';
  place.dispatchEvent(new Event('input', { bubbles: true }));
  await window.placesShown;
  if (place.checkValidity()) throw new Error('unlisted place accepted');
  await pickPlace();
  const saved = waitForProfileStatus('profile.saved');
  document.getElementById('profile-save').click();
  await saved;
};
const publishProfileOnLoopback = async (saved = false, fails = false) => {
  if (saved) {
    if (document.getElementById('profile-gender').value !== '1'
      || document.getElementById('birth-year').value !== '1990'
      || document.getElementById('birth-month').value !== '2') {
      throw new Error('failed publish lost the saved draft');
    }
  } else {
    document.getElementById('profile-gender').value = '1';
    document.getElementById('birth-year').value = '1990';
    document.getElementById('birth-month').value = '2';
  }
  document.getElementById('profile-preview-action').click();
  await waitFor('profile-preview');
  const fields = [...document.querySelectorAll('#profile-public-fields li')]
    .map((item) => item.textContent);
  if (fields.join('|') !== [
    `${window.strings['profile.field.gender']}: ${window.strings['profile.value.gender.Woman']}`,
    `${window.strings['profile.birthYear']}: 1990`,
    `${window.strings['profile.birthMonth']}: 2`,
  ].join('|')) throw new Error(`profile preview changed: ${fields}`);
  const previewAge = document.getElementById('profile-age').textContent;
  const previewDateWarning = document.getElementById('profile-date-warning').hidden;
  document.getElementById('profile-disclosure').checked = true;
  document.getElementById('profile-attested').checked = true;
  const result = waitForProfileStatus(
    fails ? 'profile.notConnected' : 'profile.published', fails ? '' : '1', 150000);
  document.getElementById('profile-publish').click();
  await result;
  await shot(fails ? 'profile-publish-failed' : 'profile-published');
  if (!fails) {
    const readback = [...document.querySelectorAll('#profile-public-fields li')]
      .map((item) => item.textContent);
    if (readback.join('|') !== fields.join('|')
      || document.getElementById('profile-age').textContent !== previewAge
      || document.getElementById('profile-date-warning').hidden !== previewDateWarning) {
      throw new Error(`published readback differs from preview: ${readback}`);
    }
  }
  if (document.getElementById('profile-attested').checked
    || document.getElementById('profile-disclosure').checked) {
    throw new Error('publish confirmations remained selected');
  }
};
const checkSavedProfile = async () => {
  if (document.getElementById('profile-gender').value !== '1'
    || document.getElementById('birth-year').value !== '1990'
    || document.getElementById('birth-month').value !== '2'
    || document.getElementById('birth-day').value !== ''
    || document.getElementById('profile-country').value !== 'DE') {
    throw new Error('profile draft changed after restart');
  }
  const place = document.getElementById('profile-place').value;
  if (!place.startsWith(window.locale === 'ru' ? 'Берлин, ' : 'Berlin, ')) {
    throw new Error(`picked place lost after restart: ${place}`);
  }
  document.getElementById('profile-preview-action').click();
  await waitFor('profile-preview');
  const placeFields = [...document.querySelectorAll('#profile-public-fields li')]
    .map((item) => item.textContent).filter((field) => field.startsWith(window.strings['profile.country'])
      || field.startsWith(window.strings['profile.place']));
  if (placeFields.join('|') !== [
    `${window.strings['profile.country']}: ${new Intl.DisplayNames([window.locale], { type: 'region' }).of('DE')}`,
    `${window.strings['profile.place']}: ${place}`,
  ].join('|')) throw new Error(`place preview changed: ${placeFields}`);
  // Clearing the country clears the place; the later publish steps expect neither.
  const country = document.getElementById('profile-country');
  country.value = '';
  country.dispatchEvent(new Event('input', { bubbles: true }));
  if (document.getElementById('profile-place').value !== '') throw new Error('place outlived its country');
  const cleared = waitForProfileStatus('profile.saved');
  document.getElementById('profile-save').click();
  await cleared;
  document.getElementById('profile-preview-action').click();
  await waitFor('profile-preview');
  const fields = [...document.querySelectorAll('#profile-public-fields li')]
    .map((item) => item.textContent);
  if (fields.join('|') !== [
    `${window.strings['profile.field.gender']}: ${window.strings['profile.value.gender.Woman']}`,
    `${window.strings['profile.birthYear']}: 1990`,
    `${window.strings['profile.birthMonth']}: 2`,
  ].join('|')) throw new Error(`profile preview changed: ${fields}`);
  if (!document.getElementById('profile-age').textContent
    .startsWith(`${window.strings['profile.age']}: `)) {
    throw new Error('known age hidden after restart');
  }
  if (document.getElementById('profile-attested').checked) {
    throw new Error('publish attestation survived restart');
  }
};
// Without a network the search asks to connect and offers a retry; on the loopback node it
// returns the sample, the published profile alone, with the node's filters and no failures.
// The profile has no place, so filtering by Berlin finds nobody.
const searchOnLoopback = async (connected) => {
  document.getElementById('run-search').click();
  await window.searchDone;
  const status = document.getElementById('search-status').textContent;
  const retry = !document.getElementById('search-retry').hidden;
  if (!connected) {
    if (status !== window.strings['search.notConnected'] || !retry) {
      throw new Error(`search without a network: ${status}, retry ${retry}`);
    }
    await shot('search-not-connected');
    return;
  }
  const peers = [...document.querySelectorAll('#search-results [data-peer-id]')]
    .map((item) => item.dataset.peerId);
  const failures = [...document.getElementById('search-failures').children]
    .map((item) => item.textContent);
  if (status !== `${window.strings['search.results']} 1` || retry || failures.length
    || document.getElementById('search-filters').hidden
    || peers.join() !== document.getElementById('peer-id').textContent) {
    throw new Error(`loopback search: ${status}, peers ${peers}, failures ${failures}`);
  }
  await shot('search-results');
  if (document.getElementById('search-place-row').hidden) throw new Error('place filter not offered');
  await pickPlace('search');
  document.getElementById('run-search').click();
  await window.searchDone;
  const placed = document.getElementById('search-status').textContent;
  if (placed !== window.strings['search.empty']) throw new Error(`place search: ${placed}`);
  await shot('search-empty');
  document.getElementById('search-country').value = '';
  document.getElementById('search-country').dispatchEvent(new Event('input', { bubbles: true }));
};
// The fixture forged the signature of the published profile in the node's index: the search drops
// it and lists the forged reply instead of showing a person.
const searchForged = async () => {
  document.getElementById('run-search').click();
  await window.searchDone;
  const failures = [...document.getElementById('search-failures').children]
    .map((item) => item.textContent);
  if (document.querySelectorAll('#search-results [data-peer-id]').length
    || !failures.includes(window.strings['search.failure.invalid'])) {
    throw new Error(`forged search: ${document.getElementById('search-status').textContent}, `
      + `failures ${failures}`);
  }
  await shot('search-error');
};
Promise.all([window.identityShown, invokeTest('ui_test_mode'), invokeTest('ui_test_existing'),
  invokeTest('ui_test_step')])
  .then(async ([state, mode, existing, step]) => {
    if (step === 'recovery') {
      await shot('recovery');
      const passed = state.status === 'recovery' && state.reason === 'missing'
        && !document.getElementById('recovery').hidden
        && document.getElementById('recovery-reason').textContent
          === window.strings['recovery.missing'];
      return invokeTest('ui_test_result', { passed, peerId: '', reason: 'recovery not shown' });
    }
    if (step === 'add') {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      await checkSettingsInventory(state.peerId, state.mode);
      document.getElementById('settings-open').click();
      await waitFor('settings');
      document.getElementById('settings-create').click();
      await waitFor('setup');
      await chooseStorage(mode);
      const addedPeer = document.getElementById('peer-id').textContent;
      await checkSettingsInventory(addedPeer, mode);
      await switchFromSettings(state.mode);
      await checkSettingsInventory(state.peerId, state.mode);
      await switchFromSettings(mode);
      await checkSettingsInventory(addedPeer, mode);
    } else if (step === 'pick') {
      if (state.status !== 'picker') throw new Error('identity picker not shown');
      await pickReady(mode);
      await checkSettingsInventory(document.getElementById('peer-id').textContent, mode);
    } else if (step === 'remove') {
      await removeOpened(state, mode);
    } else if (step === 'network') {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      await checkNetwork();
    } else if (step.startsWith('connect:')) {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      await connectNetwork(step.slice('connect:'.length));
    } else if (step === 'network-restart') {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      if (document.getElementById('seeds').value !== testSeeds) throw new Error('draft lost');
      if (document.getElementById('network-status').textContent
        !== window.strings['network.idle']) throw new Error('unchecked draft shown as active');
    } else if (step === 'node') {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      await checkNode();
    } else if (step === 'network-settings-failure') {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      await checkNetworkInSettings();
    } else if (step === 'settings-restart') {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      await checkSettingsInventory(state.peerId, state.mode);
    } else if (step === 'settings-language') {
      if (state.status !== 'setup') throw new Error('storage picker not shown');
      await chooseStorage(mode);
      await checkSettingsInventory(document.getElementById('peer-id').textContent, mode);
      await checkSettingsLanguage();
    } else if (step === 'profile-save') {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      await saveProfileForRestart();
    } else if (step === 'profile-restart') {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      await checkSavedProfile();
    } else if (step === 'profile-publish-fail') {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      await publishProfileOnLoopback(false, true);
    } else if (step === 'profile-publish-retry') {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      await publishProfileOnLoopback(true);
    } else if (step === 'search-no-network' || step === 'search') {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      await searchOnLoopback(step === 'search');
    } else if (step === 'search-forged') {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      await searchForged();
    } else if (step === 'profile-publish') {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      await publishProfileOnLoopback();
    } else if (existing && state.status !== 'ready') {
      throw new Error('identity not reopened');
    } else if (!existing) {
      if (state.status !== 'setup') throw new Error('storage picker not shown');
      await chooseStorage(mode);
      await checkEmptyProfilePreview();
    }
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    if (existing && !step) await shot('ready');
    const ready = document.getElementById('app-ready');
    const rect = ready.getBoundingClientRect();
    const style = getComputedStyle(ready);
    const { strings } = window;
    const peerId = document.getElementById('peer-id').textContent;
    const peer = document.getElementById('peer-id');
    const originalBodyWidth = document.body.style.width;
    const peerLinesAt = (width) => {
      document.body.style.width = `${width}px`;
      const range = document.createRange();
      range.selectNodeContents(peer);
      return range.getClientRects().length;
    };
    const narrowLines = peerLinesAt(350);
    const wideLines = peerLinesAt(960);
    document.body.style.width = originalBodyWidth;
    const backup = document.getElementById('file-backup');
    const keyPath = document.getElementById('key-path').textContent;
    const backupMatches = mode === 'file'
      ? !backup.hidden && /[\\/]identity\.key$/.test(keyPath)
        && backup.querySelector('p').textContent === strings['storage.file.backup']
      : backup.hidden && keyPath === '';
    const checks = {
      backupMatches,
      title: document.title === strings['app.name'],
      heading: document.querySelector('h1').textContent === strings['app.name'],
      readyText: ready.textContent === strings['status.ready'],
      locale: strings['status.ready'] === (window.pseudoLocale ? '[Réádý··]'
        : window.locale === 'ru' ? 'Готово' : 'Ready'),
      dimensions: rect.width > 0 && rect.height > 0,
      visibility: style.visibility === 'visible' && style.opacity !== '0',
      onTop: document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2) === ready,
      peerId: /^[0-9a-f]{64}$/.test(peerId),
      peerIdReflows: narrowLines > 1 && wideLines === 1,
      hiddenNotRendered: hiddenRendered().length === 0,
      noIdentityActions: document.getElementById('switch-identity').hidden
        && document.getElementById('remove-identity').hidden,
    };
    const failed = Object.entries(checks).filter(([, ok]) => !ok).map(([name]) => name);
    return invokeTest('ui_test_result', {
      passed: failed.length === 0, peerId, reason: failed.join(','),
    });
  })
  .catch(fail);
