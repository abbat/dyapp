// Desktop UI smoke: identity, search, safety and early generation checks.
const waitFor = (id) =>
  new Promise((resolve, reject) => {
    const section = document.getElementById(id);
    const shown = () => !section.hidden && (section.tagName !== 'DIALOG' || section.open);
    if (shown()) {
      resolve();
      return;
    }
    const observer = new MutationObserver(() => {
      if (shown()) {
        observer.disconnect();
        clearTimeout(timeout);
        resolve();
      }
    });
    observer.observe(section, { attributes: true, attributeFilter: ['hidden', 'open'] });
    const timeout = setTimeout(() => {
      observer.disconnect();
      reject(new Error(`${id} not shown`));
    }, 10000);
  });
// Opens a sidebar area of the shell and waits until it shows.
const openArea = async (area, pane = null) => {
  const button = document.querySelector(`#sidebar [data-area="${area}"]`);
  if (!button) throw Error(`Sidebar destination ${area} is missing`);
  button.click();
  await waitFor(area);
  if (area === 'settings') {
    await appApi.settingsShown;
    if (pane) openSettings(pane);
  }
};
const openSettings = (pane) => {
  const root = document.getElementById('settings-root');
  for (let depth = 0; root.hidden && depth < 2; depth += 1)
    document.getElementById('settings-back').click();
  if (root.hidden) throw Error('Settings Back did not return to root');
  const parent = {
    identities: 'account',
    devices: 'account',
    blocked: 'privacy',
    passed: 'privacy',
  };
  for (const name of [parent[pane], pane].filter(Boolean)) {
    const button = [...document.querySelectorAll(`[data-settings-open="${name}"]`)].find(
      (item) => !item.closest('[hidden]'),
    );
    if (!button) throw Error(`Settings destination ${name} is not visible`);
    button.click();
  }
};
// The hidden attribute must remove an element from the layout, whatever its display rule.
const hiddenRendered = () =>
  [...document.querySelectorAll('[hidden]')]
    .filter((element) => getComputedStyle(element).display !== 'none')
    .map((element) => element.id || element.tagName);
