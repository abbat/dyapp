// Scoped fixture/assertion helpers and shared smoke scenarios.
{
  // Profile loads have two await boundaries; an old save must not repaint Ready either.
  const checkStaleProfilePlace = async (readyState) => {
    const finishPlaceCall = smoke.pendingSmokeCall();
    const oldPlace = appBindings.showProfile((command) => {
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
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    const place = appBindings.placeInput.value;
    finishPlaceCall.resolve({ id: 1, name: 'Stale place' });
    await oldPlace;
    if (appBindings.placeInput.value !== place) throw new Error('old place changed new editor');
  };
  const checkStaleProfileLoad = async () => {
    const readyState = appBindings.current;
    const finishLoadCall = smoke.pendingSmokeCall();
    const oldLoad = appBindings.showProfile((command) =>
      command === 'load_profile' ? finishLoadCall.call() : Promise.resolve([]),
    );
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    const status = appBindings.profileStatus.textContent;
    finishLoadCall.resolve({});
    await oldLoad;
    if (appBindings.profileStatus.textContent !== status)
      throw new Error('old profile load changed new editor');

    await checkStaleProfilePlace(readyState);
    const finishSaveCall = smoke.pendingSmokeCall();
    const oldSave = appBindings.saveProfile(() => finishSaveCall.call());
    if (!finishSaveCall.resolve) throw new Error('old save did not start');
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    const savedStatus = appBindings.profileStatus.textContent;
    finishSaveCall.resolve({ problem: null, ages: null });
    await oldSave;
    if (appBindings.profileStatus.textContent !== savedStatus)
      throw new Error('old save changed new editor');
  };
  // An old form handler's finally must not unlock a newer save or open its preview dialog.
  const checkStaleProfileHandlers = async () => {
    const readyState = appBindings.current;
    const finishOldSaveCall = smoke.pendingSmokeCall();
    const oldSave = appBindings.submitProfile(() => finishOldSaveCall.call());
    if (!appBindings.profileBusy) throw new Error('old profile save did not set busy');
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    const finishNewSaveCall = smoke.pendingSmokeCall();
    const newSave = appBindings.submitProfile(() => finishNewSaveCall.call());
    finishOldSaveCall.resolve(null);
    await oldSave;
    if (!appBindings.profileBusy || !document.getElementById('profile-save').disabled) {
      throw new Error('old save unlocked new form');
    }
    finishNewSaveCall.resolve(null);
    await newSave;
    if (appBindings.profileBusy) throw new Error('new save remained busy');

    const finishOldPreviewCall = smoke.pendingSmokeCall();
    const oldPreview = appBindings.previewAndPublish(() => finishOldPreviewCall.call());
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    const finishNewPreviewCall = smoke.pendingSmokeCall();
    const newPreview = appBindings.previewAndPublish(() => finishNewPreviewCall.call());
    finishOldPreviewCall.resolve({
      problem: null,
      draft: { birthYear: null, birthMonth: null, birthDay: null },
    });
    await oldPreview;
    if (!appBindings.profileBusy || appBindings.publishDialog.open)
      throw new Error('old preview changed new dialog or busy state');
    finishNewPreviewCall.resolve(null);
    await newPreview;
    if (appBindings.profileBusy || appBindings.publishDialog.open)
      throw new Error('new preview did not finish');
  };
  // A late publish or hide must leave the next identity's dialog and busy state alone.
  const assertStalePublishAndHideOldPublishChangedNewDialog = () => {
    if (
      !appBindings.profileBusy ||
      !appBindings.publishDialog.open ||
      appBindings.profileStatus.textContent !== appApi.strings['profile.solving']
    ) {
      throw new Error('old publish changed new dialog');
    }
  };
  const assertStalePublishAndHideNewPublishDidNotFinish = () => {
    if (
      appBindings.profileBusy ||
      appBindings.publishDialog.open ||
      appBindings.profileStatus.textContent !== `${appApi.strings['profile.published']} 1`
    ) {
      throw new Error('new publish did not finish');
    }
  };
  const checkStaleProfileHide = async (readyState, published) => {
    appBindings.profileHidden.checked = true;
    const finishOldHideCall = smoke.pendingSmokeCall();
    const oldHide = appBindings.hideProfile(() => finishOldHideCall.call());
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    appBindings.profileHidden.checked = true;
    const finishNewHideCall = smoke.pendingSmokeCall();
    const newHide = appBindings.hideProfile(() => finishNewHideCall.call());
    const status = appBindings.profileStatus.textContent;
    finishOldHideCall.resolve({ ...published, hidden: true });
    await oldHide;
    if (!appBindings.profileBusy || appBindings.profileStatus.textContent !== status) {
      throw new Error('old hide changed new editor');
    }
    finishNewHideCall.resolve({ ...published, hidden: true });
    await newHide;
    if (appBindings.profileBusy || !appBindings.profileHidden.checked)
      throw new Error('new hide did not finish');
    await appBindings.showProfile();
  };
  const checkStalePublishAndHide = async () => {
    const readyState = appBindings.current;
    const published = { published: 1, hidden: false, searchMinutes: null };
    const openDialog = () => {
      appBindings.openPublishDialog({ draft: appBindings.profileDraft() });
      appBindings.profileAttested.checked = true;
      appBindings.profileDisclosure.checked = true;
      appBindings.publishConfirm.disabled = false;
    };
    openDialog();
    const finishOldPublishCall = smoke.pendingSmokeCall();
    const oldPublish = appBindings.confirmPublish((command) =>
      command === 'solve_profile' ? Promise.resolve() : finishOldPublishCall.call(),
    );
    await new Promise((resolve) => {
      setTimeout(resolve, 0);
    });
    if (!finishOldPublishCall.resolve) throw new Error('old profile publish did not start');
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    openDialog();
    const finishNewSolveCall = smoke.pendingSmokeCall();
    const newPublish = appBindings.confirmPublish((command) =>
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
      appBindings.startElapsed();
    } finally {
      globalThis.setInterval = originalInterval;
    }
    if (!elapsedTick) throw new Error('publish elapsed timer was not scheduled');
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    appBindings.elapsed.textContent = 'new identity';
    elapsedTick();
    if (appBindings.elapsed.textContent !== 'new identity')
      throw new Error('old elapsed timer changed new dialog');
  };
  const checkStaleProfileTimers = async () => {
    const readyState = appBindings.current;
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
      appBindings.showSearchWait({ searchMinutes: 1 }, () => {
        sent += 1;
      });
    } finally {
      globalThis.setTimeout = originalTimeout;
    }
    if (!oldTick) throw new Error('search wait timer was not scheduled');
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    oldTick();
    if (sent) throw new Error('old search wait timer ran on new identity');

    const finishSearchCall = smoke.pendingSmokeCall();
    const oldSearch = appBindings.sendSearchProfile(appBindings.identityGeneration, () =>
      finishSearchCall.call(),
    );
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    const status = appBindings.profileStatus.textContent;
    finishSearchCall.resolve({ searchMinutes: null, hidden: true, published: 1 });
    await oldSearch;
    if (appBindings.profileStatus.textContent !== status)
      throw new Error('old search send changed new profile');

    await checkStaleElapsedTimer(readyState);
  };
  // Old Settings and status polls must not paint B or schedule another A poll.
  const checkStaleStatusPoll = async (readyState, originalTimeout) => {
    let oldTimers = 0;
    const finishStatusCall = smoke.pendingSmokeCall();
    const oldStatus = appBindings.showStatus(() => finishStatusCall.call());
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    await appApi.statusShown;
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
    const readyState = appBindings.current;
    await openArea('settings', 'node');
    await appApi.usageShown;
    const finishUsageCall = smoke.pendingSmokeCall();
    const oldUsage = appBindings.showUsage(() => finishUsageCall.call());
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    await openArea('settings', 'node');
    await appApi.usageShown;
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
    const failLoadCall = smoke.pendingSmokeCall();
    const oldFailure = appBindings.showPrivacy(() => failLoadCall.call());
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    await openArea('settings', 'privacy');
    await appApi.privacyShown;
    failLoadCall.reject(new Error('old load'));
    await oldFailure;
    if (shown('privacy-error')) throw new Error('old privacy load error changed new settings');

    const finishSaveCall = smoke.pendingSmokeCall();
    const oldSave = appBindings.savePrivacy(() => finishSaveCall.call());
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    await openArea('settings', 'privacy');
    await appApi.privacyShown;
    // The blocked list loads alongside and hides the error when it finishes.
    await appApi.blockedShown;
    appBindings.privacyError.hidden = false;
    finishSaveCall.resolve();
    await oldSave;
    if (appBindings.privacyError.hidden) throw new Error('old privacy save cleared new error');
    await appBindings.savePrivacy(() => Promise.resolve());
    if (shown('privacy-error')) throw new Error('current privacy save did not clear error');

    const failSaveCall = smoke.pendingSmokeCall();
    const oldSaveFailure = appBindings.savePrivacy(() => failSaveCall.call());
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    await openArea('settings', 'privacy');
    await appApi.privacyShown;
    failSaveCall.reject(new Error('old save'));
    await oldSaveFailure;
    if (shown('privacy-error')) throw new Error('old privacy save error changed new settings');
  };
  const checkStalePrivacy = async () => {
    const readyState = appBindings.current;
    await openArea('settings', 'privacy');
    await appApi.privacyShown;
    const finishLoadCall = smoke.pendingSmokeCall();
    const oldLoad = appBindings.showPrivacy(() => finishLoadCall.call());
    await appBindings.act(Promise.resolve({ status: 'picker' }));
    await appBindings.act(Promise.resolve(readyState));
    await openArea('settings', 'privacy');
    await appApi.privacyShown;
    const newer = {
      sendViews: true,
      sendReadReceipts: true,
      sendTyping: false,
      presenceMatches: false,
      notifications: false,
      noticeContent: 'name',
    };
    await appBindings.showPrivacy(() => Promise.resolve(newer));
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
    await appBindings.showBlocked(() => Promise.resolve([oldPeer]));
    const oldUnblockButton = document.getElementById('unblock-1');
    const finishUnblockCall = smoke.pendingSmokeCall();
    const oldUnblock = appBindings.unblockPeer(oldPeer, oldUnblockButton, () =>
      finishUnblockCall.call(),
    );
    await switchToNewList();
    finishUnblockCall.resolve();
    await oldUnblock;
    checkNewList('old unblock reloaded new list');

    await appBindings.showBlocked(() => Promise.resolve([oldPeer]));
    const failUnblockCall = smoke.pendingSmokeCall();
    const oldUnblockFailure = appBindings.unblockPeer(
      oldPeer,
      document.getElementById('unblock-1'),
      () => failUnblockCall.call(),
    );
    await switchToNewList();
    failUnblockCall.reject(new Error('old unblock'));
    await oldUnblockFailure;
    checkNewList('old unblock error reloaded new list');
    if (shown('privacy-error')) throw new Error('old unblock error changed privacy error');

    await appBindings.showBlocked(() => Promise.resolve([oldPeer]));
    const oldRetryButton = document.getElementById('retry-revocation-1');
    const finishRetryCall = smoke.pendingSmokeCall();
    const oldRetry = appBindings.retryRevocation(oldPeer, oldRetryButton, () =>
      finishRetryCall.call(),
    );
    await switchToNewList();
    finishRetryCall.resolve({ revocation: 'notNeeded' });
    await oldRetry;
    checkNewList('old revocation retry changed new settings');

    await appBindings.showBlocked(() => Promise.resolve([oldPeer]));
    const failRetryCall = smoke.pendingSmokeCall();
    const oldRetryFailure = appBindings.retryRevocation(
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
    const readyState = appBindings.current;
    const oldPeer = 'a'.repeat(64);
    const newPeer = 'b'.repeat(64);
    const status = document.getElementById('blocked-revoke-status');
    const list = document.getElementById('blocked-list');
    const switchToNewList = async () => {
      await appBindings.act(Promise.resolve({ status: 'picker' }));
      await appBindings.act(Promise.resolve(readyState));
      await openArea('settings', 'blocked');
      await appApi.blockedShown;
      await appBindings.showBlocked(() => Promise.resolve([newPeer]));
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
    await appApi.blockedShown;
    const finishLoadCall = smoke.pendingSmokeCall();
    const oldLoad = appBindings.showBlocked(() => finishLoadCall.call());
    await switchToNewList();
    finishLoadCall.resolve([oldPeer]);
    await oldLoad;
    checkNewList('old blocked list changed new settings');

    const failLoadCall = smoke.pendingSmokeCall();
    const oldFailure = appBindings.showBlocked(() => failLoadCall.call());
    await switchToNewList();
    failLoadCall.reject(new Error('old list'));
    await oldFailure;
    checkNewList('old blocked list error changed new settings');
    if (shown('privacy-error')) throw new Error('old blocked list error changed privacy error');

    await checkStaleBlockedActions(oldPeer, switchToNewList, checkNewList);
  };
  smoke.uiSmokeIdentityProfile = Object.freeze({
    checkStaleProfileLoad,
    checkStaleProfileHandlers,
    checkStalePublishAndHide,
    checkStaleProfileTimers,
    checkStaleUsageStatus,
    checkStalePrivacy,
    checkStaleBlocked,
  });
}
