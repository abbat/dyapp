// Scoped fixture/assertion helpers and shared smoke scenarios.
{
  // Profile loads have two await boundaries; an old save must not repaint Ready either.
  const checkStaleProfilePlace = async (readyState) => {
    const finishPlaceCall = pendingSmokeCall();
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
      return finishPlaceCall.call();
    });
    await new Promise((resolve) => {
      setTimeout(resolve, 0);
    });
    if (!finishPlaceCall.resolve) throw new Error('old place lookup did not start');
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    const place = placeInput.value;
    finishPlaceCall.resolve({ id: 1, name: 'Stale place' });
    await oldPlace;
    if (placeInput.value !== place) throw new Error('old place changed new editor');
  };
  const checkStaleProfileLoad = async () => {
    const readyState = current;
    const finishLoadCall = pendingSmokeCall();
    const oldLoad = showProfile((command) =>
      command === 'load_profile' ? finishLoadCall.call() : Promise.resolve([]),
    );
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    const status = profileStatus.textContent;
    finishLoadCall.resolve({});
    await oldLoad;
    if (profileStatus.textContent !== status)
      throw new Error('old profile load changed new editor');

    await checkStaleProfilePlace(readyState);
    const finishSaveCall = pendingSmokeCall();
    const oldSave = saveProfile(() => finishSaveCall.call());
    if (!finishSaveCall.resolve) throw new Error('old save did not start');
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    const savedStatus = profileStatus.textContent;
    finishSaveCall.resolve({ problem: null, ages: null });
    await oldSave;
    if (profileStatus.textContent !== savedStatus) throw new Error('old save changed new editor');
  };
  // An old form handler's finally must not unlock a newer save or open its preview dialog.
  const checkStaleProfileHandlers = async () => {
    const readyState = current;
    const finishOldSaveCall = pendingSmokeCall();
    const oldSave = submitProfile(() => finishOldSaveCall.call());
    if (!profileBusy) throw new Error('old profile save did not set busy');
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    const finishNewSaveCall = pendingSmokeCall();
    const newSave = submitProfile(() => finishNewSaveCall.call());
    finishOldSaveCall.resolve(null);
    await oldSave;
    if (!profileBusy || !document.getElementById('profile-save').disabled) {
      throw new Error('old save unlocked new form');
    }
    finishNewSaveCall.resolve(null);
    await newSave;
    if (profileBusy) throw new Error('new save remained busy');

    const finishOldPreviewCall = pendingSmokeCall();
    const oldPreview = previewAndPublish(() => finishOldPreviewCall.call());
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    const finishNewPreviewCall = pendingSmokeCall();
    const newPreview = previewAndPublish(() => finishNewPreviewCall.call());
    finishOldPreviewCall.resolve({
      problem: null,
      draft: { birthYear: null, birthMonth: null, birthDay: null },
    });
    await oldPreview;
    if (!profileBusy || publishDialog.open)
      throw new Error('old preview changed new dialog or busy state');
    finishNewPreviewCall.resolve(null);
    await newPreview;
    if (profileBusy || publishDialog.open) throw new Error('new preview did not finish');
  };
  // A late publish or hide must leave the next identity's dialog and busy state alone.
  const assertStalePublishAndHideOldPublishChangedNewDialog = () => {
    if (
      !profileBusy ||
      !publishDialog.open ||
      profileStatus.textContent !== window.strings['profile.solving']
    ) {
      throw new Error('old publish changed new dialog');
    }
  };
  const assertStalePublishAndHideNewPublishDidNotFinish = () => {
    if (
      profileBusy ||
      publishDialog.open ||
      profileStatus.textContent !== `${window.strings['profile.published']} 1`
    ) {
      throw new Error('new publish did not finish');
    }
  };
  const checkStaleProfileHide = async (readyState, published) => {
    profileHidden.checked = true;
    const finishOldHideCall = pendingSmokeCall();
    const oldHide = hideProfile(() => finishOldHideCall.call());
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    profileHidden.checked = true;
    const finishNewHideCall = pendingSmokeCall();
    const newHide = hideProfile(() => finishNewHideCall.call());
    const status = profileStatus.textContent;
    finishOldHideCall.resolve({ ...published, hidden: true });
    await oldHide;
    if (!profileBusy || profileStatus.textContent !== status) {
      throw new Error('old hide changed new editor');
    }
    finishNewHideCall.resolve({ ...published, hidden: true });
    await newHide;
    if (profileBusy || !profileHidden.checked) throw new Error('new hide did not finish');
    await showProfile();
  };
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
    const finishOldPublishCall = pendingSmokeCall();
    const oldPublish = confirmPublish((command) =>
      command === 'solve_profile' ? Promise.resolve() : finishOldPublishCall.call(),
    );
    await new Promise((resolve) => {
      setTimeout(resolve, 0);
    });
    if (!finishOldPublishCall.resolve) throw new Error('old profile publish did not start');
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    openDialog();
    const finishNewSolveCall = pendingSmokeCall();
    const newPublish = confirmPublish((command) =>
      command === 'publish_profile' ? Promise.resolve(published) : finishNewSolveCall.call(),
    );
    finishOldPublishCall.resolve(published);
    await oldPublish;
    assertStalePublishAndHideOldPublishChangedNewDialog();
    finishNewSolveCall.resolve();
    await newPublish;
    assertStalePublishAndHideNewPublishDidNotFinish();

    await checkStaleProfileHide(readyState, published);
  };
  // Search-wait and elapsed timers must retain the identity generation that scheduled them.
  const checkStaleElapsedTimer = async (readyState) => {
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

    const finishSearchCall = pendingSmokeCall();
    const oldSearch = sendSearchProfile(identityGeneration, () => finishSearchCall.call());
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    const status = profileStatus.textContent;
    finishSearchCall.resolve({ searchMinutes: null, hidden: true, published: 1 });
    await oldSearch;
    if (profileStatus.textContent !== status)
      throw new Error('old search send changed new profile');

    await checkStaleElapsedTimer(readyState);
  };
  // Old Settings and status polls must not paint B or schedule another A poll.
  const checkStaleStatusPoll = async (readyState, originalTimeout) => {
    let oldTimers = 0;
    const finishStatusCall = pendingSmokeCall();
    const oldStatus = showStatus(() => finishStatusCall.call());
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    await window.statusShown;
    const traffic = document.getElementById('status-traffic').textContent;
    globalThis.setTimeout = (callback, delay, ...args) => {
      if (delay === 2000) oldTimers += 1;
      return originalTimeout(callback, delay, ...args);
    };
    try {
      finishStatusCall.resolve({ traffic: [1, 2], messages: 1 });
      await oldStatus;
    } finally {
      globalThis.setTimeout = originalTimeout;
    }
    if (document.getElementById('status-traffic').textContent !== traffic || oldTimers) {
      throw new Error('old status changed new status bar or rearmed polling');
    }
  };
  const checkStaleUsageStatus = async () => {
    const readyState = current;
    await openArea('settings', 'node');
    await window.usageShown;
    const finishUsageCall = pendingSmokeCall();
    const oldUsage = showUsage(() => finishUsageCall.call());
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
      finishUsageCall.resolve({
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

    await checkStaleStatusPoll(readyState, originalTimeout);
  };
  // A privacy response from A must leave B's toggles and error state alone.
  const assertStalePrivacyOldPrivacyLoadChangedNewSettings = () => {
    if (
      privacyState() !== 'true,true,false,false,false' ||
      !document.getElementById('presence-nobody').checked ||
      shown('privacy-error')
    ) {
      throw new Error('old privacy load changed new settings');
    }
  };
  const checkStalePrivacyWrites = async (readyState) => {
    const failLoadCall = pendingSmokeCall();
    const oldFailure = showPrivacy(() => failLoadCall.call());
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    await openArea('settings', 'privacy');
    await window.privacyShown;
    failLoadCall.reject(new Error('old load'));
    await oldFailure;
    if (shown('privacy-error')) throw new Error('old privacy load error changed new settings');

    const finishSaveCall = pendingSmokeCall();
    const oldSave = savePrivacy(() => finishSaveCall.call());
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    await openArea('settings', 'privacy');
    await window.privacyShown;
    // The blocked list loads alongside and hides the error when it finishes.
    await window.blockedShown;
    privacyError.hidden = false;
    finishSaveCall.resolve();
    await oldSave;
    if (privacyError.hidden) throw new Error('old privacy save cleared new error');
    await savePrivacy(() => Promise.resolve());
    if (shown('privacy-error')) throw new Error('current privacy save did not clear error');

    const failSaveCall = pendingSmokeCall();
    const oldSaveFailure = savePrivacy(() => failSaveCall.call());
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    await openArea('settings', 'privacy');
    await window.privacyShown;
    failSaveCall.reject(new Error('old save'));
    await oldSaveFailure;
    if (shown('privacy-error')) throw new Error('old privacy save error changed new settings');
  };
  const checkStalePrivacy = async () => {
    const readyState = current;
    await openArea('settings', 'privacy');
    await window.privacyShown;
    const finishLoadCall = pendingSmokeCall();
    const oldLoad = showPrivacy(() => finishLoadCall.call());
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
    finishLoadCall.resolve({
      sendViews: false,
      sendReadReceipts: false,
      sendTyping: true,
      presenceMatches: true,
      notifications: true,
      noticeContent: 'fixed',
    });
    await oldLoad;
    assertStalePrivacyOldPrivacyLoadChangedNewSettings();

    await checkStalePrivacyWrites(readyState);
  };
  // Blocked-peer loads and actions from A must not repaint B's list or status.
  const checkStaleBlockedActions = async (oldPeer, switchToNewList, checkNewList) => {
    await showBlocked(() => Promise.resolve([oldPeer]));
    const oldUnblockButton = document.getElementById('unblock-1');
    const finishUnblockCall = pendingSmokeCall();
    const oldUnblock = unblockPeer(oldPeer, oldUnblockButton, () => finishUnblockCall.call());
    await switchToNewList();
    finishUnblockCall.resolve();
    await oldUnblock;
    checkNewList('old unblock reloaded new list');

    await showBlocked(() => Promise.resolve([oldPeer]));
    const failUnblockCall = pendingSmokeCall();
    const oldUnblockFailure = unblockPeer(oldPeer, document.getElementById('unblock-1'), () =>
      failUnblockCall.call(),
    );
    await switchToNewList();
    failUnblockCall.reject(new Error('old unblock'));
    await oldUnblockFailure;
    checkNewList('old unblock error reloaded new list');
    if (shown('privacy-error')) throw new Error('old unblock error changed privacy error');

    await showBlocked(() => Promise.resolve([oldPeer]));
    const oldRetryButton = document.getElementById('retry-revocation-1');
    const finishRetryCall = pendingSmokeCall();
    const oldRetry = retryRevocation(oldPeer, oldRetryButton, () => finishRetryCall.call());
    await switchToNewList();
    finishRetryCall.resolve({ revocation: 'notNeeded' });
    await oldRetry;
    checkNewList('old revocation retry changed new settings');

    await showBlocked(() => Promise.resolve([oldPeer]));
    const failRetryCall = pendingSmokeCall();
    const oldRetryFailure = retryRevocation(
      oldPeer,
      document.getElementById('retry-revocation-1'),
      () => failRetryCall.call(),
    );
    await switchToNewList();
    failRetryCall.reject(new Error('old retry'));
    await oldRetryFailure;
    checkNewList('old revocation error changed new settings');
  };
  const staleBlockedFixture = () => {
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
    return { oldPeer, switchToNewList, checkNewList };
  };
  const checkStaleBlocked = async () => {
    const { oldPeer, switchToNewList, checkNewList } = staleBlockedFixture();
    await openArea('settings', 'blocked');
    await window.blockedShown;
    const finishLoadCall = pendingSmokeCall();
    const oldLoad = showBlocked(() => finishLoadCall.call());
    await switchToNewList();
    finishLoadCall.resolve([oldPeer]);
    await oldLoad;
    checkNewList('old blocked list changed new settings');

    const failLoadCall = pendingSmokeCall();
    const oldFailure = showBlocked(() => failLoadCall.call());
    await switchToNewList();
    failLoadCall.reject(new Error('old list'));
    await oldFailure;
    checkNewList('old blocked list error changed new settings');
    if (shown('privacy-error')) throw new Error('old blocked list error changed privacy error');

    await checkStaleBlockedActions(oldPeer, switchToNewList, checkNewList);
  };
  globalThis.uiSmokeIdentityProfile = Object.freeze({
    checkStaleProfileLoad,
    checkStaleProfileHandlers,
    checkStalePublishAndHide,
    checkStaleProfileTimers,
    checkStaleUsageStatus,
    checkStalePrivacy,
    checkStaleBlocked,
  });
}