// The short Peer ID rule: "@" and the last 6 characters, one more per shared tail, and a copy
// of the same ID in another slot is no collision.
const shortPeerIdRule = () => {
  const short = appApi.shortPeerId;
  const a = `${'0'.repeat(56)}12abcdef`;
  const b = `${'1'.repeat(56)}92abcdef`;
  const c = `${'2'.repeat(56)}12abcdef`;
  return (
    short(a, [a]) === '@abcdef' &&
    short(a, [a, a]) === '@abcdef' &&
    short(a, [a, b]) === '@12abcdef' &&
    short(b, [a, b]) === '@92abcdef' &&
    short(a, [a, b, c]) === '@012abcdef'
  );
};
// First run: no identity actions, the recommended available store preselected, and a storage
// choice alone creates nothing until Create.
const assertSetupIdentityActions = () => {
  const rendered = hiddenRendered();
  if (rendered.length) throw new Error(`hidden but rendered: ${rendered}`);
  if (
    !document.getElementById('switch-identity').hidden ||
    !document.getElementById('remove-identity').hidden
  )
    throw new Error('identity actions on setup');
};
const assertStorageRecommendation = () => {
  const system = document.getElementById('system-choice');
  if (!document.getElementById(system.disabled ? 'file-choice' : 'system-choice').checked) {
    throw new Error('recommended storage not preselected');
  }
  if (system.disabled && !document.getElementById('system-reason').textContent) {
    throw new Error('system storage reason missing');
  }
  if (
    !document.getElementById('no-backup').textContent ||
    !document.getElementById('setup-step').textContent
  )
    throw new Error('step or backup line missing');
  // Only the first run links here; an open identity links from Settings.
  if (
    document.getElementById('link-identity').hidden !==
    !document.getElementById('setup-cancel').hidden
  )
    throw new Error('link offer on the wrong setup');
};
const chooseStorage = async (mode, seed) => {
  const choice = document.getElementById(`${mode}-choice`);
  if (!choice || choice.disabled || choice.hidden) throw new Error('storage choice unavailable');
  assertSetupIdentityActions();
  assertStorageRecommendation();
  await smoke.shot('setup');
  const before = (await invokeTest('list_identities')).length;
  choice.click();
  if (
    (await invokeTest('list_identities')).length !== before ||
    document.getElementById('setup').hidden
  )
    throw new Error('storage choice created an identity');
  document.getElementById('create-identity').click();
  await waitFor('wizard-network');
  // Step 2: the full Peer ID with Copy and the Network form, which can be skipped.
  const peerCode = document.getElementById('peer-id');
  if (
    !/^[0-9a-f]{64}$/.test(peerCode.textContent) ||
    !document.getElementById('wizard-peer').contains(peerCode) ||
    !document.getElementById('wizard-form').contains(document.getElementById('network')) ||
    document.getElementById('skip-network').hidden
  )
    throw new Error('wizard network step incomplete');
  const rendered2 = hiddenRendered();
  if (rendered2.length) throw new Error(`hidden but rendered: ${rendered2}`);
  await checkCopy('copy-peer-id', peerCode);
  await smoke.shot('wizard-network');
  if (seed) await connectNetwork(seed);
  else document.getElementById('skip-network').click();
  await waitFor('identity');
  if (
    !document.getElementById('profile-area').contains(peerCode) ||
    !document.getElementById('skip-network').hidden
  )
    throw new Error('wizard did not hand back');
};
// Copy puts the text of `source` on the clipboard and says so on the button.
const checkCopy = async (id, source) => {
  if (!navigator.clipboard) throw new Error('no clipboard API');
  const copied = [];
  navigator.clipboard.writeText = (text) => {
    copied.push(text);
    return Promise.resolve();
  };
  const button = document.getElementById(id);
  button.click();
  try {
    await appApi.copied;
  } finally {
    delete navigator.clipboard.writeText;
  }
  if (
    copied.join() !== source.textContent ||
    button.textContent !== appApi.strings['action.copied']
  )
    throw new Error(`${id} did not copy`);
};
const pickReady = async (mode) => {
  const pick = document.querySelector(
    `#identities button[data-status="ready"][data-mode="${mode}"]`,
  );
  if (!pick) {
    const shown = [...document.querySelectorAll('#identities button')].map(
      (button) => `${button.dataset.status}/${button.dataset.mode}`,
    );
    throw new Error(`no ready ${mode} identity in the picker: ${shown}`);
  }
  await smoke.shot('picker');
  pick.click();
  await waitFor('identity');
};
const checkSettingsInventory = async (peerId, mode) => {
  await openArea('settings', 'identities');
  const expected = await invokeTest('list_identities');
  const list = document.getElementById('settings-identities');
  await new Promise((resolve, reject) => {
    if (list.children.length === expected.length) {
      resolve();
      return;
    }
    const observer = new MutationObserver(() => {
      if (list.children.length === expected.length) {
        observer.disconnect();
        clearTimeout(timeout);
        resolve();
      }
    });
    observer.observe(list, { childList: true });
    const timeout = setTimeout(() => {
      observer.disconnect();
      reject(new Error('Settings inventory did not load'));
    }, 10000);
  });
  await smoke.shot('settings');
  const active = [...list.children].filter((item) => item.dataset.active === 'true');
  const short = appApi.shortPeerId(peerId, expected.map((item) => item.peerId).filter(Boolean));
  if (
    active.length !== 1 ||
    active[0].dataset.peerId !== peerId ||
    !active[0].textContent.includes(short) ||
    !active[0].textContent.includes(appApi.strings[`identity.mode.${mode}`])
  ) {
    throw new Error('Settings marked the wrong active identity or storage mode');
  }
  await openArea('profile-area');
};
const switchFromSettings = async (mode) => {
  await openArea('settings', 'identities');
  document.getElementById('settings-switch').click();
  await waitFor('picker');
  // Cancel goes back to the open identity without switching.
  document.getElementById('picker-cancel').click();
  await waitFor('identity');
  if (document.getElementById('settings').hidden) throw new Error('picker Cancel left Settings');
  openSettings('identities');
  document.getElementById('settings-switch').click();
  await waitFor('picker');
  await pickReady(mode);
};
// A pending identity response cannot render after a later transition has shown the picker.
const checkStaleRender = async () => {
  const readyState = appBindings.current;
  let finishOld;
  const old = appBindings.act(
    new Promise((resolve) => {
      finishOld = resolve;
    }),
  );
  await appBindings.act(Promise.resolve({ status: 'picker' }));
  if (document.getElementById('picker').hidden) throw new Error('picker not shown');
  finishOld({ status: 'setup' });
  await old;
  if (document.getElementById('picker').hidden) throw new Error('stale identity response rendered');
  await appBindings.act(Promise.resolve(readyState));
  if (document.getElementById('identity').hidden) throw new Error('ready view not restored');
  // A failure that arrives after a later transition stays silent.
  let failOld;
  const oldFailure = appBindings.act(
    new Promise((_, reject) => {
      failOld = reject;
    }),
  );
  await appBindings.act(Promise.resolve({ status: 'picker' }));
  failOld(new Error('refused'));
  await oldFailure;
  if (document.getElementById('picker').hidden) throw new Error('stale failure rendered');
  await appBindings.act(Promise.resolve(readyState));
  // A failed switch or removal offers a way back to the identity the backend kept.
  const retry = document.getElementById('boot-retry');
  await appBindings.act(Promise.reject(new Error('refused')));
  const actions = document.getElementById('boot-actions');
  if (
    document.getElementById('storage-error').hidden ||
    actions.hidden ||
    document.activeElement !== retry
  )
    throw new Error('failed change left no way out');
  retry.click();
  await waitFor('identity');
  if (document.getElementById('peer-id').textContent !== readyState.peerId || !actions.hidden) {
    throw new Error('retry did not reopen the kept identity');
  }
};
const { checkStaleSearchAndChats, checkStalePeerActions, checkStaleSafety } =
  smoke.uiSmokeIdentityPeer;
const {
  checkStaleProfileLoad,
  checkStaleProfileHandlers,
  checkStalePublishAndHide,
  checkStaleProfileTimers,
  checkStaleUsageStatus,
  checkStalePrivacy,
  checkStaleBlocked,
} = smoke.uiSmokeIdentityProfile;
// These fragments are concatenated into one WebView eval; expose the shared test API explicitly.
smoke.uiSmokeIdentity = Object.freeze({
  waitFor,
  openArea,
  hiddenRendered,
  shortPeerIdRule,
  chooseStorage,
  checkCopy,
  pickReady,
  checkSettingsInventory,
  switchFromSettings,
  checkStaleRender,
  checkStaleSearchAndChats,
  checkStalePeerActions,
  checkStaleSafety,
  checkStaleProfileLoad,
  checkStaleProfileHandlers,
  checkStalePublishAndHide,
  checkStaleProfileTimers,
  checkStaleUsageStatus,
  checkStalePrivacy,
  checkStaleBlocked,
});
