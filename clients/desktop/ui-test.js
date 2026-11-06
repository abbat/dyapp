// Desktop UI smoke: network, profile, connected fixtures and step dispatch.
const identityChecks = window.timedChecks(globalThis.uiSmokeIdentity);
const chatChecks = window.timedChecks(globalThis.uiSmokeChats);
const presenceChecks = window.timedChecks(globalThis.uiSmokePresence);
const nameChecks = window.timedChecks(globalThis.uiSmokeNames);
const photoChecks = window.timedChecks(globalThis.uiSmokePhotos);
const guardChecks = window.timedChecks(globalThis.uiSmokeGuards);
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
  document.getElementById('profile-height').value = '180';
  document.getElementById('profile-smoking').value = '10';
  document.getElementById('profile-about').value = ABOUT;
  fillSeeking();
  const place = document.getElementById('profile-place');
  place.value = 'Nowhere';
  place.dispatchEvent(new Event('input', { bubbles: true }));
  await window.placesShown;
  if (place.checkValidity()) throw new Error('unlisted place accepted');
  await openArea('network-area');
  await openArea('profile-area');
  if (document.getElementById('profile-gender').value !== '1'
    || document.getElementById('birth-year').value !== '1990'
    || document.getElementById('birth-month').value !== '2'
    || place.value !== 'Nowhere' || place.checkValidity()) {
    throw new Error('unsaved profile fields changed after section navigation');
  }
  await pickPlace();
  const saved = waitForProfileStatus('profile.saved');
  document.getElementById('profile-save').click();
  await saved;
};
const ABOUT = 'First line\n<b>not bold</b>';
const fillSeeking = () => {
  byId('profile-seeking-gender-1').checked = true;
  byId('profile-seeking-age-from').value = '30';
  byId('profile-seeking-age-to').value = '60';
};
const seekingKept = () => byId('profile-seeking-gender-1').checked && !byId('profile-seeking-gender-2').checked
  && byId('profile-seeking-age-from').value === '30' && byId('profile-seeking-age-to').value === '60'
  && !byId('profile-not-looking').checked;
