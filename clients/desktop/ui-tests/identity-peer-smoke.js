// Scoped fixture/assertion helpers and shared smoke scenarios.
{
  // A result from an old identity must not paint or unlock controls for a newer one.
  const assertStaleSearchAndChatsOldSearchChangedNewSearchState = () => {
    if (
      !searchBusy ||
      document.getElementById('search-status').textContent !== window.strings['search.loading']
    )
      throw new Error('old search changed new search state');
  };
  const assertStaleSearchAndChatsCurrentSearchDidNotFinish = () => {
    if (
      searchBusy ||
      document.getElementById('search-status').textContent !== window.strings['search.empty']
    )
      throw new Error('current search did not finish');
  };
  const assertStaleSearchAndChatsOldMailboxChangedNewChats = (oldLikedCalls) => {
    if (
      oldLikedCalls ||
      document.getElementById('chats-status').textContent !== window.strings['chats.loading']
    )
      throw new Error('old mailbox changed new Chats');
  };
  const checkStaleMailbox = async (readyState) => {
    await openArea('chats-area');
    await window.chatsDone;
    const finishMailCall = pendingSmokeCall();
    let oldLikedCalls = 0;
    const oldChats = window.showChats((command) => {
      if (command === 'liked_you') {
        oldLikedCalls += 1;
        return Promise.resolve([]);
      }
      return finishMailCall.call();
    });
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    const finishNewMailCall = pendingSmokeCall();
    const newChats = window.showChats((command) =>
      ['liked_you', 'chat_rows'].includes(command) ? Promise.resolve([]) : finishNewMailCall.call(),
    );
    finishMailCall.resolve(1);
    await oldChats;
    assertStaleSearchAndChatsOldMailboxChangedNewChats(oldLikedCalls);
    finishNewMailCall.resolve(0);
    await newChats;
    if (document.getElementById('chats-status').textContent !== window.strings['chats.none']) {
      throw new Error('current Chats did not finish');
    }
  };
  const checkStaleChatRefresh = async (readyState) => {
    await openArea('chats-area');
    await window.chatsDone;
    const finishRowsCall = pendingSmokeCall();
    const oldRefresh = window.mailUpdated(null, () => finishRowsCall.call());
    await act(Promise.resolve({ status: 'picker' }));
    const shownRows = chatList.childElementCount;
    finishRowsCall.resolve([
      { peerId: '6'.repeat(64), state: 'open', lastMessage: null, unreadCount: 0 },
    ]);
    await oldRefresh;
    if (chatList.childElementCount !== shownRows)
      throw new Error('old refresh rendered Chats rows');
    await act(Promise.resolve(readyState));
  };
  const checkStaleSearchAndChats = async () => {
    const readyState = current;
    await openArea('search-area');
    const finishOldCall = pendingSmokeCall();
    const noNames = (run) => (command) =>
      command === 'contact_names' ? Promise.resolve([]) : run();
    const oldSearch = runSearch(noNames(() => finishOldCall.call()));
    if (!searchBusy) throw new Error('old search did not start');
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    await openArea('search-area');
    const finishCurrentCall = pendingSmokeCall();
    const newSearch = runSearch(noNames(() => finishCurrentCall.call()));
    finishOldCall.resolve({ profiles: [], offered: [], partial: false, failures: [] });
    await oldSearch;
    assertStaleSearchAndChatsOldSearchChangedNewSearchState();
    finishCurrentCall.resolve({ profiles: [], offered: [], partial: false, failures: [] });
    await newSearch;
    assertStaleSearchAndChatsCurrentSearchDidNotFinish();

    await checkStaleMailbox(readyState);
    await checkStaleChatRefresh(readyState);
  };
  // Even reopening the same card object for a new identity cannot admit an old peer response.
  const assertStalePeerActionsOldLikeChangedNewControls = () => {
    if (
      !document.getElementById('view-like').disabled ||
      viewStatus.textContent !== window.strings['view.sending']
    ) {
      throw new Error('old like changed new controls');
    }
  };
  const assertStalePeerActionsCurrentLikeDidNotFinish = () => {
    if (
      document.getElementById('view-like').disabled ||
      !document.getElementById('view-like').hidden
    ) {
      throw new Error('current like did not finish');
    }
  };
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
    const finishLikesCall = pendingSmokeCall();
    openFound(found, card, () => finishLikesCall.call());
    await switchBack();
    finishLikesCall.resolve({ ours: true, matched: true });
    await Promise.resolve();
    if (document.getElementById('view-like').hidden)
      throw new Error('old peer likes changed new card');

    const finishOldLikeCall = pendingSmokeCall();
    const oldLike = sendLike(true, () => finishOldLikeCall.call());
    await switchBack();
    const finishNewLikeCall = pendingSmokeCall();
    const newLike = sendLike(true, () => finishNewLikeCall.call());
    finishOldLikeCall.resolve({ ours: true, matched: true });
    await oldLike;
    assertStalePeerActionsOldLikeChangedNewControls();
    finishNewLikeCall.resolve({ ours: true, matched: false });
    await newLike;
    assertStalePeerActionsCurrentLikeDidNotFinish();

    const finishBlockCall = pendingSmokeCall();
    const oldBlock = blockViewed(() => finishBlockCall.call());
    await switchBack();
    finishBlockCall.resolve({ revocation: 'notNeeded' });
    await oldBlock;
    if (!card.isConnected || profileView.hidden) throw new Error('old block removed new card');
    await blockViewed(() => Promise.resolve({ revocation: 'notNeeded' }));
    if (card.isConnected || !profileView.hidden) throw new Error('current block did not finish');
  };
  // Late safety responses must not open or unlock the next identity's shared panel.
  const assertStaleSafetyOldSafetyResultChangedNewPanel = () => {
    if (
      !document.getElementById('view-safety').disabled ||
      !document.getElementById('view-safety-panel').hidden
    ) {
      throw new Error('old safety result changed new panel');
    }
  };
  const assertStaleSafetyCurrentSafetyResultDidNotFinish = () => {
    if (
      document.getElementById('view-safety').disabled ||
      document.getElementById('view-safety-panel').hidden
    ) {
      throw new Error('current safety result did not finish');
    }
  };
  const assertStaleSafetyActiveCameraStreamSurvivedIdentityTransition = (activeStopped) => {
    if (
      !activeStopped ||
      cameraStream !== null ||
      cameraPending ||
      document.getElementById('view-safety-camera').textContent !==
        window.strings['view.safety.camera.start']
    ) {
      throw new Error('active camera stream survived identity transition');
    }
  };
  const assertStaleSafetyAnUnknownConfirmationErrorWasNotShown = (mark, confirmStatus) => {
    if (
      !mark.hidden ||
      confirmStatus.textContent !== window.strings['view.safety.confirm.failed']
    ) {
      throw new Error('an unknown confirmation error was not shown');
    }
  };
  const assertStaleSafetyTheFreshProfileDidNotShowThe = (mark, found) => {
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
  };
  const assertStaleSafetyAFailedRemovalChangedTheMark = (mark, confirmStatus) => {
    if (
      mark.hidden ||
      confirmStatus.textContent !== window.strings['view.safety.confirm.removeFailed']
    ) {
      throw new Error('a failed removal changed the mark');
    }
  };
  const assertStaleSafetyTheMarkWasNotRemoved = (mark) => {
    if (
      !mark.hidden ||
      document.getElementById('view-confirm').hidden ||
      document.activeElement.id !== 'view-confirm'
    ) {
      throw new Error('the mark was not removed');
    }
  };
  const checkStaleCameraCapture = async (readyState, reopen) => {
    const mediaDescriptor = Object.getOwnPropertyDescriptor(navigator, 'mediaDevices');
    const finishCaptureCall = pendingSmokeCall();
    let stopped = false;
    Object.defineProperty(navigator, 'mediaDevices', {
      configurable: true,
      value: {
        getUserMedia: () => finishCaptureCall.call(),
      },
    });
    try {
      document.getElementById('view-safety-camera').click();
      await act(Promise.resolve({ status: 'picker' }));
      finishCaptureCall.resolve({
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
  };
  const checkStaleActiveCamera = async (readyState, reopen) => {
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
    assertStaleSafetyActiveCameraStreamSurvivedIdentityTransition(activeStopped);
    await leaving;
    await act(Promise.resolve(readyState));
    await reopen();
  };
  const checkStaleSafetyCopy = async (readyState, reopen) => {
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
  const checkSafetyConfirmationErrors = async (mark, confirmStatus) => {
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
    assertStaleSafetyAnUnknownConfirmationErrorWasNotShown(mark, confirmStatus);
  };
  const checkStaleSafetyVerification = async (
    readyState,
    reopen,
    found,
    safetyAnswers,
    mark,
    confirmStatus,
  ) => {
    const finishOldConfirmCall = pendingSmokeCall();
    const oldConfirm = confirmContact(() => finishOldConfirmCall.call());
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    await reopen();
    finishOldConfirmCall.resolve({ ...found, version: 9 });
    await oldConfirm;
    if (!mark.hidden || found.version === 9 || document.getElementById('view-confirm').disabled) {
      throw new Error('old confirmation changed the new view');
    }
    await loadSafety(safetyAnswers);
    await confirmContact(() => Promise.resolve({ ...found, version: 2 }));
    assertStaleSafetyTheFreshProfileDidNotShowThe(mark, found);
    await shot('search-safety-verified');
    await removeMark(() => Promise.reject(new Error('locked')));
    assertStaleSafetyAFailedRemovalChangedTheMark(mark, confirmStatus);
    await removeMark(() => Promise.resolve());
    assertStaleSafetyTheMarkWasNotRemoved(mark);
    await reopen();
    await checkStaleSafetyCopy(readyState, reopen);
  };
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
    const finishOldNumberCall = pendingSmokeCall();
    const oldSafety = loadSafety((command) =>
      command === 'contact_safety_qr' ? Promise.resolve(matrix) : finishOldNumberCall.call(),
    );
    await act(Promise.resolve({ status: 'picker' }));
    await act(Promise.resolve(readyState));
    await reopen();
    const finishNewNumberCall = pendingSmokeCall();
    const newSafety = loadSafety((command) =>
      command === 'contact_safety_qr' ? Promise.resolve(matrix) : finishNewNumberCall.call(),
    );
    finishOldNumberCall.resolve(number);
    await oldSafety;
    assertStaleSafetyOldSafetyResultChangedNewPanel();
    finishNewNumberCall.resolve(number);
    await newSafety;
    assertStaleSafetyCurrentSafetyResultDidNotFinish();

    await checkStaleCameraCapture(readyState, reopen);
    await checkStaleActiveCamera(readyState, reopen);
    // Failures and answers for an earlier identity leave the mark off; only a current answer sets it.
    const safetyAnswers = (command) =>
      Promise.resolve(command === 'contact_safety_qr' ? matrix : number);
    const mark = document.getElementById('view-verified');
    const confirmStatus = document.getElementById('view-safety-scan-status');
    await loadSafety(safetyAnswers);
    await checkSafetyConfirmationErrors(mark, confirmStatus);
    await checkStaleSafetyVerification(
      readyState,
      reopen,
      found,
      safetyAnswers,
      mark,
      confirmStatus,
    );
  };
  globalThis.uiSmokeIdentityPeer = Object.freeze({
    checkStaleSearchAndChats,
    checkStalePeerActions,
    checkStaleSafety,
  });
}
