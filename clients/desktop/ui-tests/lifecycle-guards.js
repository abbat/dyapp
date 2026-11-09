// Identity withdrawal and link-screen smoke phases; all delayed callbacks keep their owner.
(() => {
  const createWithdrawalFixture = () => {
    const readyState = appBindings.current;
    const fixture = { removed: 0 };
    const remove = () => {
      fixture.removed += 1;
    };
    const switchAndSetDialog = async () => {
      await appBindings.act(Promise.resolve({ status: 'picker' }));
      await appBindings.act(Promise.resolve(readyState));
      appBindings.removeAsk.hidden = false;
      appBindings.removeOffline.hidden = true;
      appBindings.removeRetry.disabled = true;
      appBindings.removeAnyway.disabled = true;
    };
    return Object.assign(fixture, { remove, switchAndSetDialog });
  };
  const checkOldWithdrawalSuccess = async (fixture) => {
    const { remove, switchAndSetDialog } = fixture;
    let finishOld;
    const oldSuccess = appBindings.withdrawThenRemove(
      () =>
        new Promise((resolve) => {
          finishOld = resolve;
        }),
      remove,
    );
    await switchAndSetDialog();
    finishOld();
    await oldSuccess;
    if (fixture.removed || appBindings.removeAsk.hidden || !appBindings.removeOffline.hidden) {
      throw new Error('old withdrawal success removed new identity');
    }
  };
  const checkOldWithdrawalFailure = async (fixture) => {
    const { remove, switchAndSetDialog } = fixture;
    let failOld;
    const oldFailure = appBindings.withdrawThenRemove(
      () =>
        new Promise((_, reject) => {
          failOld = reject;
        }),
      remove,
    );
    await switchAndSetDialog();
    failOld(new Error('old withdrawal'));
    await oldFailure;
    if (
      fixture.removed ||
      appBindings.removeAsk.hidden ||
      !appBindings.removeOffline.hidden ||
      !appBindings.removeRetry.disabled ||
      !appBindings.removeAnyway.disabled
    ) {
      throw new Error('old withdrawal failure changed new dialog');
    }
  };
  const checkCurrentWithdrawal = async (fixture) => {
    const { remove } = fixture;
    await appBindings.withdrawThenRemove(() => Promise.resolve(), remove);
    if (fixture.removed !== 1) throw new Error('current withdrawal did not continue to removal');
    await appBindings.withdrawThenRemove(() => Promise.reject(new Error('offline')), remove);
    if (
      !appBindings.removeAsk.hidden ||
      appBindings.removeOffline.hidden ||
      appBindings.removeRetry.disabled ||
      appBindings.removeAnyway.disabled
    ) {
      throw new Error('current withdrawal failure did not show Offline');
    }
    appBindings.removePublished = false;
    appBindings.removeAsk.hidden = false;
    appBindings.removeOffline.hidden = true;
  };
  const createLinkFixture = () => {
    const seen = (...ids) => ids.every((id) => document.getElementById(id).checkVisibility());
    const status = () => document.getElementById('link-status').textContent;
    const fixture = { polls: [], calls: [] };
    const call = (command) => {
      fixture.calls.push(command);
      if (command === 'start_link') {
        return Promise.resolve({
          code: 'dyapp-link:1:test',
          qr: { size: 21, modules: Array(441).fill(true) },
        });
      }
      if (command === 'link_pending') return Promise.resolve(fixture.polls.shift());
      if (command !== 'poll_link') return Promise.resolve(null);
      const next = fixture.polls.shift();
      return next instanceof Error ? Promise.reject(next.message) : Promise.resolve(next);
    };
    return Object.assign(fixture, { seen, status, call });
  };
  const checkWaitingLink = async (fixture) => {
    const { seen, status, call } = fixture;
    fixture.polls = [null];
    await appBindings.startLink(call);
    if (
      !seen('link', 'link-qr', 'link-code', 'copy-link-code', 'link-cancel') ||
      seen('link-new') ||
      document.getElementById('link-code').textContent !== 'dyapp-link:1:test' ||
      status() !== appApi.strings['link.waiting']
    )
      throw new Error('link page does not show a code');
    await smoke.shot('link-show');
    clearTimeout(appBindings.linkTimer);
    fixture.polls = [new Error('notConnected')];
    await appBindings.pollLink(appBindings.linkRun, call);
    if (status() !== appApi.strings['link.offline'] || !seen('link-qr', 'link-cancel')) {
      throw new Error('link page does not say that no node answers');
    }
    clearTimeout(appBindings.linkTimer);
  };
  const checkExpiredLink = async (fixture) => {
    const { seen, status, call } = fixture;
    fixture.polls = [new Error('expired')];
    await appBindings.pollLink(appBindings.linkRun, call);
    if (
      status() !== appApi.strings['link.expired'] ||
      !seen('link-new', 'link-cancel') ||
      seen('link-qr')
    ) {
      throw new Error('an expired link keeps its code or offers no new one');
    }
    await smoke.shot('link-expired');
    fixture.polls = [null];
    await appBindings.startLink(call);
    if (seen('link-new') || !seen('link-qr'))
      throw new Error('a new code does not replace the expired one');
  };
  const checkCancelledAndReceivedLink = async (fixture) => {
    const { seen, status, calls, call } = fixture;
    const stale = appBindings.linkRun;
    await appBindings.stopLink(call);
    if (calls.at(-1) !== 'cancel_link') throw new Error('Cancel leaves the link waiting');
    fixture.polls = [null];
    await appBindings.pollLink(stale, call);
    if (calls.at(-1) !== 'poll_link' || status() !== appApi.strings['link.waiting']) {
      throw new Error('a cancelled poll changed the page');
    }
    // A record that won the race with Cancel still ends on the identity it opened.
    const state = { status: 'ready', peerId: 'ab'.repeat(32), mode: 'file' };
    fixture.polls = [state, true];
    await appBindings.pollLink(stale, call);
    if (
      !seen('link-done', 'link-peer', 'link-pending', 'link-open') ||
      seen('link-wait') ||
      !calls.includes('receive_mail') ||
      document.activeElement.id !== 'link-open' ||
      !document.getElementById('link-peer').textContent.includes(appApi.peerName(state.peerId))
    ) {
      throw new Error('a received link does not name its identity');
    }
    await smoke.shot('link-done-pending');
    fixture.polls = [state, false];
    await appBindings.pollLink(appBindings.linkRun, call);
    if (seen('link-pending') || !seen('link-done'))
      throw new Error('a finished link says it is pending');
  };
  const checkLinkNetworkStep = async (fixture) => {
    const { seen } = fixture;
    appBindings.wizardStep('link');
    appBindings.show(appBindings.wizardNetwork, 'status.ready');
    if (
      !seen('link-network-cancel', 'wizard-link-step') ||
      ['skip-network', 'wizard-peer', 'wizard-step'].some((id) => seen(id))
    ) {
      throw new Error('wrong link network step');
    }
    document.getElementById('link-network-cancel').click();
    if (
      !appBindings.identity.checkVisibility() ||
      seen('link-network-cancel') ||
      !document.getElementById('skip-network').hidden
    )
      throw new Error('link network step stayed');
    await openArea('settings', 'identities');
    if (
      !seen('device-join') ||
      document.getElementById('devices').contains(document.getElementById('device-join'))
    ) {
      throw new Error('an unpublished identity cannot link this computer');
    }
  };
  const withdrawal = async () => {
    const fixture = createWithdrawalFixture();
    appBindings.removePublished = true;
    await checkOldWithdrawalSuccess(fixture);
    await checkOldWithdrawalFailure(fixture);
    await checkCurrentWithdrawal(fixture);
  };
  const linkScreen = async () => {
    const fixture = createLinkFixture();
    await checkWaitingLink(fixture);
    await checkExpiredLink(fixture);
    await checkCancelledAndReceivedLink(fixture);
    await checkLinkNetworkStep(fixture);
  };
  smoke.uiSmokeLifecycleGuards = { withdrawal, linkScreen };
})();
