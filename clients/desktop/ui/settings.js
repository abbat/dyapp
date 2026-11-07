// Settings controllers share app.js helpers; loaded after it, before UI interaction.
let identitiesRead = 0;
let blockedRead = 0;
// Settings → Blocked people: Unblock shows the peer in later searches again.
const unblockPeer = async (peerId, button, call = invoke) => {
  const generation = identityGeneration;
  button.disabled = true;
  let failed = false;
  try {
    await call('unblock_peer', { peerId });
  } catch (_) {
    failed = true;
  }
  if (identityCurrent(generation)) {
    await showBlocked();
    if (failed && identityCurrent(generation)) window.settingsCollectionFailed('blocked');
  }
};
const retryRevocation = async (peerId, retry, call = invoke) => {
  const generation = identityGeneration;
  retry.disabled = true;
  const status = document.getElementById('blocked-revoke-status');
  try {
    const result = await call('block_peer', { peerId });
    if (identityCurrent(generation)) {
      status.textContent = window.strings[`settings.blocked.${result.revocation}`];
    }
  } catch (error) {
    if (identityCurrent(generation)) {
      status.textContent = `${window.strings['settings.blocked.failed']} ${String(error)}`;
    }
  } finally {
    if (identityCurrent(generation)) retry.disabled = false;
  }
};
const showBlocked = async (call = invoke) => {
  const generation = identityGeneration;
  const read = ++blockedRead;
  let peers = [];
  let failed = false;
  try {
    peers = await call('blocked_peers');
    if (!identityCurrent(generation) || read !== blockedRead) return;
  } catch (_) {
    if (!identityCurrent(generation) || read !== blockedRead) return;
    failed = true;
  }
  document.getElementById('blocked-revoke-status').textContent = '';
  window.renderSettingsRows(
    'blocked',
    failed ? null : peers,
    (peerId, index) => {
      const row = document.createElement('tr');
      const name = document.createElement('td');
      name.className = 'who';
      name.append(...who(row, peerId, null, ''));
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'btn';
      button.id = `unblock-${index + 1}`;
      button.textContent = window.strings['settings.unblock'];
      button.addEventListener('click', () => {
        window.blockedShown = unblockPeer(peerId, button);
      });
      const retry = document.createElement('button');
      retry.type = 'button';
      retry.className = 'btn';
      retry.id = `retry-revocation-${index + 1}`;
      retry.textContent = window.strings['settings.blocked.retry'];
      retry.addEventListener('click', () => {
        window.revocationRetried = retryRevocation(peerId, retry);
      });
      const action = document.createElement('td');
      const buttons = document.createElement('div');
      buttons.className = 'actions';
      buttons.append(retry, button);
      action.append(buttons);
      row.append(name, action);
      return row;
    },
    (peer) => `${peer} ${window.peerName(peer)}`,
    () => showBlocked(),
  );
  document.getElementById('blocked-none').hidden = peers.length > 0;
};

const showSettingsIdentities = async (call = invoke) => {
  const generation = identityGeneration;
  const read = ++identitiesRead;
  const list = document.getElementById('settings-identities');
  const error = document.getElementById('settings-error');
  // Remove waits under the error until a row for the open identity takes it.
  settingsRemove.hidden = true;
  error.after(settingsRemove);
  list.replaceChildren();
  error.hidden = true;
  try {
    const items = await call('list_identities');
    if (!identityCurrent(generation) || read !== identitiesRead) return;
    window.renderSettingsRows(
      'identities',
      knowPeers(items),
      window.settingsIdentity,
      (item) =>
        [item.peerId, item.peerId && window.peerName(item.peerId), item.slot, modeText(item)]
          .filter(Boolean)
          .join(' '),
      () => showSettingsIdentities(),
    );
    error.hidden = true;
  } catch (_) {
    if (!identityCurrent(generation) || read !== identitiesRead) return;
    window.renderSettingsRows(
      'identities',
      null,
      window.settingsIdentity,
      () => '',
      () => showSettingsIdentities(),
    );
    error.textContent = window.strings['storage.error'];
    error.hidden = false;
  }
};
window.showSettings = () => {
  window.nodeShown = window.showNode();
  window.usageShown = window.showUsage();
  window.trayShown = setTray(null);
  window.privacyShown = showPrivacy();
  window.blockedShown = showBlocked();
  window.passedShown = window.showPassed();
  window.devicesShown = showDevices();
  return showSettingsIdentities();
};
