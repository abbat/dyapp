// Privacy persistence and device-disconnect smoke phases, with the original callback order.
(() => {
  const checkPrivacyDefaults = async (restarted) => {
    await openArea('settings', 'privacy');
    await appApi.privacyShown;
    await appApi.blockedShown;
    const expected = restarted ? 'true,true,false,false,false' : 'false,false,true,true,true';
    if (
      privacyState() !== expected ||
      shown('privacy-error') ||
      document.getElementById('presence-nobody').checked !== restarted
    ) {
      throw new Error(`privacy settings ${privacyState()}, expected ${expected}`);
    }
    if (!shown('blocked-none') || document.getElementById('blocked-list').children.length) {
      throw new Error('blocked list not empty');
    }
    const noticeChoice = document.getElementById('notice-content');
    if (
      noticeChoice.value !== (restarted ? 'text' : 'fixed') ||
      noticeChoice.disabled !== restarted
    ) {
      throw new Error(`notice content ${noticeChoice.value}, disabled ${noticeChoice.disabled}`);
    }
    return noticeChoice;
  };
  const changePrivacyChoices = async (noticeChoice) => {
    openSettings('notifications');
    noticeChoice.value = 'text';
    noticeChoice.dispatchEvent(new Event('change', { bubbles: true }));
    await appApi.privacySaved;
    for (const id of [
      'send-views',
      'send-read-receipts',
      'send-typing',
      'presence-nobody',
      'message-notifications',
    ]) {
      openSettings(id === 'message-notifications' ? 'notifications' : 'privacy');
      document.getElementById(id).click();
      await appApi.privacySaved;
    }
    if (shown('privacy-error')) throw new Error('privacy settings not saved');
  };
  const checkThemes = () => {
    const root = document.documentElement;
    openSettings('appearance');
    document.getElementById('theme-dark').click();
    if (root.dataset.theme !== 'dark') throw new Error('dark theme not applied');
    document.getElementById('theme-system').click();
    if ('theme' in root.dataset) throw new Error('system theme not restored');
  };
  const checkBlockedRecovery = async () => {
    const blockedPeer = 'f'.repeat(64);
    const blocked = await invokeTest('block_peer', { peerId: blockedPeer });
    if (blocked.revocation !== 'notNeeded')
      throw new Error(`unexpected block state: ${blocked.revocation}`);
    await openArea('profile-area');
    await openArea('settings', 'blocked');
    await appApi.blockedShown;
    const retry = document.getElementById('retry-revocation-1');
    if (
      !retry ||
      !document.getElementById('unblock-1') ||
      document.getElementById('blocked-revoke-status').textContent
    ) {
      throw new Error('blocked peer has no retry and unblock actions');
    }
    retry.click();
    await appApi.revocationRetried;
    if (
      document.getElementById('blocked-revoke-status').textContent !==
      appApi.strings['settings.blocked.notNeeded']
    ) {
      throw new Error('retry did not report token revocation state');
    }
    document.getElementById('unblock-1').click();
    await appApi.blockedShown;
    if (!shown('blocked-none') || document.getElementById('blocked-list').children.length) {
      throw new Error('unblock did not clear the blocked list');
    }
  };
  const createDeviceFixture = () => {
    const card = document.getElementById('devices');
    const dialog = document.getElementById('disconnect');
    const rows = [
      { key: [1, 2, 255], this: false },
      { key: [10, 11, 12], this: true },
    ];
    const stale = appBindings.showDevices(
      () =>
        new Promise((resolve) => {
          setTimeout(() => resolve([rows[0]]));
        }),
    );
    const list = (command) => {
      if (command !== 'list_devices') throw new Error(`unexpected ${command}`);
      return Promise.resolve(structuredClone(rows));
    };
    return { card, dialog, stale, list };
  };
  const checkDeviceRows = async (fixture) => {
    const { card, stale, list } = fixture;
    await appBindings.showDevices(() => Promise.resolve([]));
    if (!card.hidden) throw new Error('an unpublished identity shows Devices');
    await openArea('settings', 'devices');
    await appBindings.showDevices(list);
    await stale;
    const shownKeys = [...document.querySelectorAll('#devices-list code')].map(
      (key) => key.textContent,
    );
    if (
      card.hidden ||
      shownKeys.join() !== '0a0b0c,0102ff' ||
      document.querySelectorAll('#devices-list button').length !== 1
    ) {
      throw new Error(`wrong device rows: ${shownKeys}`);
    }
  };
  const checkDisconnectDialog = (fixture) => {
    const { dialog } = fixture;
    document.getElementById('device-disconnect-1').click();
    if (
      !dialog.open ||
      document.activeElement.id !== 'disconnect-cancel' ||
      document.getElementById('disconnect-device').textContent !== '0102ff'
    ) {
      throw new Error('disconnect dialog did not open on the row');
    }
  };
  const checkFailedDisconnect = async () => {
    await appBindings.disconnectNow(() => Promise.reject(new Error('notConnected')));
    if (
      document.getElementById('disconnect-problem').hidden ||
      document.getElementById('disconnect-retry').hidden ||
      !document.getElementById('disconnect-action').hidden
    ) {
      throw new Error('failed disconnect does not offer Retry');
    }
  };
  const checkSuccessfulDisconnect = async (fixture) => {
    const { dialog } = fixture;
    const sent = [];
    await appBindings.disconnectNow((command, args) => {
      sent.push([command, args.deviceKey.join()]);
      return Promise.resolve();
    });
    if (dialog.open || sent.join() !== 'revoke_device,1,2,255') {
      throw new Error(`disconnect sent ${sent}`);
    }
    await closed(dialog);
    if (document.getElementById('devices-status').textContent !== appApi.strings['devices.gone']) {
      throw new Error('disconnect did not report its result');
    }
  };
  const privacy = async (restarted) => {
    const noticeChoice = await checkPrivacyDefaults(restarted);
    if (restarted) return;
    await changePrivacyChoices(noticeChoice);
    checkThemes();
    await checkBlockedRecovery();
  };
  const devices = async () => {
    const fixture = createDeviceFixture();
    await checkDeviceRows(fixture);
    checkDisconnectDialog(fixture);
    await checkFailedDisconnect();
    await checkSuccessfulDisconnect(fixture);
  };
  smoke.uiSmokeSettingsGuards = { privacy, devices };
})();
