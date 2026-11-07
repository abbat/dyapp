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
    await window.settingsShown;
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
  const short = window.shortPeerId;
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
const chooseStorage = async (mode, seed) => {
  const choice = document.getElementById(`${mode}-choice`);
  if (!choice || choice.disabled || choice.hidden) throw new Error('storage choice unavailable');
  const rendered = hiddenRendered();
  if (rendered.length) throw new Error(`hidden but rendered: ${rendered}`);
  if (
    !document.getElementById('switch-identity').hidden ||
    !document.getElementById('remove-identity').hidden
  )
    throw new Error('identity actions on setup');
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
  await shot('setup');
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
  await shot('wizard-network');
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
    await window.copied;
  } finally {
    delete navigator.clipboard.writeText;
  }
  if (
    copied.join() !== source.textContent ||
    button.textContent !== window.strings['action.copied']
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
  await shot('picker');
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
  await shot('settings');
  const active = [...list.children].filter((item) => item.dataset.active === 'true');
  const short = window.shortPeerId(peerId, expected.map((item) => item.peerId).filter(Boolean));
  if (
    active.length !== 1 ||
    active[0].dataset.peerId !== peerId ||
    !active[0].textContent.includes(short) ||
    !active[0].textContent.includes(window.strings[`identity.mode.${mode}`])
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
  const readyState = current;
  let finishOld;
  const old = act(
    new Promise((resolve) => {
      finishOld = resolve;
    }),
  );
  await act(Promise.resolve({ status: 'picker' }));
  if (document.getElementById('picker').hidden) throw new Error('picker not shown');
  finishOld({ status: 'setup' });
  await old;
  if (document.getElementById('picker').hidden) throw new Error('stale identity response rendered');
  await act(Promise.resolve(readyState));
  if (document.getElementById('identity').hidden) throw new Error('ready view not restored');
  // A failure that arrives after a later transition stays silent.
  let failOld;
  const oldFailure = act(
    new Promise((_, reject) => {
      failOld = reject;
    }),
  );
  await act(Promise.resolve({ status: 'picker' }));
  failOld(new Error('refused'));
  await oldFailure;
  if (document.getElementById('picker').hidden) throw new Error('stale failure rendered');
  await act(Promise.resolve(readyState));
  // A failed switch or removal offers a way back to the identity the backend kept.
  const retry = document.getElementById('boot-retry');
  await act(Promise.reject(new Error('refused')));
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
// A result from an old identity must not paint or unlock controls for a newer one.
const checkStaleSearchAndChats = async () => {
  const readyState = current;
  await openArea('search-area');
  let finishOld;
  const noNames = (run) => (command) => (command === 'contact_names' ? Promise.resolve([]) : run());
  const oldSearch = runSearch(
    noNames(
      () =>
        new Promise((resolve) => {
          finishOld = resolve;
        }),
    ),
  );
  if (!searchBusy) throw new Error('old search did not start');
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  await openArea('search-area');
  let finishCurrent;
  const newSearch = runSearch(
    noNames(
      () =>
        new Promise((resolve) => {
          finishCurrent = resolve;
        }),
    ),
  );
  finishOld({ profiles: [], offered: [], partial: false, failures: [] });
  await oldSearch;
  if (
    !searchBusy ||
    document.getElementById('search-status').textContent !== window.strings['search.loading']
  )
    throw new Error('old search changed new search state');
  finishCurrent({ profiles: [], offered: [], partial: false, failures: [] });
  await newSearch;
  if (
    searchBusy ||
    document.getElementById('search-status').textContent !== window.strings['search.empty']
  )
    throw new Error('current search did not finish');

  await openArea('chats-area');
  await window.chatsDone;
  let finishMail;
  let oldLikedCalls = 0;
  const oldChats = window.showChats((command) => {
    if (command === 'liked_you') {
      oldLikedCalls += 1;
      return Promise.resolve([]);
    }
    return new Promise((resolve) => {
      finishMail = resolve;
    });
  });
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  let finishNewMail;
  const newChats = window.showChats((command) =>
    ['liked_you', 'chat_rows'].includes(command)
      ? Promise.resolve([])
      : new Promise((resolve) => {
          finishNewMail = resolve;
        }),
  );
  finishMail(1);
  await oldChats;
  if (
    oldLikedCalls ||
    document.getElementById('chats-status').textContent !== window.strings['chats.loading']
  )
    throw new Error('old mailbox changed new Chats');
  finishNewMail(0);
  await newChats;
  if (document.getElementById('chats-status').textContent !== window.strings['chats.none']) {
    throw new Error('current Chats did not finish');
  }

  await openArea('chats-area');
  await window.chatsDone;
  let finishRows;
  const oldRefresh = window.mailUpdated(
    null,
    () =>
      new Promise((resolve) => {
        finishRows = resolve;
      }),
  );
  await act(Promise.resolve({ status: 'picker' }));
  const shownRows = chatList.childElementCount;
  finishRows([{ peerId: '6'.repeat(64), state: 'open', lastMessage: null, unreadCount: 0 }]);
  await oldRefresh;
  if (chatList.childElementCount !== shownRows) throw new Error('old refresh rendered Chats rows');
  await act(Promise.resolve(readyState));
};
// Even reopening the same card object for a new identity cannot admit an old peer response.
const checkStalePeerActions = async () => {
  const readyState = current;
  const found = {
    peerId: 'f'.repeat(64),
    draft: { birthYear: null, birthMonth: null, birthDay: null },
    place: '',
  };
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
  openFound(
    found,
    card,
    () =>
      new Promise((resolve) => {
        finishLikes = resolve;
      }),
  );
  await switchBack();
  finishLikes({ ours: true, matched: true });
  await Promise.resolve();
  if (document.getElementById('view-like').hidden)
    throw new Error('old peer likes changed new card');

  let finishOldLike;
  const oldLike = sendLike(
    true,
    () =>
      new Promise((resolve) => {
        finishOldLike = resolve;
      }),
  );
  await switchBack();
  let finishNewLike;
  const newLike = sendLike(
    true,
    () =>
      new Promise((resolve) => {
        finishNewLike = resolve;
      }),
  );
  finishOldLike({ ours: true, matched: true });
  await oldLike;
  if (
    !document.getElementById('view-like').disabled ||
    viewStatus.textContent !== window.strings['view.sending']
  ) {
    throw new Error('old like changed new controls');
  }
  finishNewLike({ ours: true, matched: false });
  await newLike;
  if (
    document.getElementById('view-like').disabled ||
    !document.getElementById('view-like').hidden
  ) {
    throw new Error('current like did not finish');
  }

  let finishBlock;
  const oldBlock = blockViewed(
    () =>
      new Promise((resolve) => {
        finishBlock = resolve;
      }),
  );
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
  const found = {
    peerId: 'e'.repeat(64),
    draft: { birthYear: null, birthMonth: null, birthDay: null },
    place: '',
  };
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
  const oldSafety = loadSafety((command) =>
    command === 'contact_safety_qr'
      ? Promise.resolve(matrix)
      : new Promise((resolve) => {
          finishOldNumber = resolve;
        }),
  );
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  await reopen();
  let finishNewNumber;
  const newSafety = loadSafety((command) =>
    command === 'contact_safety_qr'
      ? Promise.resolve(matrix)
      : new Promise((resolve) => {
          finishNewNumber = resolve;
        }),
  );
  finishOldNumber(number);
  await oldSafety;
  if (
    !document.getElementById('view-safety').disabled ||
    !document.getElementById('view-safety-panel').hidden
  ) {
    throw new Error('old safety result changed new panel');
  }
  finishNewNumber(number);
  await newSafety;
  if (
    document.getElementById('view-safety').disabled ||
    document.getElementById('view-safety-panel').hidden
  ) {
    throw new Error('current safety result did not finish');
  }

  const mediaDescriptor = Object.getOwnPropertyDescriptor(navigator, 'mediaDevices');
  let finishCapture;
  let stopped = false;
  Object.defineProperty(navigator, 'mediaDevices', {
    configurable: true,
    value: {
      getUserMedia: () =>
        new Promise((resolve) => {
          finishCapture = resolve;
        }),
    },
  });
  try {
    document.getElementById('view-safety-camera').click();
    await act(Promise.resolve({ status: 'picker' }));
    finishCapture({
      getTracks: () => [
        {
          stop: () => {
            stopped = true;
          },
        },
      ],
    });
    await window.cameraDone;
    if (!stopped) throw new Error('old camera stream was not stopped');
  } finally {
    if (mediaDescriptor) Object.defineProperty(navigator, 'mediaDevices', mediaDescriptor);
    else delete navigator.mediaDevices;
  }
  await act(Promise.resolve(readyState));
  await reopen();
  let activeStopped = false;
  cameraStream = {
    getTracks: () => [
      {
        stop: () => {
          activeStopped = true;
        },
      },
    ],
  };
  const leaving = act(Promise.resolve({ status: 'picker' }));
  if (
    !activeStopped ||
    cameraStream !== null ||
    cameraPending ||
    document.getElementById('view-safety-camera').textContent !==
      window.strings['view.safety.camera.start']
  ) {
    throw new Error('active camera stream survived identity transition');
  }
  await leaving;
  await act(Promise.resolve(readyState));
  await reopen();
  // Failures and answers for an earlier identity leave the mark off; only a current answer sets it.
  const safetyAnswers = (command) =>
    Promise.resolve(command === 'contact_safety_qr' ? matrix : number);
  const mark = document.getElementById('view-verified');
  const confirmStatus = document.getElementById('view-safety-scan-status');
  await loadSafety(safetyAnswers);
  for (const code of [
    'notConnected',
    'unavailable',
    'unknownPeer',
    'staleProfile',
    'unpublished',
  ]) {
    await confirmContact(() => Promise.reject(code));
    if (
      !mark.hidden ||
      confirmStatus.textContent !== window.strings[`view.safety.confirm.${code}`]
    ) {
      throw new Error(`confirmation error ${code} was not shown or changed the mark`);
    }
  }
  await confirmContact(() => Promise.reject('storage is broken'));
  if (!mark.hidden || confirmStatus.textContent !== window.strings['view.safety.confirm.failed']) {
    throw new Error('an unknown confirmation error was not shown');
  }
  let finishOldConfirm;
  const oldConfirm = confirmContact(
    () =>
      new Promise((resolve) => {
        finishOldConfirm = resolve;
      }),
  );
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  await reopen();
  finishOldConfirm({ ...found, version: 9 });
  await oldConfirm;
  if (!mark.hidden || found.version === 9 || document.getElementById('view-confirm').disabled) {
    throw new Error('old confirmation changed the new view');
  }
  await loadSafety(safetyAnswers);
  await confirmContact(() => Promise.resolve({ ...found, version: 2 }));
  if (
    mark.hidden ||
    found.version !== 2 ||
    !document.getElementById('view-confirm').hidden ||
    document.getElementById('view-safety-state').textContent !==
      window.strings['view.safety.verified'] ||
    document.activeElement.id !== 'view-unverify'
  ) {
    throw new Error('the fresh profile did not show the verified mark');
  }
  await shot('search-safety-verified');
  await removeMark(() => Promise.reject(new Error('locked')));
  if (
    mark.hidden ||
    confirmStatus.textContent !== window.strings['view.safety.confirm.removeFailed']
  ) {
    throw new Error('a failed removal changed the mark');
  }
  await removeMark(() => Promise.resolve());
  if (
    !mark.hidden ||
    document.getElementById('view-confirm').hidden ||
    document.activeElement.id !== 'view-confirm'
  ) {
    throw new Error('the mark was not removed');
  }
  await reopen();
  const copyButton = document.getElementById('copy-safety-number');
  const copyLabel = copyButton.textContent;
  const originalTimeout = globalThis.setTimeout;
  const expiries = [];
  globalThis.setTimeout = (callback, delay, ...args) => {
    if (delay === 60000) {
      expiries.push(callback);
      return 0;
    }
    return originalTimeout(callback, delay, ...args);
  };
  try {
    let finishCopy;
    const oldCopy = copySafety(
      () =>
        new Promise((resolve) => {
          finishCopy = resolve;
        }),
    );
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
  const oldLoad = showProfile((command) =>
    command === 'load_profile'
      ? new Promise((resolve) => {
          finishLoad = resolve;
        })
      : Promise.resolve([]),
  );
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  const status = profileStatus.textContent;
  finishLoad({});
  await oldLoad;
  if (profileStatus.textContent !== status) throw new Error('old profile load changed new editor');

  let finishPlace;
  const oldPlace = showProfile((command) => {
    if (command === 'profile_scales' || command === 'place_countries') return Promise.resolve([]);
    if (command === 'load_profile')
      return Promise.resolve({
        draft: {
          name: '',
          country: 'DE',
          place: '1',
          birthYear: null,
          birthMonth: null,
          birthDay: null,
        },
        searchMinutes: null,
        published: 0,
        hidden: false,
        ages: null,
      });
    return new Promise((resolve) => {
      finishPlace = resolve;
    });
  });
  await new Promise((resolve) => {
    setTimeout(resolve, 0);
  });
  if (!finishPlace) throw new Error('old place lookup did not start');
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  const place = placeInput.value;
  finishPlace({ id: 1, name: 'Stale place' });
  await oldPlace;
  if (placeInput.value !== place) throw new Error('old place changed new editor');

  let finishSave;
  const oldSave = saveProfile(
    () =>
      new Promise((resolve) => {
        finishSave = resolve;
      }),
  );
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
  const oldSave = submitProfile(
    () =>
      new Promise((resolve) => {
        finishOldSave = resolve;
      }),
  );
  if (!profileBusy) throw new Error('old profile save did not set busy');
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  let finishNewSave;
  const newSave = submitProfile(
    () =>
      new Promise((resolve) => {
        finishNewSave = resolve;
      }),
  );
  finishOldSave(null);
  await oldSave;
  if (!profileBusy || !document.getElementById('profile-save').disabled) {
    throw new Error('old save unlocked new form');
  }
  finishNewSave(null);
  await newSave;
  if (profileBusy) throw new Error('new save remained busy');

  let finishOldPreview;
  const oldPreview = previewAndPublish(
    () =>
      new Promise((resolve) => {
        finishOldPreview = resolve;
      }),
  );
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  let finishNewPreview;
  const newPreview = previewAndPublish(
    () =>
      new Promise((resolve) => {
        finishNewPreview = resolve;
      }),
  );
  finishOldPreview({ problem: null, draft: { birthYear: null, birthMonth: null, birthDay: null } });
  await oldPreview;
  if (!profileBusy || publishDialog.open)
    throw new Error('old preview changed new dialog or busy state');
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
  const oldPublish = confirmPublish((command) =>
    command === 'solve_profile'
      ? Promise.resolve()
      : new Promise((resolve) => {
          finishOldPublish = resolve;
        }),
  );
  await new Promise((resolve) => {
    setTimeout(resolve, 0);
  });
  if (!finishOldPublish) throw new Error('old profile publish did not start');
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  openDialog();
  let finishNewSolve;
  const newPublish = confirmPublish((command) =>
    command === 'publish_profile'
      ? Promise.resolve(published)
      : new Promise((resolve) => {
          finishNewSolve = resolve;
        }),
  );
  finishOldPublish(published);
  await oldPublish;
  if (
    !profileBusy ||
    !publishDialog.open ||
    profileStatus.textContent !== window.strings['profile.solving']
  ) {
    throw new Error('old publish changed new dialog');
  }
  finishNewSolve();
  await newPublish;
  if (
    profileBusy ||
    publishDialog.open ||
    profileStatus.textContent !== `${window.strings['profile.published']} 1`
  ) {
    throw new Error('new publish did not finish');
  }

  profileHidden.checked = true;
  let finishOldHide;
  const oldHide = hideProfile(
    () =>
      new Promise((resolve) => {
        finishOldHide = resolve;
      }),
  );
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  profileHidden.checked = true;
  let finishNewHide;
  const newHide = hideProfile(
    () =>
      new Promise((resolve) => {
        finishNewHide = resolve;
      }),
  );
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
    if (delay === 60000) {
      oldTick = callback;
      return 0;
    }
    return originalTimeout(callback, delay, ...args);
  };
  try {
    showSearchWait({ searchMinutes: 1 }, () => {
      sent += 1;
    });
  } finally {
    globalThis.setTimeout = originalTimeout;
  }
  if (!oldTick) throw new Error('search wait timer was not scheduled');
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  oldTick();
  if (sent) throw new Error('old search wait timer ran on new identity');

  let finishSearch;
  const oldSearch = sendSearchProfile(
    identityGeneration,
    () =>
      new Promise((resolve) => {
        finishSearch = resolve;
      }),
  );
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  const status = profileStatus.textContent;
  finishSearch({ searchMinutes: null, hidden: true, published: 1 });
  await oldSearch;
  if (profileStatus.textContent !== status) throw new Error('old search send changed new profile');

  const originalInterval = globalThis.setInterval;
  let elapsedTick;
  globalThis.setInterval = (callback, delay, ...args) => {
    if (delay === 1000) {
      elapsedTick = callback;
      return 0;
    }
    return originalInterval(callback, delay, ...args);
  };
  try {
    startElapsed();
  } finally {
    globalThis.setInterval = originalInterval;
  }
  if (!elapsedTick) throw new Error('publish elapsed timer was not scheduled');
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  elapsed.textContent = 'new identity';
  elapsedTick();
  if (elapsed.textContent !== 'new identity')
    throw new Error('old elapsed timer changed new dialog');
};
// Old Settings and status polls must not paint B or schedule another A poll.
const checkStaleUsageStatus = async () => {
  const readyState = current;
  await openArea('settings', 'node');
  await window.usageShown;
  let finishUsage;
  const oldUsage = showUsage(
    () =>
      new Promise((resolve) => {
        finishUsage = resolve;
      }),
  );
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  await openArea('settings', 'node');
  await window.usageShown;
  const path = document.getElementById('usage-identity-dir').textContent;
  const originalTimeout = globalThis.setTimeout;
  let oldTimers = 0;
  globalThis.setTimeout = (callback, delay, ...args) => {
    if (delay === 10000) oldTimers += 1;
    return originalTimeout(callback, delay, ...args);
  };
  try {
    finishUsage({
      identityDir: '/old',
      messages: 0,
      messagesPath: '/old',
      nodePath: null,
      node: null,
      nodeLimit: null,
      free: null,
    });
    await oldUsage;
  } finally {
    globalThis.setTimeout = originalTimeout;
  }
  if (document.getElementById('usage-identity-dir').textContent !== path || oldTimers) {
    throw new Error('old usage changed new storage card or rearmed polling');
  }

  let finishStatus;
  const oldStatus = showStatus(
    () =>
      new Promise((resolve) => {
        finishStatus = resolve;
      }),
  );
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
  } finally {
    globalThis.setTimeout = originalTimeout;
  }
  if (document.getElementById('status-traffic').textContent !== traffic || oldTimers) {
    throw new Error('old status changed new status bar or rearmed polling');
  }
};
// A privacy response from A must leave B's toggles and error state alone.
const checkStalePrivacy = async () => {
  const readyState = current;
  await openArea('settings', 'privacy');
  await window.privacyShown;
  let finishLoad;
  const oldLoad = showPrivacy(
    () =>
      new Promise((resolve) => {
        finishLoad = resolve;
      }),
  );
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  await openArea('settings', 'privacy');
  await window.privacyShown;
  const newer = {
    sendViews: true,
    sendReadReceipts: true,
    sendTyping: false,
    presenceMatches: false,
    notifications: false,
    noticeContent: 'name',
  };
  await showPrivacy(() => Promise.resolve(newer));
  finishLoad({
    sendViews: false,
    sendReadReceipts: false,
    sendTyping: true,
    presenceMatches: true,
    notifications: true,
    noticeContent: 'fixed',
  });
  await oldLoad;
  if (
    privacyState() !== 'true,true,false,false,false' ||
    !document.getElementById('presence-nobody').checked ||
    shown('privacy-error')
  ) {
    throw new Error('old privacy load changed new settings');
  }

  let failLoad;
  const oldFailure = showPrivacy(
    () =>
      new Promise((_, reject) => {
        failLoad = reject;
      }),
  );
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  await openArea('settings', 'privacy');
  await window.privacyShown;
  failLoad(new Error('old load'));
  await oldFailure;
  if (shown('privacy-error')) throw new Error('old privacy load error changed new settings');

  let finishSave;
  const oldSave = savePrivacy(
    () =>
      new Promise((resolve) => {
        finishSave = resolve;
      }),
  );
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  await openArea('settings', 'privacy');
  await window.privacyShown;
  // The blocked list loads alongside and hides the error when it finishes.
  await window.blockedShown;
  privacyError.hidden = false;
  finishSave();
  await oldSave;
  if (privacyError.hidden) throw new Error('old privacy save cleared new error');
  await savePrivacy(() => Promise.resolve());
  if (shown('privacy-error')) throw new Error('current privacy save did not clear error');

  let failSave;
  const oldSaveFailure = savePrivacy(
    () =>
      new Promise((_, reject) => {
        failSave = reject;
      }),
  );
  await act(Promise.resolve({ status: 'picker' }));
  await act(Promise.resolve(readyState));
  await openArea('settings', 'privacy');
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
    await openArea('settings', 'blocked');
    await window.blockedShown;
    await showBlocked(() => Promise.resolve([newPeer]));
    status.textContent = 'new identity';
  };
  const checkNewList = (message) => {
    if (
      list.children.length !== 1 ||
      !list.textContent.includes(newPeer.slice(-6)) ||
      status.textContent !== 'new identity' ||
      document.getElementById('unblock-1').disabled ||
      document.getElementById('retry-revocation-1').disabled
    )
      throw new Error(message);
  };
  await openArea('settings', 'blocked');
  await window.blockedShown;
  let finishLoad;
  const oldLoad = showBlocked(
    () =>
      new Promise((resolve) => {
        finishLoad = resolve;
      }),
  );
  await switchToNewList();
  finishLoad([oldPeer]);
  await oldLoad;
  checkNewList('old blocked list changed new settings');

  let failLoad;
  const oldFailure = showBlocked(
    () =>
      new Promise((_, reject) => {
        failLoad = reject;
      }),
  );
  await switchToNewList();
  failLoad(new Error('old list'));
  await oldFailure;
  checkNewList('old blocked list error changed new settings');
  if (shown('privacy-error')) throw new Error('old blocked list error changed privacy error');

  await showBlocked(() => Promise.resolve([oldPeer]));
  const oldUnblockButton = document.getElementById('unblock-1');
  let finishUnblock;
  const oldUnblock = unblockPeer(
    oldPeer,
    oldUnblockButton,
    () =>
      new Promise((resolve) => {
        finishUnblock = resolve;
      }),
  );
  await switchToNewList();
  finishUnblock();
  await oldUnblock;
  checkNewList('old unblock reloaded new list');

  await showBlocked(() => Promise.resolve([oldPeer]));
  let failUnblock;
  const oldUnblockFailure = unblockPeer(
    oldPeer,
    document.getElementById('unblock-1'),
    () =>
      new Promise((_, reject) => {
        failUnblock = reject;
      }),
  );
  await switchToNewList();
  failUnblock(new Error('old unblock'));
  await oldUnblockFailure;
  checkNewList('old unblock error reloaded new list');
  if (shown('privacy-error')) throw new Error('old unblock error changed privacy error');

  await showBlocked(() => Promise.resolve([oldPeer]));
  const oldRetryButton = document.getElementById('retry-revocation-1');
  let finishRetry;
  const oldRetry = retryRevocation(
    oldPeer,
    oldRetryButton,
    () =>
      new Promise((resolve) => {
        finishRetry = resolve;
      }),
  );
  await switchToNewList();
  finishRetry({ revocation: 'notNeeded' });
  await oldRetry;
  checkNewList('old revocation retry changed new settings');

  await showBlocked(() => Promise.resolve([oldPeer]));
  let failRetry;
  const oldRetryFailure = retryRevocation(
    oldPeer,
    document.getElementById('retry-revocation-1'),
    () =>
      new Promise((_, reject) => {
        failRetry = reject;
      }),
  );
  await switchToNewList();
  failRetry(new Error('old retry'));
  await oldRetryFailure;
  checkNewList('old revocation error changed new settings');
};
// These fragments are concatenated into one WebView eval; expose the shared test API explicitly.
globalThis.uiSmokeIdentity = Object.freeze({
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
