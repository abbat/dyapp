// Runs in the actual Tauri webview after the bundled page loads.
const invokeTest = window.__TAURI__.core.invoke;
const fail = (error) => invokeTest('ui_test_result', {
  passed: false, peerId: '', reason: String(error),
});
const waitFor = (id) => new Promise((resolve, reject) => {
  const section = document.getElementById(id);
  if (!section.hidden) { resolve(); return; }
  const observer = new MutationObserver(() => {
    if (!section.hidden) { observer.disconnect(); clearTimeout(timeout); resolve(); }
  });
  observer.observe(section, { attributes: true, attributeFilter: ['hidden'] });
  const timeout = setTimeout(() => { observer.disconnect(); reject(new Error(`${id} not shown`)); }, 10000);
});
const chooseStorage = async (mode) => {
  const choice = document.getElementById(`${mode}-choice`);
  if (!choice || choice.disabled || choice.hidden) throw new Error('storage choice unavailable');
  choice.click();
  await waitFor('identity');
};
const pickReady = async (mode) => {
  const pick = document.querySelector(
    `#identities button[data-status="ready"][data-mode="${mode}"]`);
  if (!pick) throw new Error('no ready identity in the picker');
  pick.click();
  await waitFor('identity');
};
// Removes the opened identity through both confirmations, then opens the one left.
const removeOpened = async (state, mode) => {
  const { strings } = window;
  document.getElementById('remove-identity').click();
  await waitFor('remove');
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
};
// An empty list is refused, then a refused and a malformed row keep Continue disabled.
const checkNetwork = async () => {
  if (await submitCheck('') !== 'empty') throw new Error('empty list not reported');
  const rows = await submitCheck(testSeeds);
  if (rows.join() !== 'unreachable,invalidAddress') throw new Error(`rows ${rows}`);
  if (document.getElementById('network-status').textContent
    !== window.strings['network.invalidAddress']) throw new Error('row problem not summarized');
  if (!document.getElementById('continue-connection').disabled) {
    throw new Error('continue enabled without a connected node');
  }
};
const checkEmptyProfilePreview = async () => {
  document.getElementById('profile-preview-action').click();
  await waitFor('profile-preview');
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
const saveProfileForRestart = async () => {
  document.getElementById('profile-gender').value = '1';
  document.getElementById('birth-year').value = '1990';
  document.getElementById('birth-month').value = '2';
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
  document.getElementById('profile-disclosure').checked = true;
  document.getElementById('profile-attested').checked = true;
  const result = waitForProfileStatus(
    fails ? 'profile.notConnected' : 'profile.published', fails ? '' : '1', 150000);
  document.getElementById('profile-publish').click();
  await result;
  if (document.getElementById('profile-attested').checked
    || document.getElementById('profile-disclosure').checked) {
    throw new Error('publish confirmations remained selected');
  }
};
const checkSavedProfile = async () => {
  if (document.getElementById('profile-gender').value !== '1'
    || document.getElementById('birth-year').value !== '1990'
    || document.getElementById('birth-month').value !== '2'
    || document.getElementById('birth-day').value !== '') {
    throw new Error('profile draft changed after restart');
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
  if (!document.getElementById('profile-age').textContent
    .startsWith(`${window.strings['profile.age']}: `)) {
    throw new Error('known age hidden after restart');
  }
  if (document.getElementById('profile-attested').checked) {
    throw new Error('publish attestation survived restart');
  }
};
Promise.all([window.identityShown, invokeTest('ui_test_mode'), invokeTest('ui_test_existing'),
  invokeTest('ui_test_step')])
  .then(async ([state, mode, existing, step]) => {
    if (step === 'recovery') {
      const passed = state.status === 'recovery' && state.reason === 'missing'
        && !document.getElementById('recovery').hidden
        && document.getElementById('recovery-reason').textContent
          === window.strings['recovery.missing'];
      return invokeTest('ui_test_result', { passed, peerId: '', reason: 'recovery not shown' });
    }
    if (step === 'add') {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      document.getElementById('switch-identity').click();
      await waitFor('picker');
      document.getElementById('add-identity').click();
      await waitFor('setup');
      await chooseStorage(mode);
    } else if (step === 'pick') {
      if (state.status !== 'picker') throw new Error('identity picker not shown');
      await pickReady(mode);
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
    };
    const failed = Object.entries(checks).filter(([, ok]) => !ok).map(([name]) => name);
    return invokeTest('ui_test_result', {
      passed: failed.length === 0, peerId, reason: failed.join(','),
    });
  })
  .catch(fail);
