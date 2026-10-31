// Runs in the actual Tauri webview after the bundled page loads.
const invokeTest = window.__TAURI__.core.invoke;
const fail = (error) => invokeTest('ui_test_result', {
  passed: false, peerId: '', reason: String(error),
});
const frames = () => new Promise((resolve) => {
  requestAnimationFrame(() => requestAnimationFrame(resolve));
});
// With axe-core loaded (DYAPP_AXE), checks the current screen: WCAG 2.x A/AA (labels and
// contrast included) in the light and dark themes, tab order following the DOM, a visible focus
// ring on every focusable control and no horizontal overflow at 440 px.
const audit = async (name) => {
  if (!window.axe) return;
  const problems = [];
  const root = document.documentElement;
  const theme = root.dataset.theme;
  const tags = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'];
  try {
    for (const mode of ['light', 'dark']) {
      root.dataset.theme = mode;
      await frames();
      const result = await window.axe.run(document, { runOnly: { type: 'tag', values: tags } });
      for (const v of result.violations) problems.push(`${mode} ${v.id} ${v.nodes[0].target}`);
    }
  } finally {
    if (theme) root.dataset.theme = theme; else delete root.dataset.theme;
  }
  const visible = (element) => element.getClientRects().length > 0
    && getComputedStyle(element).visibility === 'visible';
  for (const element of document.querySelectorAll('[tabindex]')) {
    if (element.tabIndex > 0 && visible(element)) {
      problems.push(`tabindex>0 ${element.id || element.tagName}`);
    }
  }
  const before = document.activeElement;
  const [scrollLeft, scrollTop] = [window.scrollX, window.scrollY];
  const focusable = 'a[href], button, input, select, textarea, [tabindex]:not([tabindex="-1"])';
  for (const element of document.querySelectorAll(focusable)) {
    if (element.disabled || !visible(element)) continue;
    element.focus();
    const style = getComputedStyle(element);
    if (document.activeElement === element && style.boxShadow === 'none'
      && style.outlineStyle === 'none') {
      problems.push(`no focus ring ${element.id || element.tagName}`);
    }
  }
  if (before?.focus) before.focus(); else document.activeElement.blur();
  const [window0] = await invokeTest('ui_test_window', { set: null });
  await invokeTest('ui_test_window', {
    set: { ...window0, width: Math.round(440 * devicePixelRatio) },
  });
  await frames();
  const overflow = root.scrollWidth - root.clientWidth;
  const wide = [...document.querySelectorAll('body *')]
    .filter((element) => element.scrollWidth > element.clientWidth + 1 && visible(element))
    .slice(-3).map((element) => element.id || element.className || element.tagName);
  await invokeTest('ui_test_window', { set: window0 });
  await frames();
  // Focusing and the narrow window scroll the page; the checks after the audit expect it as it was.
  window.scrollTo(scrollLeft, scrollTop);
  if (overflow > 0) problems.push(`overflow ${overflow}px at 440: ${wide}`);
  if (problems.length) throw new Error(`accessibility ${name}: ${problems.join('; ')}`);
};
// Audits the current screen, then saves its screenshots when the run asks for them.
const shot = async (name) => {
  await audit(name);
  await invokeTest('ui_test_shot', { name }).catch(() => {});
};
const waitFor = (id) => new Promise((resolve, reject) => {
  const section = document.getElementById(id);
  if (!section.hidden) { resolve(); return; }
  const observer = new MutationObserver(() => {
    if (!section.hidden) { observer.disconnect(); clearTimeout(timeout); resolve(); }
  });
  observer.observe(section, { attributes: true, attributeFilter: ['hidden'] });
  const timeout = setTimeout(() => { observer.disconnect(); reject(new Error(`${id} not shown`)); }, 10000);
});
// Opens a sidebar area of the shell and waits until it shows.
const openArea = async (area) => {
  document.querySelector(`#sidebar [data-area="${area}"]`).click();
  await waitFor(area);
};
// The hidden attribute must remove an element from the layout, whatever its display rule.
const hiddenRendered = () => [...document.querySelectorAll('[hidden]')]
  .filter((element) => getComputedStyle(element).display !== 'none')
  .map((element) => element.id || element.tagName);
