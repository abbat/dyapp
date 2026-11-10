// Desktop UI smoke: network, profile, connected fixtures and step dispatch.
const identityChecks = smoke.timedChecks(smoke.uiSmokeIdentity);
const chatChecks = smoke.timedChecks(smoke.uiSmokeChats);
const presenceChecks = smoke.timedChecks(smoke.uiSmokePresence);
const nameChecks = smoke.timedChecks(smoke.uiSmokeNames);
const photoChecks = smoke.timedChecks(smoke.uiSmokePhotos);
const guardChecks = smoke.timedChecks(smoke.uiSmokeGuards);
// Saves the node form and returns the status it ends on.
const saveNode = async (enabled, disk) => {
  document.getElementById('node-enabled').checked = enabled;
  document.getElementById('node-disk').value = disk;
  appApi.nodeApplied = null;
  document.getElementById('node-save').click();
  await appApi.nodeApplied;
  return document.getElementById('node').dataset.status;
};
// Settings → Storage: the node store is Unavailable until the node first runs, then sized.
const checkUsage = async (limit) => {
  const { strings } = appApi;
  await openArea('profile-area');
  await openArea('settings', 'node');
  await appApi.usageShown;
  const text = (id) => document.getElementById(id).textContent;
  const bar = document.getElementById('usage-node-bar');
  if (
    document.getElementById('usage').hidden ||
    !text('usage-identity-dir') ||
    text('usage-identity-dir') === strings['usage.unavailable'] ||
    text('usage-messages') === strings['usage.unavailable'] ||
    !text('usage-messages-path').endsWith('client.db') ||
    !text('usage-node-path').endsWith('node') ||
    document.getElementById('usage-free-row').hidden
  ) {
    throw new Error('Storage card misses a path or a known size');
  }
  if (
    limit
      ? bar.hidden ||
        bar.max !== limit ||
        !(bar.value > 0) ||
        text('usage-node') === strings['usage.unavailable']
      : !bar.hidden || text('usage-node') !== strings['usage.unavailable']
  ) {
    throw new Error(`Storage card shows the node store wrong: ${text('usage-node')}`);
  }
};
// Settings → Storage on every platform: the store sits in the identity folder and has a size; a
// row the app cannot fill is hidden, and no size reads as a made-up zero.
const checkStorageCard = async () => {
  await openArea('settings', 'node');
  await appApi.usageShown;
  const text = (id) => document.getElementById(id).textContent;
  const shown = (id) => !document.getElementById(id).hidden;
  const dir = text('usage-identity-dir');
  const sep = dir.includes('\\') ? '\\' : '/';
  const path = text('usage-messages-path');
  const sizes = ['usage-messages', 'usage-node', 'usage-free'];
  if (
    !/^(\/|[A-Za-z]:\\)/.test(dir) ||
    !path.startsWith(`${dir}${sep}`) ||
    !path.endsWith(`${sep}client.db`) ||
    text('usage-messages') === appApi.strings['usage.unavailable'] ||
    !shown('usage-node-row') ||
    !shown('usage-free-row') ||
    sizes.some((id) => /^0\s/.test(text(id)))
  ) {
    throw new Error(`Storage card wrong: ${dir} ${path} ${sizes.map(text).join(' | ')}`);
  }
  await smoke.shot('storage');
  await openArea('profile-area');
};
// The node starts on the loopback network, then stops; bad limits never reach the app.
const checkNode = async () => {
  const { strings } = appApi;
  await checkUsage(0);
  await appApi.nodeShown;
  const form = document.getElementById('node');
  if (
    form.hidden ||
    document.getElementById('node-enabled').checked ||
    document.getElementById('node-disk').value !== '1024' ||
    document.getElementById('node-connections').value !== '64' ||
    form.dataset.status !== 'off' ||
    document.getElementById('node-status').textContent !== strings['node.off'] ||
    document.querySelector('#node h2').textContent !== strings['node.title']
  ) {
    throw new Error('node form not shown with its defaults');
  }
  if ((await saveNode(true, '3')) !== 'invalidLimits') throw new Error('tiny disk limit accepted');
  const running = await saveNode(true, '64');
  const text = document.getElementById('node-status').textContent;
  if (
    running !== 'running' ||
    !text.startsWith(strings['node.running']) ||
    text.length <= strings['node.running'].length + 1
  ) {
    throw new Error(`node did not start: ${text}`);
  }
  await checkUsage(64 << 20);
  await smoke.shot('settings-storage');
  if ((await saveNode(false, '64')) !== 'off') throw new Error('node did not stop');
  await openArea('profile-area');
};
const byId = (id) => document.getElementById(id);
const publicFieldList = () =>
  [...document.querySelectorAll('#profile-public-fields li')].map((item) => item.textContent);