const expectedFields = (photos = 0) => [
  `${window.strings['profile.field.gender']}: ${window.strings['profile.value.gender.Woman']}`,
  `${window.strings['profile.field.smoking']}: ${window.strings['profile.value.smoking.No']}`,
  `${window.strings['profile.birthYear']}: 1990`,
  `${window.strings['profile.birthMonth']}: 2`,
  `${window.strings['profile.height']}: 180`,
  `${window.strings['profile.about']}: ${ABOUT}`,
  `${window.strings['profile.seekingGenders']}: ${window.strings['profile.value.gender.Woman']}`,
  `${window.strings['profile.seekingAgeFrom']}: 30`,
  `${window.strings['profile.seekingAgeTo']}: 60`,
  ...(photos ? [`${window.strings['profile.photos']}: ${photos}`] : []),
].join('|');
const publishProfileOnLoopback = async (saved = false, fails = false, photos = 0) => {
  if (saved) {
    if (byId('profile-gender').value !== '1' || byId('birth-year').value !== '1990'
      || byId('birth-month').value !== '2' || byId('profile-height').value !== '180'
      || byId('profile-smoking').value !== '10' || byId('profile-about').value !== ABOUT || !seekingKept()) {
      throw new Error('failed publish lost the saved draft');
    }
  } else {
    byId('profile-gender').value = '1';
    byId('birth-year').value = '1990';
    byId('birth-month').value = '2';
    byId('profile-height').value = '180';
    byId('profile-smoking').value = '10';
    byId('profile-about').value = ABOUT;
    fillSeeking();
  }
  const fields = await openPublishForTest();
  if (fields.join('|') !== expectedFields(photos)) throw new Error(`profile preview changed: ${fields}`);
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
const unhideProfileOnLoopback = async () => {
  await openArea('profile-area');
  const hidden = byId('profile-hidden');
  if (!hidden.checked || byId('profile-gender').value !== '1') {
    throw new Error('saved hidden draft missing before unhide');
  }
  hidden.checked = false;
  hidden.dispatchEvent(new Event('change'));
  await waitFor('publish-dialog');
  consent('profile-disclosure');
  consent('profile-attested');
  const published = waitForProfileStatus('profile.published', '3', 150000);
  byId('profile-publish-confirm').click();
  await published;
  if (hidden.checked || !byId('profile-hidden-notice').hidden
    || byId('profile-search-wait').hidden || byId('profile-gender').value !== '1') {
    throw new Error('unhide did not publish the saved draft with search pending');
  }
  await shot('profile-unhidden-pending');
};
// The switch locks the choices and publishes only itself; a reversed age range is never saved.
const checkSeekingForm = async () => {
  const paused = byId('profile-not-looking');
  paused.click();
  if (!byId('profile-seeking').disabled) throw new Error('not-looking switch left the choices open');
  const fields = await openPublishForTest();
  if (!fields.includes(`${window.strings['profile.notLooking']}: ${window.strings['profile.notLookingOn']}`)
    || fields.some((field) => field.startsWith(window.strings['profile.seekingGenders']))) {
    throw new Error(`not-looking preview: ${fields}`);
  }
  await closePublishDialog();
  paused.click();
  if (byId('profile-seeking').disabled) throw new Error('not-looking switch kept the choices locked');
  const to = byId('profile-seeking-age-to');
  to.value = '20';
  byId('profile-save').click();
  if (to.validationMessage !== window.strings['search.rangeReversed']) {
    throw new Error(`reversed partner age: ${to.validationMessage}`);
  }
  to.value = '60';
  const saved = waitForProfileStatus('profile.saved');
  byId('profile-save').click();
  await saved;
};
const checkSavedProfile = async () => {
  if (byId('profile-gender').value !== '1' || byId('birth-year').value !== '1990'
    || byId('birth-month').value !== '2' || byId('birth-day').value !== ''
    || byId('profile-height').value !== '180' || byId('profile-country').value !== 'DE'
    || byId('profile-smoking').value !== '10' || byId('profile-about').value !== ABOUT || !seekingKept()) {
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
  await checkSeekingForm();
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
// The published profile is 180 cm tall: bounds are inclusive and a reversed range is never sent.
const checkHeightFilter = async () => {
  if (!shown('search-height')) throw new Error('height filter not offered');
  const [min, max] = [byId('search-height-min'), byId('search-height-max')];
  const search = async (from, to) => {
    [min.value, max.value] = [from, to];
    byId('run-search').click();
    await window.searchDone;
    return byId('search-status').textContent;
  };
  const before = byId('search-status').textContent;
  if (await search('190', '170') !== before || max.validationMessage !== window.strings['search.rangeReversed']) {
    throw new Error(`reversed height range: ${max.validationMessage}`);
  }
  const within = await search('180', '180');
  if (within !== `${window.strings['search.results']} 1`) throw new Error(`height search: ${within}`);
  const above = await search('181', '');
  if (above !== window.strings['search.empty']) throw new Error(`height search above: ${above}`);
  await search('', '');
};
// The published profile states smoking only: its value matches, another or an empty field does not.
const checkHabitFilters = async () => {
  const search = async (id, value) => {
    byId(id).value = value;
    byId('run-search').click();
    await window.searchDone;
    const status = byId('search-status').textContent;
    byId(id).value = '0';
    return status;
  };
  for (const [id, value, expected] of [
    ['search-smoking', '10', `${window.strings['search.results']} 1`],
    ['search-smoking', '20', window.strings['search.empty']],
    ['search-pets', '2', window.strings['search.empty']],
  ]) {
    const status = await search(id, value);
    if (status !== expected) throw new Error(`${id}=${value}: ${status}`);
  }
  for (const key of ['smoking', 'drinking', 'tattoos', 'marital_status', 'pets']) {
    if (!byId(`search-${key}`)) throw new Error(`${key} filter not offered`);
  }
};
// The published profile seeks a woman of 30 to 60 and is the woman born in February 1990 who
// searches; in February her age is not exact, so the search is refused with what to fix.
const checkMutualFilter = async () => {
  if (!shown('search-mutual-row')) throw new Error('mutual search not offered');
  byId('search-mutual').checked = true;
  byId('run-search').click();
  await window.searchDone;
  byId('search-mutual').checked = false;
  if (new Date().getUTCMonth() === 1) {
    if (byId('search-error-text').textContent !== window.strings['search.mutualUnavailable']) {
      throw new Error(`mutual search refusal: ${byId('search-error-text').textContent}`);
    }
    return;
  }
  const status = byId('search-status').textContent;
  if (status !== `${window.strings['search.results']} 1`) throw new Error(`mutual search: ${status}`);
};
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
    await openArea('chats-area');
    await window.chatsDone;
    if (document.getElementById('chats-status').textContent !== window.strings['chats.notConnected']) {
      throw new Error('Chats showed a fresh empty list without a network');
    }
    await shot('chats-not-connected');
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
  await globalThis.checkSafetyNumber();
  await shot('search-safety');
  document.getElementById('view-back').click();
  if (!shown('search-main') || shown('profile-view')
    || document.activeElement.id !== 'result-1'
    || document.querySelectorAll('#search-results [data-peer-id]').length !== 1) {
    throw new Error('Back did not return to the same results');
  }
  await checkStatus(true);
  await shot('statusbar');
  await checkHeightFilter();
  await checkHabitFilters();
  await checkMutualFilter();
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
    if (step.startsWith('video-bench:')) return globalThis.videoBenchmark(step.slice('video-bench:'.length));
    if (step.startsWith('video:')) {
      const size = step.slice('video:'.length);
      if (!['360p', '720p'].includes(size)) throw new Error('invalid video smoke size');
      const metrics = await videoFrames(size);
      return invokeTest('ui_test_result', {
        passed: true, peerId: '', reason: `video:${size}:${JSON.stringify(metrics)}`,
      });
    }
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
      'profile-publish-photo': async () => {
        await photoChecks.addRealPhoto();
        await publishProfileOnLoopback(false, false, 1);
      },
      'profile-hide': hideProfileOnLoopback,
      'profile-unhide': unhideProfileOnLoopback,
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
      const labels = ['tray.open', 'tray.quit', 'notice.newMessage'].map((key) => window.strings[key]);
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
    if (step.startsWith('link-')) {
      // Not timed like a check: the wait lasts as long as the other client takes.
      const peerId = await globalThis.uiSmokeGuards.linkStep(step, mode, state);
      return invokeTest('ui_test_result', { passed: true, peerId, reason: '' });
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
      await expectPrivacy('false,false,true,true,true');
      document.getElementById('send-views').click();
      await window.privacySaved;
      await switchFromSettings(state.mode);
      await checkSettingsInventory(state.peerId, state.mode);
      await expectPrivacy('false,false,true,true,true');
      await switchFromSettings(mode);
      await checkSettingsInventory(addedPeer, mode);
      await expectPrivacy('true,false,true,true,true');
      document.getElementById('send-views').click();
      await window.privacySaved;
      await openArea('profile-area');
      await identityChecks.checkStaleRender();
      await identityChecks.checkStaleSearchAndChats();
      await chatChecks.checkChatProjection();
      await chatChecks.checkFirstMessage();
      await chatChecks.checkClearChat();
      await chatChecks.checkChatBlock();
      await chatChecks.checkChatEdit();
      await chatChecks.checkChatDelete();
      await chatChecks.checkChatDelivery();
      await chatChecks.checkNewDevice();
      await chatChecks.checkChatRead();
      await presenceChecks.checkChatPresence();
      await presenceChecks.checkLikePending();
      await nameChecks.checkContactNames();
      await nameChecks.checkPublicName();
      await nameChecks.checkInterests();
      await nameChecks.checkFavoriteAction();
      await nameChecks.checkContactNotes();
      await nameChecks.checkPassActions();
      await nameChecks.checkSavedProfiles();
      await nameChecks.checkChatMute();
      await photoChecks.checkChatPhoto();
      await photoChecks.checkProfilePhotos();
      await photoChecks.checkPeerPhotos();
      await chatChecks.checkChatForwarded();
      await chatChecks.checkChatForwardAction();
      await chatChecks.checkChatLinkHosts();
      await nameChecks.checkProfileShare();
      await identityChecks.checkStalePeerActions();
      await identityChecks.checkStaleSafety();
      await identityChecks.checkStaleProfileLoad();
      await identityChecks.checkStaleProfileHandlers();
      await identityChecks.checkStalePublishAndHide();
      await identityChecks.checkStaleProfileTimers();
      await identityChecks.checkStaleUsageStatus();
      await identityChecks.checkStalePrivacy();
      await identityChecks.checkStaleBlocked();
      await guardChecks.checkStaleSettingsInventory();
      await guardChecks.checkStaleNode();
      await guardChecks.checkStaleTray();
      await guardChecks.checkStaleRemovePreparation();
      await guardChecks.checkStaleWithdrawal();
      await guardChecks.checkDeviceRevoked();
      await guardChecks.checkDevices();
      await guardChecks.checkLinkCode();
      await guardChecks.checkLinkScreen();
      await guardChecks.checkLinkedNotice();
      await guardChecks.checkStaleLaunchConnection();
      await guardChecks.checkStaleNetworkForm();
      await guardChecks.checkStaleNetworkCheck();
      await guardChecks.checkStaleCopy();
      await guardChecks.checkStalePlaceSuggestions();
    } else if (step === 'pick') {
      if (state.status !== 'picker') throw new Error('identity picker not shown');
      await pickReady(mode);
      await checkSettingsInventory(document.getElementById('peer-id').textContent, mode);
    } else if (step === 'remove') {
      await removeOpened(state, mode);
    } else if (step.startsWith('connect:')) {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      await connectNetwork(step.slice('connect:'.length));
    } else if (step.startsWith('peer:')) {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      await chatChecks.checkPeerStep(...step.slice('peer:'.length).split(':'));
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
