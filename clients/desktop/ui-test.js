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
// A pending identity response cannot render after a later transition has shown the picker.
const checkStaleRender = async () => {
  const readyState = current;
  let finishOld;
  const old = act(new Promise((resolve) => { finishOld = resolve; }));
  await act(Promise.resolve({ status: 'picker' }));
  if (document.getElementById('picker').hidden) throw new Error('picker not shown');
  finishOld({ status: 'setup' });
  await old;
  if (document.getElementById('picker').hidden) throw new Error('stale identity response rendered');
  await act(Promise.resolve(readyState));
  if (document.getElementById('identity').hidden) throw new Error('ready view not restored');
};
// A result from an old identity must not paint or unlock controls for a newer one.
const checkStaleSearchAndChats = async () => {
  const readyState = current;
  await openArea('search-area');
  let finishOld;
  const oldSearch = runSearch(() => new Promise((resolve) => { finishOld = resolve; }));
  if (!searchBusy) throw new Error('old search did not start');
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  await openArea('search-area');
  let finishCurrent;
  const newSearch = runSearch(() => new Promise((resolve) => { finishCurrent = resolve; }));
  finishOld({ profiles: [], offered: [], partial: false, failures: [] });
  await oldSearch;
  if (!searchBusy || document.getElementById('search-status').textContent
    !== window.strings['search.loading']) throw new Error('old search changed new search state');
  finishCurrent({ profiles: [], offered: [], partial: false, failures: [] });
  await newSearch;
  if (searchBusy || document.getElementById('search-status').textContent
    !== window.strings['search.empty']) throw new Error('current search did not finish');

  await openArea('chats-area');
  await window.chatsDone;
  let finishMail;
  let oldLikedCalls = 0;
  const oldChats = window.showChats((command) => {
    if (command === 'liked_you') { oldLikedCalls += 1; return Promise.resolve([]); }
    return new Promise((resolve) => { finishMail = resolve; });
  });
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  let finishNewMail;
  const newChats = window.showChats((command) => command === 'liked_you'
    ? Promise.resolve([]) : new Promise((resolve) => { finishNewMail = resolve; }));
  finishMail(1);
  await oldChats;
  if (oldLikedCalls || document.getElementById('chats-status').textContent
    !== window.strings['chats.loading']) throw new Error('old mailbox changed new Chats');
  finishNewMail(0);
  await newChats;
  if (document.getElementById('chats-status').textContent !== window.strings['chats.none']) {
    throw new Error('current Chats did not finish');
  }

  await openArea('chats-area');
  await window.chatsDone;
  let finishPoll;
  const oldPoll = pollMailbox(() => new Promise((resolve) => { finishPoll = resolve; }));
  await act(Promise.resolve({ status: 'picker' }));
  const chatsDone = window.chatsDone;
  finishPoll(1);
  await oldPoll;
  if (window.chatsDone !== chatsDone) throw new Error('old poll scheduled a Chats refresh');
  await act(Promise.resolve(readyState));
};
// Even reopening the same card object for a new identity cannot admit an old peer response.
const checkStalePeerActions = async () => {
  const readyState = current;
  const found = { peerId: 'f'.repeat(64), draft: { birthYear: null, birthMonth: null, birthDay: null }, place: '' };
  const card = resultCard(found, 'stale-peer');
  const reopen = async () => {
    await openArea('search-area');
    searchResults.append(card);
    openFound(found, card, () => Promise.resolve({ ours: false, matched: false }));
    await Promise.resolve();
  };
  const switchBack = async () => {
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    await reopen();
  };
  await openArea('search-area');
  searchResults.append(card);
  let finishLikes;
  openFound(found, card, () => new Promise((resolve) => { finishLikes = resolve; }));
  await switchBack();
  finishLikes({ ours: true, matched: true });
  await Promise.resolve();
  if (document.getElementById('view-like').hidden) throw new Error('old peer likes changed new card');

  let finishOldLike;
  const oldLike = sendLike(true, () => new Promise((resolve) => { finishOldLike = resolve; }));
  await switchBack();
  let finishNewLike;
  const newLike = sendLike(true, () => new Promise((resolve) => { finishNewLike = resolve; }));
  finishOldLike({ ours: true, matched: true });
  await oldLike;
  if (!document.getElementById('view-like').disabled
    || viewStatus.textContent !== window.strings['view.sending']) {
    throw new Error('old like changed new controls');
  }
  finishNewLike({ ours: true, matched: false });
  await newLike;
  if (document.getElementById('view-like').disabled || !document.getElementById('view-like').hidden) {
    throw new Error('current like did not finish');
  }

  let finishBlock;
  const oldBlock = blockViewed(() => new Promise((resolve) => { finishBlock = resolve; }));
  await switchBack();
  finishBlock({ revocation: 'notNeeded' });
  await oldBlock;
  if (!card.isConnected || profileView.hidden) throw new Error('old block removed new card');
  await blockViewed(() => Promise.resolve({ revocation: 'notNeeded' }));
  if (card.isConnected || !profileView.hidden) throw new Error('current block did not finish');
};
// Late safety responses must not open or unlock the next identity's shared panel.
const checkStaleSafety = async () => {
  const readyState = current;
  const found = { peerId: 'e'.repeat(64), draft: { birthYear: null, birthMonth: null, birthDay: null }, place: '' };
  const card = resultCard(found, 'stale-safety');
  const matrix = { size: 21, modules: Array(21 * 21).fill(false) };
  const number = '12345'.repeat(12);
  const reopen = async () => {
    await openArea('search-area');
    searchResults.append(card);
    openFound(found, card, () => Promise.resolve({ ours: false, matched: false }));
  };
  await reopen();
  let finishOldNumber;
  const oldSafety = loadSafety((command) => command === 'contact_safety_qr'
    ? Promise.resolve(matrix) : new Promise((resolve) => { finishOldNumber = resolve; }));
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  await reopen();
  let finishNewNumber;
  const newSafety = loadSafety((command) => command === 'contact_safety_qr'
    ? Promise.resolve(matrix) : new Promise((resolve) => { finishNewNumber = resolve; }));
  finishOldNumber(number);
  await oldSafety;
  if (!document.getElementById('view-safety').disabled
    || !document.getElementById('view-safety-panel').hidden) {
    throw new Error('old safety result changed new panel');
  }
  finishNewNumber(number);
  await newSafety;
  if (document.getElementById('view-safety').disabled
    || document.getElementById('view-safety-panel').hidden) {
    throw new Error('current safety result did not finish');
  }

  const mediaDescriptor = Object.getOwnPropertyDescriptor(navigator, 'mediaDevices');
  let finishCapture;
  let stopped = false;
  Object.defineProperty(navigator, 'mediaDevices', { configurable: true, value: {
    getUserMedia: () => new Promise((resolve) => { finishCapture = resolve; }),
  } });
  try {
    document.getElementById('view-safety-camera').click();
    await act(Promise.resolve({ status: 'picker' }));
    finishCapture({ getTracks: () => [{ stop: () => { stopped = true; } }] });
    await window.cameraDone;
    if (!stopped) throw new Error('old camera stream was not stopped');
  } finally {
    if (mediaDescriptor) Object.defineProperty(navigator, 'mediaDevices', mediaDescriptor);
    else delete navigator.mediaDevices;
  }
  await act(Promise.resolve(readyState));
  await reopen();
  const copyButton = document.getElementById('copy-safety-number');
  const copyLabel = copyButton.textContent;
  const originalTimeout = globalThis.setTimeout;
  const expiries = [];
  globalThis.setTimeout = (callback, delay, ...args) => {
    if (delay === 60000) { expiries.push(callback); return 0; }
    return originalTimeout(callback, delay, ...args);
  };
  try {
    let finishCopy;
    const oldCopy = copySafety(() => new Promise((resolve) => { finishCopy = resolve; }));
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    await reopen();
    finishCopy('old sensitive number');
    await oldCopy;
    if (copyButton.textContent !== copyLabel || expiries.length !== 1) {
      throw new Error('old safety copy changed UI or lost its expiry');
    }
  } finally {
    globalThis.setTimeout = originalTimeout;
  }
};
// Profile loads have two await boundaries; an old save must not repaint Ready either.
const checkStaleProfileLoad = async () => {
  const readyState = current;
  let finishLoad;
  const oldLoad = showProfile((command) => command === 'load_profile'
    ? new Promise((resolve) => { finishLoad = resolve; }) : Promise.resolve([]));
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  const status = profileStatus.textContent;
  finishLoad({});
  await oldLoad;
  if (profileStatus.textContent !== status) throw new Error('old profile load changed new editor');

  let finishPlace;
  const oldPlace = showProfile((command) => {
    if (command === 'profile_scales' || command === 'place_countries') return Promise.resolve([]);
    if (command === 'load_profile') return Promise.resolve({
      draft: { country: 'DE', place: '1', birthYear: null, birthMonth: null, birthDay: null },
      searchMinutes: null, published: 0, hidden: false, ages: null,
    });
    return new Promise((resolve) => { finishPlace = resolve; });
  });
  await new Promise((resolve) => { setTimeout(resolve, 0); });
  if (!finishPlace) throw new Error('old place lookup did not start');
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  const place = placeInput.value;
  finishPlace({ id: 1, name: 'Stale place' });
  await oldPlace;
  if (placeInput.value !== place) throw new Error('old place changed new editor');

  let finishSave;
  const oldSave = saveProfile(() => new Promise((resolve) => { finishSave = resolve; }));
  if (!finishSave) throw new Error('old save did not start');
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  const savedStatus = profileStatus.textContent;
  finishSave({ problem: null, ages: null });
  await oldSave;
  if (profileStatus.textContent !== savedStatus) throw new Error('old save changed new editor');
};
// An old form handler's finally must not unlock a newer save or open its preview dialog.
const checkStaleProfileHandlers = async () => {
  const readyState = current;
  let finishOldSave;
  const oldSave = submitProfile(() => new Promise((resolve) => { finishOldSave = resolve; }));
  if (!profileBusy) throw new Error('old profile save did not set busy');
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  let finishNewSave;
  const newSave = submitProfile(() => new Promise((resolve) => { finishNewSave = resolve; }));
  finishOldSave(null);
  await oldSave;
  if (!profileBusy || !document.getElementById('profile-save').disabled) {
    throw new Error('old save unlocked new form');
  }
  finishNewSave(null);
  await newSave;
  if (profileBusy) throw new Error('new save remained busy');

  let finishOldPreview;
  const oldPreview = previewAndPublish(() => new Promise((resolve) => { finishOldPreview = resolve; }));
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  let finishNewPreview;
  const newPreview = previewAndPublish(() => new Promise((resolve) => { finishNewPreview = resolve; }));
  finishOldPreview({ problem: null, draft: { birthYear: null, birthMonth: null, birthDay: null } });
  await oldPreview;
  if (!profileBusy || publishDialog.open) throw new Error('old preview changed new dialog or busy state');
  finishNewPreview(null);
  await newPreview;
  if (profileBusy || publishDialog.open) throw new Error('new preview did not finish');
};
// A late publish or hide must leave the next identity's dialog and busy state alone.
const checkStalePublishAndHide = async () => {
  const readyState = current;
  const published = { published: 1, hidden: false, searchMinutes: null };
  const openDialog = () => {
    openPublishDialog({ draft: profileDraft() });
    profileAttested.checked = true;
    profileDisclosure.checked = true;
    publishConfirm.disabled = false;
  };
  openDialog();
  let finishOldPublish;
  const oldPublish = confirmPublish((command) => command === 'solve_profile'
    ? Promise.resolve() : new Promise((resolve) => { finishOldPublish = resolve; }));
  await new Promise((resolve) => { setTimeout(resolve, 0); });
  if (!finishOldPublish) throw new Error('old profile publish did not start');
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  openDialog();
  let finishNewSolve;
  const newPublish = confirmPublish((command) => command === 'publish_profile'
    ? Promise.resolve(published) : new Promise((resolve) => { finishNewSolve = resolve; }));
  finishOldPublish(published);
  await oldPublish;
  if (!profileBusy || !publishDialog.open
    || profileStatus.textContent !== window.strings['profile.solving']) {
    throw new Error('old publish changed new dialog');
  }
  finishNewSolve();
  await newPublish;
  if (profileBusy || publishDialog.open
    || profileStatus.textContent !== `${window.strings['profile.published']} 1`) {
    throw new Error('new publish did not finish');
  }

  profileHidden.checked = true;
  let finishOldHide;
  const oldHide = hideProfile(() => new Promise((resolve) => { finishOldHide = resolve; }));
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  profileHidden.checked = true;
  let finishNewHide;
  const newHide = hideProfile(() => new Promise((resolve) => { finishNewHide = resolve; }));
  const status = profileStatus.textContent;
  finishOldHide({ ...published, hidden: true });
  await oldHide;
  if (!profileBusy || profileStatus.textContent !== status) {
    throw new Error('old hide changed new editor');
  }
  finishNewHide({ ...published, hidden: true });
  await newHide;
  if (profileBusy || !profileHidden.checked) throw new Error('new hide did not finish');
  await showProfile();
};
// Search-wait and elapsed timers must retain the identity generation that scheduled them.
const checkStaleProfileTimers = async () => {
  const readyState = current;
  const originalTimeout = globalThis.setTimeout;
  let oldTick;
  let sent = 0;
  globalThis.setTimeout = (callback, delay, ...args) => {
    if (delay === 60000) { oldTick = callback; return 0; }
    return originalTimeout(callback, delay, ...args);
  };
  try { showSearchWait({ searchMinutes: 1 }, () => { sent += 1; }); }
  finally { globalThis.setTimeout = originalTimeout; }
  if (!oldTick) throw new Error('search wait timer was not scheduled');
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  oldTick();
  if (sent) throw new Error('old search wait timer ran on new identity');

  let finishSearch;
  const oldSearch = sendSearchProfile(identityGeneration,
    () => new Promise((resolve) => { finishSearch = resolve; }));
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  const status = profileStatus.textContent;
  finishSearch({ searchMinutes: null, hidden: true, published: 1 });
  await oldSearch;
  if (profileStatus.textContent !== status) throw new Error('old search send changed new profile');

  const originalInterval = globalThis.setInterval;
  let elapsedTick;
  globalThis.setInterval = (callback, delay, ...args) => {
    if (delay === 1000) { elapsedTick = callback; return 0; }
    return originalInterval(callback, delay, ...args);
  };
  try { startElapsed(); }
  finally { globalThis.setInterval = originalInterval; }
  if (!elapsedTick) throw new Error('publish elapsed timer was not scheduled');
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  elapsed.textContent = 'new identity';
  elapsedTick();
  if (elapsed.textContent !== 'new identity') throw new Error('old elapsed timer changed new dialog');
};
// Old Settings and status polls must not paint B or schedule another A poll.
const checkStaleUsageStatus = async () => {
  const readyState = current;
  await openArea('settings');
  await window.usageShown;
  let finishUsage;
  const oldUsage = showUsage(() => new Promise((resolve) => { finishUsage = resolve; }));
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  await openArea('settings');
  await window.usageShown;
  const path = document.getElementById('usage-identity-dir').textContent;
  const originalTimeout = globalThis.setTimeout;
  let oldTimers = 0;
  globalThis.setTimeout = (callback, delay, ...args) => {
    if (delay === 10000) oldTimers += 1;
    return originalTimeout(callback, delay, ...args);
  };
  try {
    finishUsage({ identityDir: '/old', messages: 0, messagesPath: '/old',
      nodePath: null, node: null, nodeLimit: null, free: null });
    await oldUsage;
  } finally { globalThis.setTimeout = originalTimeout; }
  if (document.getElementById('usage-identity-dir').textContent !== path || oldTimers) {
    throw new Error('old usage changed new storage card or rearmed polling');
  }

  let finishStatus;
  const oldStatus = showStatus(() => new Promise((resolve) => { finishStatus = resolve; }));
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  await window.statusShown;
  const traffic = document.getElementById('status-traffic').textContent;
  globalThis.setTimeout = (callback, delay, ...args) => {
    if (delay === 2000) oldTimers += 1;
    return originalTimeout(callback, delay, ...args);
  };
  try {
    finishStatus({ traffic: [1, 2], messages: 1 });
    await oldStatus;
  } finally { globalThis.setTimeout = originalTimeout; }
  if (document.getElementById('status-traffic').textContent !== traffic || oldTimers) {
    throw new Error('old status changed new status bar or rearmed polling');
  }
};
// A privacy response from A must leave B's toggles and error state alone.
const checkStalePrivacy = async () => {
  const readyState = current;
  await openArea('settings');
  await window.privacyShown;
  let finishLoad;
  const oldLoad = showPrivacy(() => new Promise((resolve) => { finishLoad = resolve; }));
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  await openArea('settings');
  await window.privacyShown;
  const newer = { sendViews: true, sendReadReceipts: true, sendTyping: false,
    presenceMatches: false, notifications: false };
  await showPrivacy(() => Promise.resolve(newer));
  finishLoad({ sendViews: false, sendReadReceipts: false, sendTyping: true,
    presenceMatches: true, notifications: true });
  await oldLoad;
  if (privacyState() !== 'true,true,false,false,false'
    || !document.getElementById('presence-nobody').checked || shown('privacy-error')) {
    throw new Error('old privacy load changed new settings');
  }

  let failLoad;
  const oldFailure = showPrivacy(() => new Promise((_, reject) => { failLoad = reject; }));
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  await openArea('settings');
  await window.privacyShown;
  failLoad(new Error('old load'));
  await oldFailure;
  if (shown('privacy-error')) throw new Error('old privacy load error changed new settings');

  let finishSave;
  const oldSave = savePrivacy(() => new Promise((resolve) => { finishSave = resolve; }));
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  await openArea('settings');
  await window.privacyShown;
  privacyError.hidden = false;
  finishSave();
  await oldSave;
  if (privacyError.hidden) throw new Error('old privacy save cleared new error');
  await savePrivacy(() => Promise.resolve());
  if (shown('privacy-error')) throw new Error('current privacy save did not clear error');

  let failSave;
  const oldSaveFailure = savePrivacy(() => new Promise((_, reject) => { failSave = reject; }));
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  await openArea('settings');
  await window.privacyShown;
  failSave(new Error('old save'));
  await oldSaveFailure;
  if (shown('privacy-error')) throw new Error('old privacy save error changed new settings');
};
// Blocked-peer loads and actions from A must not repaint B's list or status.
const checkStaleBlocked = async () => {
  const readyState = current;
  const oldPeer = 'a'.repeat(64);
  const newPeer = 'b'.repeat(64);
  const status = document.getElementById('blocked-revoke-status');
  const list = document.getElementById('blocked-list');
  const switchToNewList = async () => {
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    await openArea('settings');
    await window.blockedShown;
    await showBlocked(() => Promise.resolve([newPeer]));
    status.textContent = 'new identity';
  };
  const checkNewList = (message) => {
    if (list.children.length !== 1 || !list.textContent.includes(newPeer.slice(-6))
      || status.textContent !== 'new identity'
      || document.getElementById('unblock-1').disabled
      || document.getElementById('retry-revocation-1').disabled) throw new Error(message);
  };
  await openArea('settings');
  await window.blockedShown;
  let finishLoad;
  const oldLoad = showBlocked(() => new Promise((resolve) => { finishLoad = resolve; }));
  await switchToNewList();
  finishLoad([oldPeer]);
  await oldLoad;
  checkNewList('old blocked list changed new settings');

  let failLoad;
  const oldFailure = showBlocked(() => new Promise((_, reject) => { failLoad = reject; }));
  await switchToNewList();
  failLoad(new Error('old list'));
  await oldFailure;
  checkNewList('old blocked list error changed new settings');
  if (shown('privacy-error')) throw new Error('old blocked list error changed privacy error');

  await showBlocked(() => Promise.resolve([oldPeer]));
  const oldUnblockButton = document.getElementById('unblock-1');
  let finishUnblock;
  const oldUnblock = unblockPeer(oldPeer, oldUnblockButton,
    () => new Promise((resolve) => { finishUnblock = resolve; }));
  await switchToNewList();
  finishUnblock();
  await oldUnblock;
  checkNewList('old unblock reloaded new list');

  await showBlocked(() => Promise.resolve([oldPeer]));
  let failUnblock;
  const oldUnblockFailure = unblockPeer(oldPeer, document.getElementById('unblock-1'),
    () => new Promise((_, reject) => { failUnblock = reject; }));
  await switchToNewList();
  failUnblock(new Error('old unblock'));
  await oldUnblockFailure;
  checkNewList('old unblock error reloaded new list');
  if (shown('privacy-error')) throw new Error('old unblock error changed privacy error');

  await showBlocked(() => Promise.resolve([oldPeer]));
  const oldRetryButton = document.getElementById('retry-revocation-1');
  let finishRetry;
  const oldRetry = retryRevocation(oldPeer, oldRetryButton,
    () => new Promise((resolve) => { finishRetry = resolve; }));
  await switchToNewList();
  finishRetry({ revocation: 'notNeeded' });
  await oldRetry;
  checkNewList('old revocation retry changed new settings');

  await showBlocked(() => Promise.resolve([oldPeer]));
  let failRetry;
  const oldRetryFailure = retryRevocation(oldPeer,
    document.getElementById('retry-revocation-1'),
    () => new Promise((_, reject) => { failRetry = reject; }));
  await switchToNewList();
  failRetry(new Error('old retry'));
  await oldRetryFailure;
  checkNewList('old revocation error changed new settings');
};
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
      await checkStaleRender();
      await checkStaleSearchAndChats();
      await checkStalePeerActions();
      await checkStaleSafety();
      await checkStaleProfileLoad();
      await checkStaleProfileHandlers();
      await checkStalePublishAndHide();
      await checkStaleProfileTimers();
      await checkStaleUsageStatus();
      await checkStalePrivacy();
      await checkStaleBlocked();
      await checkStaleSettingsInventory();
      await checkStaleNode();
      await checkStaleTray();
      await checkStaleRemovePreparation();
      await checkStaleWithdrawal();
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