const openPublishForTest = async () => {
  byId('profile-publish').click();
  await waitFor('publish-dialog');
  if (
    byId('profile-attested').checked ||
    byId('profile-disclosure').checked ||
    !byId('profile-publish-confirm').disabled
  ) {
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
  const finished = closed(byId('publish-dialog'));
  byId('profile-cancel').click();
  await finished;
  if (!byId('publish-dialog').hidden) throw new Error('Cancel left the publish dialog open');
};
const checkEmptyProfilePreview = async () => {
  await smoke.shot('profile-edit');
  if (!byId('profile-hidden').disabled)
    throw new Error('Hide from search offered before a publish');
  const fields = await openPublishForTest();
  await smoke.shot('publish-dialog');
  if (fields.join('|') !== appApi.strings['profile.empty'])
    throw new Error('unset fields appeared in preview');
  if (byId('profile-age').textContent !== appApi.strings['profile.ageUnknown']) {
    throw new Error('unknown age not shown');
  }
  if (!byId('profile-date-warning').hidden)
    throw new Error('full-date warning shown without a birth date');
  consent('profile-attested');
  if (!byId('profile-publish-confirm').disabled) throw new Error('Publish enabled with one box');
  consent('profile-disclosure');
  if (byId('profile-publish-confirm').disabled) throw new Error('Publish disabled with both boxes');
  await closePublishDialog();
  if (byId('profile-status').textContent !== appApi.strings['profile.saved']) {
    throw new Error('Cancel lost the draft');
  }
  await openPublishForTest();
  await closePublishDialog();
};
const waitForProfileStatus = (key, detail = '', ms = 10000) =>
  new Promise((resolve, reject) => {
    const status = document.getElementById('profile-status');
    const expected = `${appApi.strings[key]}${detail ? ` ${detail}` : ''}`;
    const observer = new MutationObserver(() => {
      if (status.textContent === expected) {
        observer.disconnect();
        clearTimeout(timeout);
        resolve();
      }
    });
    observer.observe(status, { childList: true });
    const timeout = setTimeout(() => {
      observer.disconnect();
      reject(new Error(`profile status: ${status.textContent}`));
    }, ms);
  });
// Picks Germany and the first suggestion for "Berl", the largest match: Berlin.
const pickPlace = async (form = 'profile') => {
  const country = document.getElementById(`${form}-country`);
  const place = document.getElementById(`${form}-place`);
  country.value = 'DE';
  country.dispatchEvent(new Event('input', { bubbles: true }));
  place.value = 'Berl';
  place.dispatchEvent(new Event('input', { bubbles: true }));
  await appApi.placesShown;
  const label = document.querySelector(`#${form}-places option`)?.value;
  if (!label?.startsWith(appApi.locale === 'ru' ? 'Берлин, ' : 'Berlin, ')) {
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
  await appApi.placesShown;
  if (place.checkValidity()) throw new Error('unlisted place accepted');
  await openArea('network-area');
  await openArea('profile-area');
  if (
    document.getElementById('profile-gender').value !== '1' ||
    document.getElementById('birth-year').value !== '1990' ||
    document.getElementById('birth-month').value !== '2' ||
    place.value !== 'Nowhere' ||
    place.checkValidity()
  ) {
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
const seekingKept = () =>
  byId('profile-seeking-gender-1').checked &&
  !byId('profile-seeking-gender-2').checked &&
  byId('profile-seeking-age-from').value === '30' &&
  byId('profile-seeking-age-to').value === '60' &&
  !byId('profile-not-looking').checked;
const expectedFields = (photos = 0) =>
  [
    `${appApi.strings['profile.field.gender']}: ${appApi.strings['profile.value.gender.Woman']}`,
    `${appApi.strings['profile.field.smoking']}: ${appApi.strings['profile.value.smoking.No']}`,
    `${appApi.strings['profile.birthYear']}: 1990`,
    `${appApi.strings['profile.birthMonth']}: 2`,
    `${appApi.strings['profile.height']}: 180`,
    `${appApi.strings['profile.about']}: ${ABOUT}`,
    `${appApi.strings['profile.seekingGenders']}: ${appApi.strings['profile.value.gender.Woman']}`,
    `${appApi.strings['profile.seekingAgeFrom']}: 30`,
    `${appApi.strings['profile.seekingAgeTo']}: 60`,
    ...(photos ? [`${appApi.strings['profile.photos']}: ${photos}`] : []),
  ].join('|');
const preparePublishDraft = (saved) => {
  if (saved) {
    if (
      byId('profile-gender').value !== '1' ||
      byId('birth-year').value !== '1990' ||
      byId('birth-month').value !== '2' ||
      byId('profile-height').value !== '180' ||
      byId('profile-smoking').value !== '10' ||
      byId('profile-about').value !== ABOUT ||
      !seekingKept()
    ) {
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
};
const publishProfileOnLoopback = async (saved = false, fails = false, photos = 0) => {
  preparePublishDraft(saved);
  const fields = await openPublishForTest();
  if (fields.join('|') !== expectedFields(photos))
    throw new Error(`profile preview changed: ${fields}`);
  consent('profile-disclosure');
  consent('profile-attested');
  const result = waitForProfileStatus(
    fails ? 'profile.notConnected' : 'profile.published',
    fails ? '' : '1',
    150000,
  );
  byId('profile-publish-confirm').click();
  await waitFor('publish-progress');
  if (!/\d:\d\d/.test(byId('publish-elapsed').textContent)) {
    throw new Error(`elapsed time not shown: ${byId('publish-elapsed').textContent}`);
  }
  await result;
  await new Promise((resolve) => setTimeout(resolve, 50));
  if (!byId('publish-dialog').hidden) throw new Error('publish dialog stayed open');
  await smoke.shot(fails ? 'profile-publish-failed' : 'profile-published');
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
  await smoke.shot('profile-hidden');
  if (
    !hidden.checked ||
    byId('profile-hidden-notice').hidden ||
    byId('profile-gender').value !== '1'
  ) {
    throw new Error('hidden profile not shown with its notice and draft');
  }
  await openArea('search-area');
  byId('run-search').click();
  await appApi.searchDone;
  if (!shown('search-empty') || document.querySelector('#search-results [data-peer-id]'))
    throw new Error('hidden profile remains in search');
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
  if (
    hidden.checked ||
    !byId('profile-hidden-notice').hidden ||
    byId('profile-search-wait').hidden ||
    byId('profile-gender').value !== '1'
  ) {
    throw new Error('unhide did not publish the saved draft with search pending');
  }
  await smoke.shot('profile-unhidden-pending');
};
// The switch locks the choices and publishes only itself; a reversed age range is never saved.
const checkSeekingForm = async () => {
  const paused = byId('profile-not-looking');
  paused.click();
  if (!byId('profile-seeking').disabled)
    throw new Error('not-looking switch left the choices open');
  const fields = await openPublishForTest();
  if (
    !fields.includes(
      `${appApi.strings['profile.notLooking']}: ${appApi.strings['profile.notLookingOn']}`,
    ) ||
    fields.some((field) => field.startsWith(appApi.strings['profile.seekingGenders']))
  ) {
    throw new Error(`not-looking preview: ${fields}`);
  }
  await closePublishDialog();
  paused.click();
  if (byId('profile-seeking').disabled)
    throw new Error('not-looking switch kept the choices locked');
  const to = byId('profile-seeking-age-to');
  to.value = '20';
  byId('profile-save').click();
  if (to.validationMessage !== appApi.strings['search.rangeReversed']) {
    throw new Error(`reversed partner age: ${to.validationMessage}`);
  }
  to.value = '60';
  const saved = waitForProfileStatus('profile.saved');
  byId('profile-save').click();
  await saved;
};
const checkSavedProfile = async () => {
  if (
    byId('profile-gender').value !== '1' ||
    byId('birth-year').value !== '1990' ||
    byId('birth-month').value !== '2' ||
    byId('birth-day').value !== '' ||
    byId('profile-height').value !== '180' ||
    byId('profile-country').value !== 'DE' ||
    byId('profile-smoking').value !== '10' ||
    byId('profile-about').value !== ABOUT ||
    !seekingKept()
  ) {
    throw new Error('profile draft changed after restart');
  }
  const place = byId('profile-place').value;
  if (!place.startsWith(appApi.locale === 'ru' ? 'Берлин, ' : 'Berlin, ')) {
    throw new Error(`picked place lost after restart: ${place}`);
  }
  const placeFields = (await openPublishForTest()).filter(
    (field) =>
      field.startsWith(appApi.strings['profile.country']) ||
      field.startsWith(appApi.strings['profile.place']),
  );
  if (
    placeFields.join('|') !==
    [
      `${appApi.strings['profile.country']}: ${new Intl.DisplayNames([appApi.locale], { type: 'region' }).of('DE')}`,
      `${appApi.strings['profile.place']}: ${place}`,
    ].join('|')
  )
    throw new Error(`place preview changed: ${placeFields}`);
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
  if (!byId('profile-age').textContent.startsWith(`${appApi.strings['profile.age']}: `)) {
    throw new Error('known age hidden after restart');
  }
  await checkSeekingForm();
};
// The status bar: traffic is unavailable until a search starts the client swarm, then counts the
// reply; the message store always has a size once the identity is open.
const checkStatus = async (traffic) => {
  const { strings } = appApi;
  appApi.statusShown = appBindings.showStatus();
  await appApi.statusShown;
  const text = (id) => document.getElementById(id).textContent;
  const [, received] = text('status-traffic').match(/(\d[\d.,]*)\s/) ?? [];
  if (
    traffic
      ? !received || !(Number.parseFloat(received.replace(',', '.')) > 0)
      : text('status-traffic') !== strings['statusbar.noTraffic']
  ) {
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
    await appApi.searchDone;
    return byId('search-status').textContent;
  };
  const before = byId('search-status').textContent;
  if (
    (await search('190', '170')) !== before ||
    max.validationMessage !== appApi.strings['search.rangeReversed']
  ) {
    throw new Error(`reversed height range: ${max.validationMessage}`);
  }
  const within = await search('180', '180');
  if (within !== `${appApi.strings['search.results']} 1`)
    throw new Error(`height search: ${within}`);
  const above = await search('181', '');
  if (above !== appApi.strings['search.empty']) throw new Error(`height search above: ${above}`);
  await search('', '');
};
// The published profile states smoking only: its value matches, another or an empty field does not.
const checkHabitFilters = async () => {
  const search = async (id, value) => {
    byId(id).value = value;
    byId('run-search').click();
    await appApi.searchDone;
    const status = byId('search-status').textContent;
    byId(id).value = '0';
    return status;
  };
  for (const [id, value, expected] of [
    ['search-smoking', '10', `${appApi.strings['search.results']} 1`],
    ['search-smoking', '20', appApi.strings['search.empty']],
    ['search-pets', '2', appApi.strings['search.empty']],
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
  await appApi.searchDone;
  byId('search-mutual').checked = false;
  if (new Date().getUTCMonth() === 1) {
    if (byId('search-error-text').textContent !== appApi.strings['search.mutualUnavailable']) {
      throw new Error(`mutual search refusal: ${byId('search-error-text').textContent}`);
    }
    return;
  }
  const status = byId('search-status').textContent;
  if (status !== `${appApi.strings['search.results']} 1`)
    throw new Error(`mutual search: ${status}`);
};
const searchOnLoopback = (() => {
  const checkDisconnectedSearch = async (status) => {
    if (
      status !== appApi.strings['search.notConnected'] ||
      !shown('search-error') ||
      !shown('search-open-network') ||
      searchFailures() !== 'notConnected'
    ) {
      throw new Error(`search without a network: ${status}, details ${searchFailures()}`);
    }
    await smoke.shot('search-not-connected');
    await openArea('chats-area');
    await appApi.chatsDone;
    if (
      document.getElementById('chats-status').textContent !== appApi.strings['chats.notConnected']
    ) {
      throw new Error('Chats showed a fresh empty list without a network');
    }
    await smoke.shot('chats-not-connected');
  };
  const checkSearchResult = async (status) => {
    const peers = [...document.querySelectorAll('#search-results [data-peer-id]')].map(
      (item) => item.dataset.peerId,
    );
    if (
      status !== `${appApi.strings['search.results']} 1` ||
      shown('search-error') ||
      shown('search-empty') ||
      !shown('search-again') ||
      !shown('search-filters') ||
      peers.join() !== document.getElementById('peer-id').textContent
    ) {
      throw new Error(`loopback search: ${status}, peers ${peers}, failures ${searchFailures()}`);
    }
    await smoke.shot('search-results');
    document.getElementById('result-1').click();
    if (
      shown('search-main') ||
      !shown('profile-view') ||
      document.activeElement.id !== 'view-back' ||
      !document.getElementById('view-title').textContent
    ) {
      throw new Error('the result card did not open the profile');
    }
    await smoke.shot('search-profile');
    await smoke.checkSafetyNumber();
    await smoke.shot('search-safety');
    document.getElementById('view-back').click();
    if (
      !shown('search-main') ||
      shown('profile-view') ||
      document.activeElement.id !== 'result-1' ||
      document.querySelectorAll('#search-results [data-peer-id]').length !== 1
    ) {
      throw new Error('Back did not return to the same results');
    }
  };
  const checkSearchFilters = async () => {
    await checkStatus(true);
    await smoke.shot('statusbar');
    await checkHeightFilter();
    await checkHabitFilters();
    await checkMutualFilter();
    if (document.getElementById('search-place-row').hidden)
      throw new Error('place filter not offered');
    await pickPlace('search');
    document.getElementById('run-search').click();
    await appApi.searchDone;
    const placed = document.getElementById('search-status').textContent;
    if (placed !== appApi.strings['search.empty'] || !shown('search-empty')) {
      throw new Error(`place search: ${placed}`);
    }
    await smoke.shot('search-empty');
    document.getElementById('search-country').value = '';
    document.getElementById('search-country').dispatchEvent(new Event('input', { bubbles: true }));
  };
  return async (connected) => {
    await checkStatus(false);
    await openArea('search-area');
    document.getElementById('run-search').click();
    await appApi.searchDone;
    const status = document.getElementById('search-status').textContent;
    if (!connected) return checkDisconnectedSearch(status);
    await checkSearchResult(status);
    await checkSearchFilters();
  };
})();
// The fixture forged the signature of the published profile in the node's index: the search drops
// it and explains the forged reply under Details instead of showing a person.
const searchForged = async () => {
  await openArea('search-area');
  document.getElementById('run-search').click();
  await appApi.searchDone;
  if (
    document.querySelectorAll('#search-results [data-peer-id]').length ||
    !shown('search-error') ||
    !searchFailures().split('\n').includes(appApi.strings['search.failure.invalid'])
  ) {
    throw new Error(
      `forged search: ${document.getElementById('search-status').textContent}, ` +
        `failures ${searchFailures()}`,
    );
  }
  document.querySelector('#search-error details').open = true;
  await smoke.shot('search-error');
};
// Steps that run one check on the reopened identity.
const readySmokeSteps = (mode) => ({
  network: checkNetwork,
  node: checkNode,
  'network-settings-failure': checkNetworkInSettings,
  'settings-privacy-restart': () => checkPrivacy(true),
  'remove-offline': smoke.identityRemovalChecks.offline,
  'remove-tombstone': () => smoke.identityRemovalChecks.tombstone(mode),
  'profile-save': saveProfileForRestart,
  'profile-restart': checkSavedProfile,
  'sticker-import': smoke.stickerFileChecks.importPack,
  'sticker-restart': smoke.stickerFileChecks.restart,
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
});
const requireSmokeReady = (state) => {
  if (state.status !== 'ready') throw new Error('identity not reopened');
};
const checkSmokeLaunch = async (state, step) => {
  requireSmokeReady(state);
  await appApi.connectionShown;
  if (connection() !== step.slice('launch:'.length)) {
    throw new Error(`launch shows ${connection()}`);
  }
};
const checkSmokeNetworkRestart = async (state) => {
  requireSmokeReady(state);
  if (document.getElementById('seeds').value !== testSeeds) throw new Error('draft lost');
  if (document.getElementById('network-status').textContent !== appApi.strings['network.idle'])
    throw new Error('unchecked draft shown as active');
  await appApi.connectionShown;
  if (connection() !== 'offline') throw new Error(`no active network shows ${connection()}`);
};
const runSmokeIdentityStep = async (state, mode, step) => {
  if (step.startsWith('wizard-connect:')) {
    if (state.status !== 'setup') throw new Error('storage picker not shown');
    await chooseStorage(mode, step.slice('wizard-connect:'.length));
  } else if (step === 'add') {
    await smoke.uiSmokeAddStep(state, mode, {
      identityChecks,
      chatChecks,
      presenceChecks,
      nameChecks,
      photoChecks,
      guardChecks,
    });
  } else if (step === 'pick') {
    if (state.status !== 'picker') throw new Error('identity picker not shown');
    await pickReady(mode);
    await checkSettingsInventory(document.getElementById('peer-id').textContent, mode);
  } else if (step === 'remove') {
    await removeOpened(state, mode);
  } else return false;
  return true;
};
const runSmokeNetworkStep = async (state, step) => {
  if (step.startsWith('connect:')) {
    requireSmokeReady(state);
    await connectNetwork(step.slice('connect:'.length));
  } else if (step.startsWith('peer:')) {
    requireSmokeReady(state);
    await chatChecks.checkPeerStep(...step.slice('peer:'.length).split(':'));
  } else if (step.startsWith('launch:')) {
    await checkSmokeLaunch(state, step);
  } else if (step.startsWith('cancel:')) {
    requireSmokeReady(state);
    await cancelNetwork(step.slice('cancel:'.length));
  } else if (step.startsWith('wrong:')) {
    requireSmokeReady(state);
    await checkWrongNetwork(step.slice('wrong:'.length));
  } else if (step === 'network-restart') {
    await checkSmokeNetworkRestart(state);
  } else if (step === 'settings-restart') {
    requireSmokeReady(state);
    await checkSettingsInventory(state.peerId, state.mode);
    await checkStorageCard();
    await checkStatus(false);
  } else return false;
  return true;
};
const runSmokeFallbackStep = async (state, mode, existing, step, readySteps) => {
  if (step === 'settings-language') {
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
};
const checkModuleStartup = async () => {
  await smoke.checkDomContracts();
  await smoke.checkFoundationContracts();
  const [localization, interests, pixels, fields, form, seeking, peers] = await Promise.all([
    import(new URL('locale.js', document.baseURI).href),
    import(new URL('profile-interests.js', document.baseURI).href),
    import(new URL('sticker-pixels.js', document.baseURI).href),
    import(new URL('profile-fields.js', document.baseURI).href),
    import(new URL('profile-form.js', document.baseURI).href),
    import(new URL('profile-seeking.js', document.baseURI).href),
    import(new URL('peer-labels.js', document.baseURI).href),
  ]);
  if (
    localization.strings !== appApi.strings ||
    localization.locale !== appApi.locale ||
    localization.pseudoLocale !== appApi.pseudoLocale ||
    interests.profileInterestTags !== appApi.profileInterestTags ||
    interests.showProfileInterests !== appApi.showProfileInterests ||
    pixels.stickerPixels !== appApi.stickerPixels ||
    pixels.stickerBinaryPixels !== appApi.stickerBinaryPixels ||
    fields !== appApi.profileFields ||
    form.profileForm !== appApi.profileForm ||
    seeking.seekingDraft !== appApi.seekingDraft ||
    seeking.showSeeking !== appApi.showSeeking ||
    seeking.seekingFacts !== appApi.seekingFacts ||
    seeking.profileFormValid !== appApi.profileFormValid ||
    peers.shortPeerId !== appApi.shortPeerId ||
    peers.shortPeer !== appApi.shortPeer ||
    peers.knowPeers !== appApi.knowPeers ||
    peers.addKnownPeers !== appApi.addKnownPeers
  )
    throw new Error('Desktop module startup bridge is inconsistent');
};
Promise.all([
  appApi.identityShown,
  invokeTest('ui_test_mode'),
  invokeTest('ui_test_existing'),
  invokeTest('ui_test_step'),
])
  .then(async ([state, mode, existing, step]) => {
    if (step.startsWith('video-bench:') || step.startsWith('video:'))
      return smoke.uiSmokeSystemSteps.video(step);
    await checkModuleStartup();
    const readySteps = readySmokeSteps(mode);
    if (step.startsWith('recovery:')) return smoke.uiSmokeSystemSteps.recovery(state, step);
    if (step.startsWith('window:')) return smoke.uiSmokeSystemSteps.windowBounds(step);
    if (step.startsWith('tray:')) return smoke.uiSmokeSystemSteps.tray(state, mode, step);
    if (step === 'single') {
      // A second launch, started by the smoke once the window is hidden, shows it again.
      await invokeTest('ui_test_hide', { close: false });
      return invokeTest('ui_test_result', { passed: true, peerId: '', reason: '' });
    }
    if (step.startsWith('link-')) {
      // Not timed like a check: the wait lasts as long as the other client takes.
      const peerId = await smoke.uiSmokeGuards.linkStep(step, mode, state);
      return invokeTest('ui_test_result', { passed: true, peerId, reason: '' });
    }
    if (!(await runSmokeIdentityStep(state, mode, step))) {
      if (!(await runSmokeNetworkStep(state, step)))
        await runSmokeFallbackStep(state, mode, existing, step, readySteps);
    }
    return smoke.uiSmokeReadyStep(mode, existing, step);
  })
  .catch(smoke.fail);