// The short Peer ID rule: "@" and the last 6 characters, one more per shared tail, and a copy
// of the same ID in another slot is no collision.
const shortPeerIdRule = () => {
  const short = window.shortPeerId;
  const a = `${'0'.repeat(56)}12abcdef`;
  const b = `${'1'.repeat(56)}92abcdef`;
  const c = `${'2'.repeat(56)}12abcdef`;
  return short(a, [a]) === '@abcdef' && short(a, [a, a]) === '@abcdef'
    && short(a, [a, b]) === '@12abcdef' && short(b, [a, b]) === '@92abcdef'
    && short(a, [a, b, c]) === '@012abcdef';
};
// First run: no identity actions, the recommended available store preselected, and a storage
// choice alone creates nothing until Create.
const chooseStorage = async (mode, seed) => {
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
  if (!document.getElementById('no-backup').textContent
    || !document.getElementById('setup-step').textContent) throw new Error('step or backup line missing');
  await shot('setup');
  const before = (await invokeTest('list_identities')).length;
  choice.click();
  if ((await invokeTest('list_identities')).length !== before
    || document.getElementById('setup').hidden) throw new Error('storage choice created an identity');
  document.getElementById('create-identity').click();
  await waitFor('wizard-network');
  // Step 2: the full Peer ID with Copy and the Network form, which can be skipped.
  const peerCode = document.getElementById('peer-id');
  if (!/^[0-9a-f]{64}$/.test(peerCode.textContent)
    || !document.getElementById('wizard-peer').contains(peerCode)
    || !document.getElementById('wizard-form').contains(document.getElementById('network'))
    || document.getElementById('skip-network').hidden) throw new Error('wizard network step incomplete');
  const rendered2 = hiddenRendered();
  if (rendered2.length) throw new Error(`hidden but rendered: ${rendered2}`);
  await checkCopy('copy-peer-id', peerCode);
  await shot('wizard-network');
  if (seed) await connectNetwork(seed);
  else document.getElementById('skip-network').click();
  await waitFor('identity');
  if (!document.getElementById('profile-area').contains(peerCode)
    || !document.getElementById('skip-network').hidden) throw new Error('wizard did not hand back');
};
// Copy puts the text of `source` on the clipboard and says so on the button.
const checkCopy = async (id, source) => {
  if (!navigator.clipboard) throw new Error('no clipboard API');
  const copied = [];
  navigator.clipboard.writeText = (text) => { copied.push(text); return Promise.resolve(); };
  const button = document.getElementById(id);
  button.click();
  try { await window.copied; } finally { delete navigator.clipboard.writeText; }
  if (copied.join() !== source.textContent
    || button.textContent !== window.strings['action.copied']) throw new Error(`${id} did not copy`);
};
const pickReady = async (mode) => {
  const pick = document.querySelector(
    `#identities button[data-status="ready"][data-mode="${mode}"]`);
  if (!pick) {
    const shown = [...document.querySelectorAll('#identities button')]
      .map((button) => `${button.dataset.status}/${button.dataset.mode}`);
    throw new Error(`no ready ${mode} identity in the picker: ${shown}`);
  }
  await shot('picker');
  pick.click();
  await waitFor('identity');
};
const checkSettingsInventory = async (peerId, mode) => {
  await openArea('settings');
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
  const short = window.shortPeerId(peerId, expected.map((item) => item.peerId).filter(Boolean));
  if (active.length !== 1 || active[0].dataset.peerId !== peerId
      || !active[0].textContent.includes(short)
      || !active[0].textContent.includes(window.strings[`identity.mode.${mode}`])) {
    throw new Error('Settings marked the wrong active identity or storage mode');
  }
  await openArea('profile-area');
};
const switchFromSettings = async (mode) => {
  await openArea('settings');
  document.getElementById('settings-switch').click();
  await waitFor('picker');
  // Cancel goes back to the open identity without switching.
  document.getElementById('picker-cancel').click();
  await waitFor('identity');
  if (document.getElementById('settings').hidden) throw new Error('picker Cancel left Settings');
  document.getElementById('settings-switch').click();
  await waitFor('picker');
  await pickReady(mode);
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
  if (restarted) return;
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
// Settings → Storage: the node store is Unavailable until the node first runs, then sized.
const checkUsage = async (limit) => {
  const { strings } = window;
  await openArea('profile-area');
  await openArea('settings');
  await window.usageShown;
  const text = (id) => document.getElementById(id).textContent;
  const bar = document.getElementById('usage-node-bar');
  if (document.getElementById('usage').hidden || !text('usage-identity-dir')
    || text('usage-identity-dir') === strings['usage.unavailable']
    || text('usage-messages') === strings['usage.unavailable']
    || !text('usage-messages-path').endsWith('client.db')
    || !text('usage-node-path').endsWith('node')
    || document.getElementById('usage-free-row').hidden) {
    throw new Error('Storage card misses a path or a known size');
  }
  if (limit ? bar.hidden || bar.max !== limit || !(bar.value > 0)
      || text('usage-node') === strings['usage.unavailable']
    : !bar.hidden || text('usage-node') !== strings['usage.unavailable']) {
    throw new Error(`Storage card shows the node store wrong: ${text('usage-node')}`);
  }
};
// Settings → Storage on every platform: the store sits in the identity folder and has a size; a
// row the app cannot fill is hidden, and no size reads as a made-up zero.
const checkStorageCard = async () => {
  await openArea('settings');
  await window.usageShown;
  const text = (id) => document.getElementById(id).textContent;
  const shown = (id) => !document.getElementById(id).hidden;
  const dir = text('usage-identity-dir');
  const sep = dir.includes('\\') ? '\\' : '/';
  const path = text('usage-messages-path');
  // Windows has no node.
  const windows = navigator.userAgent.includes('Windows');
  const sizes = ['usage-messages', 'usage-node', 'usage-free'];
  if (!/^(\/|[A-Za-z]:\\)/.test(dir) || !path.startsWith(`${dir}${sep}`)
    || !path.endsWith(`${sep}client.db`)
    || text('usage-messages') === window.strings['usage.unavailable']
    || shown('usage-node-row') === windows || !shown('usage-free-row')
    || sizes.some((id) => /^0\s/.test(text(id)))) {
    throw new Error(`Storage card wrong: ${dir} ${path} ${sizes.map(text).join(' | ')}`);
  }
  await shot('storage');
  await openArea('profile-area');
};
// The node starts on the loopback network, then stops; bad limits never reach the app.
const checkNode = async () => {
  const { strings } = window;
  await checkUsage(0);
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
  await checkUsage(64 << 20);
  await shot('settings-storage');
  if (await saveNode(false, '64') !== 'off') throw new Error('node did not stop');
  await openArea('profile-area');
};
const byId = (id) => document.getElementById(id);
const publicFieldList = () => [...document.querySelectorAll('#profile-public-fields li')]
  .map((item) => item.textContent);
const openPublishForTest = async () => {
  byId('profile-publish').click();
  await waitFor('publish-dialog');
  if (byId('profile-attested').checked || byId('profile-disclosure').checked
    || !byId('profile-publish-confirm').disabled) {
    throw new Error('publish consents were preselected');
  }
  if (document.activeElement !== byId('profile-cancel')) throw new Error('Cancel not focused');
  return publicFieldList();
};
const consent = (id) => {
  byId(id).checked = true;
  byId(id).dispatchEvent(new Event('change'));
};
const closePublishDialog = async () => {
  byId('profile-cancel').click();
  await new Promise((resolve) => setTimeout(resolve, 50));
  if (!byId('publish-dialog').hidden) throw new Error('Cancel left the publish dialog open');
};
const checkEmptyProfilePreview = async () => {
  await shot('profile-edit');
  if (!byId('profile-hidden').disabled) throw new Error('Hide from search offered before a publish');
  const fields = await openPublishForTest();
  await shot('publish-dialog');
  if (fields.join('|') !== window.strings['profile.empty']) throw new Error('unset fields appeared in preview');
  if (byId('profile-age').textContent !== window.strings['profile.ageUnknown']) {
    throw new Error('unknown age not shown');
  }
  if (!byId('profile-date-warning').hidden) throw new Error('full-date warning shown without a birth date');
  consent('profile-attested');
  if (!byId('profile-publish-confirm').disabled) throw new Error('Publish enabled with one box');
  consent('profile-disclosure');
  if (byId('profile-publish-confirm').disabled) throw new Error('Publish disabled with both boxes');
  await closePublishDialog();
  if (byId('profile-status').textContent !== window.strings['profile.saved']) {
    throw new Error('Cancel lost the draft');
  }
  await openPublishForTest();
  await closePublishDialog();
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
const expectedFields = () => [
  `${window.strings['profile.field.gender']}: ${window.strings['profile.value.gender.Woman']}`,
  `${window.strings['profile.birthYear']}: 1990`,
  `${window.strings['profile.birthMonth']}: 2`,
].join('|');
const publishProfileOnLoopback = async (saved = false, fails = false) => {
  if (saved) {
    if (byId('profile-gender').value !== '1' || byId('birth-year').value !== '1990'
      || byId('birth-month').value !== '2') {
      throw new Error('failed publish lost the saved draft');
    }
  } else {
    byId('profile-gender').value = '1';
    byId('birth-year').value = '1990';
    byId('birth-month').value = '2';
  }
  const fields = await openPublishForTest();
  if (fields.join('|') !== expectedFields()) throw new Error(`profile preview changed: ${fields}`);
  consent('profile-disclosure');
  consent('profile-attested');
  const result = waitForProfileStatus(
    fails ? 'profile.notConnected' : 'profile.published', fails ? '' : '1', 150000);
  byId('profile-publish-confirm').click();
  await waitFor('publish-progress');
  if (!/\d:\d\d/.test(byId('publish-elapsed').textContent)) {
    throw new Error(`elapsed time not shown: ${byId('publish-elapsed').textContent}`);
  }
  await result;
  await new Promise((resolve) => setTimeout(resolve, 50));
  if (!byId('publish-dialog').hidden) throw new Error('publish dialog stayed open');
  await shot(fails ? 'profile-publish-failed' : 'profile-published');
  if (byId('profile-open-network').hidden !== !fails) {
    throw new Error(`profile Open Network shown ${fails ? 'without' : 'after'} a failure`);
  }
  if (byId('profile-hidden').disabled !== fails || byId('profile-hidden').checked) {
    throw new Error('Hide from search not offered after a publish');
  }
};
// The switch publishes a tombstone, then the notice says how to return; the draft stays.
const hideProfileOnLoopback = async () => {
  const hidden = byId('profile-hidden');
  if (hidden.disabled || hidden.checked) throw new Error('Hide from search not offered');
  const result = waitForProfileStatus('profile.status.hidden', '', 60000);
  hidden.checked = true;
  hidden.dispatchEvent(new Event('change'));
  await result;
  await shot('profile-hidden');
  if (!hidden.checked || byId('profile-hidden-notice').hidden
    || byId('profile-gender').value !== '1') {
    throw new Error('hidden profile not shown with its notice and draft');
  }
  await openArea('search-area');
  byId('run-search').click(); await window.searchDone;
  if (!shown('search-empty') || document.querySelector('#search-results [data-peer-id]')) throw new Error('hidden profile remains in search');
};
const checkSavedProfile = async () => {
  if (byId('profile-gender').value !== '1' || byId('birth-year').value !== '1990'
    || byId('birth-month').value !== '2' || byId('birth-day').value !== ''
    || byId('profile-country').value !== 'DE') {
    throw new Error('profile draft changed after restart');
  }
  const place = byId('profile-place').value;
  if (!place.startsWith(window.locale === 'ru' ? 'Берлин, ' : 'Berlin, ')) {
    throw new Error(`picked place lost after restart: ${place}`);
  }
  const placeFields = (await openPublishForTest()).filter((field) => field.startsWith(window.strings['profile.country'])
    || field.startsWith(window.strings['profile.place']));
  if (placeFields.join('|') !== [
    `${window.strings['profile.country']}: ${new Intl.DisplayNames([window.locale], { type: 'region' }).of('DE')}`,
    `${window.strings['profile.place']}: ${place}`,
  ].join('|')) throw new Error(`place preview changed: ${placeFields}`);
  await closePublishDialog();
  // Clearing the country clears the place; the later publish steps expect neither.
  const country = byId('profile-country');
  country.value = '';
  country.dispatchEvent(new Event('input', { bubbles: true }));
  if (byId('profile-place').value !== '') throw new Error('place outlived its country');
  const cleared = waitForProfileStatus('profile.saved');
  byId('profile-save').click();
  await cleared;
  const fields = await openPublishForTest();
  if (fields.join('|') !== expectedFields()) throw new Error(`profile preview changed: ${fields}`);
  await closePublishDialog();
  if (!byId('profile-age').textContent.startsWith(`${window.strings['profile.age']}: `)) {
    throw new Error('known age hidden after restart');
  }
};
// The status bar: traffic is unavailable until a search starts the client swarm, then counts the
// reply; the message store always has a size once the identity is open.
// Removal withdraws a published profile first. With the node stopped no node takes the tombstone:
// Retry fails again and Cancel keeps the identity.
const removeOfflineIdentity = async () => {
  await openArea('settings');
  byId('settings-remove').click();
  await waitFor('remove');
  if (!shown('remove-text') || !shown('remove-ask') || shown('remove-offline')) {
    throw new Error('removal does not say the profile is withdrawn first');
  }
  byId('remove-confirm').click();
  byId('remove-action').click();
  await window.removeDone;
  if (!shown('remove-offline') || shown('remove-ask')
    || document.activeElement.id !== 'remove-cancel-offline') {
    throw new Error('no offline choice when the tombstone failed');
  }
  await shot('remove-offline');
  byId('remove-retry').click();
  await window.removeDone;
  if (!shown('remove-offline') || byId('remove-retry').disabled) {
    throw new Error('Retry did not offer the offline choice again');
  }
  byId('remove-cancel-offline').click();
  if (byId('remove').open || !shown('settings')) throw new Error('Cancel did not keep Settings');
};
// With the node back the tombstone goes first, then the key; a new identity opens for the checks.
const removeWithTombstone = async (mode) => {
  await openArea('settings');
  byId('settings-remove').click();
  await waitFor('remove');
  byId('remove-confirm').click();
  byId('remove-action').click();
  await window.removeDone;
  if (shown('remove-offline')) throw new Error('removal stopped although a node answered');
  await waitFor('setup');
  await chooseStorage(mode);
};
const checkStatus = async (traffic) => {
  const { strings } = window;
  window.statusShown = showStatus();
  await window.statusShown;
  const text = (id) => document.getElementById(id).textContent;
  const [, received] = text('status-traffic').match(/(\d[\d.,]*)\s/) ?? [];
  if (traffic ? !received || !(Number.parseFloat(received.replace(',', '.')) > 0)
    : text('status-traffic') !== strings['statusbar.noTraffic']) {
    throw new Error(`status bar traffic: ${text('status-traffic')}`);
  }
  if (!text('status-messages') || text('status-messages') === strings['statusbar.noMessages']) {
    throw new Error(`status bar messages: ${text('status-messages')}`);
  }
  // At the window's minimum width the items wrap instead of running off the bar.
  const bar = document.getElementById('statusbar');
  const width = document.body.style.width;
  document.body.style.width = '400px';
  const overflows = bar.scrollWidth > bar.clientWidth;
  document.body.style.width = width;
  if (overflows) throw new Error('status bar overflows at 400 px');
};
// Without a network the search asks to connect, explains it with details and offers the Network
// area; on the loopback node it returns the sample, the published profile alone, with the node's
// filters and no failures. The card opens the profile and Back returns to the same list. The
// profile has no place, so filtering by Berlin finds nobody.
const shown = (id) => !document.getElementById(id).hidden;
const searchFailures = () => document.getElementById('search-failures').textContent;
const searchOnLoopback = async (connected) => {
  await checkStatus(false);
  await openArea('search-area');
  document.getElementById('run-search').click();
  await window.searchDone;
  const status = document.getElementById('search-status').textContent;
  if (!connected) {
    if (status !== window.strings['search.notConnected'] || !shown('search-error')
      || !shown('search-open-network') || searchFailures() !== 'notConnected') {
      throw new Error(`search without a network: ${status}, details ${searchFailures()}`);
    }
    await shot('search-not-connected');
    return;
  }
  const peers = [...document.querySelectorAll('#search-results [data-peer-id]')]
    .map((item) => item.dataset.peerId);
  if (status !== `${window.strings['search.results']} 1` || shown('search-error')
    || shown('search-empty') || !shown('search-again') || !shown('search-filters')
    || peers.join() !== document.getElementById('peer-id').textContent) {
    throw new Error(`loopback search: ${status}, peers ${peers}, failures ${searchFailures()}`);
  }
  await shot('search-results');
  document.getElementById('result-1').click();
  if (shown('search-main') || !shown('profile-view')
    || document.activeElement.id !== 'view-back'
    || !document.getElementById('view-title').textContent) {
    throw new Error('the result card did not open the profile');
  }
  await shot('search-profile');
  document.getElementById('view-back').click();
  if (!shown('search-main') || shown('profile-view')
    || document.activeElement.id !== 'result-1'
    || document.querySelectorAll('#search-results [data-peer-id]').length !== 1) {
    throw new Error('Back did not return to the same results');
  }
  await checkStatus(true);
  await shot('statusbar');
  if (document.getElementById('search-place-row').hidden) throw new Error('place filter not offered');
  await pickPlace('search');
  document.getElementById('run-search').click();
  await window.searchDone;
  const placed = document.getElementById('search-status').textContent;
  if (placed !== window.strings['search.empty'] || !shown('search-empty')) {
    throw new Error(`place search: ${placed}`);
  }
  await shot('search-empty');
  document.getElementById('search-country').value = '';
  document.getElementById('search-country').dispatchEvent(new Event('input', { bubbles: true }));
};
// The fixture forged the signature of the published profile in the node's index: the search drops
// it and explains the forged reply under Details instead of showing a person.
const searchForged = async () => {
  await openArea('search-area');
  document.getElementById('run-search').click();
  await window.searchDone;
  if (document.querySelectorAll('#search-results [data-peer-id]').length || !shown('search-error')
    || !searchFailures().split('\n').includes(window.strings['search.failure.invalid'])) {
    throw new Error(`forged search: ${document.getElementById('search-status').textContent}, `
      + `failures ${searchFailures()}`);
  }
  document.querySelector('#search-error details').open = true;
  await shot('search-error');
};
Promise.all([window.identityShown, invokeTest('ui_test_mode'), invokeTest('ui_test_existing'),
  invokeTest('ui_test_step')])
  .then(async ([state, mode, existing, step]) => {
    // Steps that run one check on the reopened identity.
    const readySteps = {
      network: checkNetwork,
      node: checkNode,
      'network-settings-failure': checkNetworkInSettings,
      'settings-privacy-restart': () => checkPrivacy(true),
      'remove-offline': removeOfflineIdentity,
      'remove-tombstone': () => removeWithTombstone(mode),
      'profile-save': saveProfileForRestart,
      'profile-restart': checkSavedProfile,
      'profile-publish-fail': () => publishProfileOnLoopback(false, true),
      'profile-publish-retry': () => publishProfileOnLoopback(true),
      'search-forged': searchForged,
      'profile-publish': () => publishProfileOnLoopback(),
      'profile-hide': hideProfileOnLoopback,
      'search-no-network': () => searchOnLoopback(false),
      search: () => searchOnLoopback(true),
    };
    if (step.startsWith('recovery:')) {
      // A localized reason first; the code and slot row only in collapsed Details, with Copy.
      const reason = step.slice('recovery:'.length);
      const more = document.getElementById('recovery-more');
      const details = document.getElementById('recovery-details');
      if (state.status !== 'recovery' || state.reason !== reason
        || document.getElementById('recovery').hidden
        || document.getElementById('recovery-reason').textContent
          !== window.strings[`recovery.${reason}`]) throw new Error(`recovery ${reason} not shown`);
      if (more.open || !details.textContent.startsWith(`${reason}\n${state.slot}`)) {
        throw new Error(`recovery details: ${more.open} ${details.textContent}`);
      }
      await shot('recovery');
      more.open = true;
      await checkCopy('copy-recovery-details', details);
      await shot('recovery-details');
      return invokeTest('ui_test_result', { passed: true, peerId: '', reason: '' });
    }
    if (step.startsWith('window:')) {
      // save: move the window and quit with Ctrl+Q; restore: the saved bounds are back;
      // centre: an off-screen saved window lands in the middle of the primary display.
      const saved = { x: 120, y: 80, width: 900, height: 640 };
      const kind = step.slice('window:'.length);
      if (kind === 'save') {
        const [at] = await invokeTest('ui_test_window', { set: saved });
        if (JSON.stringify(at) !== JSON.stringify(saved)) throw new Error(`window not moved: ${JSON.stringify(at)}`);
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'q', ctrlKey: true }));
        return undefined;
      }
      const [at, primary] = await invokeTest('ui_test_window');
      const centred = {
        x: primary.x + Math.floor((primary.width - at.width) / 2),
        y: primary.y + Math.floor((primary.height - at.height) / 2),
        width: at.width,
        height: at.height,
      };
      const expected = kind === 'restore' ? saved : centred;
      if (JSON.stringify(at) !== JSON.stringify(expected)) {
        throw new Error(`window ${kind}: ${JSON.stringify(at)}, expected ${JSON.stringify(expected)}`);
      }
      return invokeTest('ui_test_result', { passed: true, peerId: '', reason: '' });
    }
    if (step.startsWith('tray:')) {
      // quit: the tray host clicks Quit; none: no host, the settings are off with a note;
      // close: turn on close-to-tray, close, and the host's Open brings the window back;
      // persisted: the setting survived a relaunch; vanish: the host goes away, the window returns.
      const kind = step.slice('tray:'.length);
      if (kind === 'quit') return undefined;
      const labels = [window.strings['tray.open'], window.strings['tray.quit']];
      const tray = async (check) => {
        for (let i = 0; i < 30; i += 1) {
          const now = await invokeTest('set_tray', { prefs: null, labels });
          if (check(now)) return;
          await new Promise((resolve) => { setTimeout(resolve, 500); });
        }
        throw new Error(`tray ${kind}: ${JSON.stringify(await invokeTest('set_tray', { prefs: null, labels }))}`);
      };
      if (state.status === 'setup') await chooseStorage(mode);
      if (kind !== 'none') await tray((now) => now.available);
      await openArea('settings');
      await window.trayShown;
      const show = document.getElementById('tray-show');
      const close = document.getElementById('tray-close');
      if (kind === 'none') {
        if (!show.disabled || !close.disabled || document.getElementById('tray-note').hidden) {
          throw new Error('tray settings usable without a tray host');
        }
        await shot('tray-none');
        return invokeTest('ui_test_result', { passed: true, peerId: '', reason: '' });
      }
      if (kind === 'close') {
        if (!show.checked || close.checked || close.disabled) throw new Error('tray defaults wrong');
        close.click();
        await shot('tray');
      } else if (!close.checked) throw new Error('close-to-tray not restored');
      await tray((now) => now.closeToTray);
      await invokeTest('ui_test_hide', { close: true });
      if (kind === 'vanish') await tray((now) => !now.available);
      return invokeTest('ui_test_result', { passed: true, peerId: '', reason: '' });
    }
    if (step === 'single') {
      // A second launch, started by the smoke once the window is hidden, shows it again.
      await invokeTest('ui_test_hide', { close: false });
      return invokeTest('ui_test_result', { passed: true, peerId: '', reason: '' });
    }
    if (step.startsWith('wizard-connect:')) {
      if (state.status !== 'setup') throw new Error('storage picker not shown');
      await chooseStorage(mode, step.slice('wizard-connect:'.length));
    } else if (step === 'add') {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      await checkSettingsInventory(state.peerId, state.mode);
      await openArea('settings');
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
    } else if (step.startsWith('connect:')) {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      await connectNetwork(step.slice('connect:'.length));
    } else if (step.startsWith('launch:')) {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      await window.connectionShown;
      if (connection() !== step.slice('launch:'.length)) {
        throw new Error(`launch shows ${connection()}`);
      }
    } else if (step.startsWith('cancel:')) {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      await cancelNetwork(step.slice('cancel:'.length));
    } else if (step.startsWith('wrong:')) {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      await checkWrongNetwork(step.slice('wrong:'.length));
    } else if (step === 'network-restart') {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      if (document.getElementById('seeds').value !== testSeeds) throw new Error('draft lost');
      if (document.getElementById('network-status').textContent
        !== window.strings['network.idle']) throw new Error('unchecked draft shown as active');
      await window.connectionShown;
      if (connection() !== 'offline') throw new Error(`no active network shows ${connection()}`);
    } else if (step === 'settings-restart') {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      await checkSettingsInventory(state.peerId, state.mode);
      await checkStorageCard();
      await checkStatus(false);
    } else if (step === 'settings-language') {
      if (state.status !== 'setup') throw new Error('storage picker not shown');
      await chooseStorage(mode);
      await checkSettingsInventory(document.getElementById('peer-id').textContent, mode);
      await checkSettingsLanguage();
      await checkPrivacy(false);
    } else if (Object.hasOwn(readySteps, step)) {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      await readySteps[step]();
    } else if (existing && state.status !== 'ready') {
      throw new Error('identity not reopened');
    } else if (!existing) {
      if (state.status !== 'setup') throw new Error('storage picker not shown');
      await chooseStorage(mode);
      await checkEmptyProfilePreview();
    }
    if (!document.getElementById('identity').hidden) await openArea('profile-area');
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    if (existing && !step) await shot('ready');
    const ready = document.getElementById('app-ready');
    const area = document.querySelector('#profile-area h1');
    const rect = area.getBoundingClientRect();
    const style = getComputedStyle(area);
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
      heading: document.querySelector('.topbar .title').textContent === strings['app.name']
        && area.textContent === strings['nav.profile'],
      readyText: ready.textContent === strings['status.ready'],
      locale: strings['status.ready'] === (window.pseudoLocale ? '[Réádý··]'
        : window.locale === 'ru' ? 'Готово' : 'Ready'),
      dimensions: rect.width > 0 && rect.height > 0,
      visibility: style.visibility === 'visible' && style.opacity !== '0',
      onTop: document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2) === area,
      peerId: /^[0-9a-f]{64}$/.test(peerId),
      shortPeerId: shortPeerIdRule(),
      peerIdReflows: narrowLines > 1 && wideLines === 1,
      hiddenNotRendered: hiddenRendered().length === 0,
      noIdentityActions: document.getElementById('switch-identity').hidden
        && document.getElementById('remove-identity').hidden,
    };
    const failed = Object.entries(checks).filter(([, ok]) => !ok).map(([name]) => name);
    if (!checks.onTop) {
      const top = document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2);
      failed.push(`covered by ${top?.id || top?.className || top?.tagName} at ${Math.round(rect.x)}`
        + ` ${Math.round(rect.y)} in ${innerWidth}x${innerHeight} scroll ${scrollY}`);
    }
    return invokeTest('ui_test_result', {
      passed: failed.length === 0, peerId, reason: failed.join(','),
    });
  })
  .catch(fail);
