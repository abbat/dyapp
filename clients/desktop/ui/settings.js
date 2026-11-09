import { byId as domById } from './dom.js';
import { who, modeText, setTray, controller as appController } from './app.js';
import { showDevices } from './devices.js';
import { settingsRemove } from './elements.js';
import { identityState, identityCurrent } from './identity-state.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
import { controller as passesController } from './passes.js';
import { knowPeers } from './peer-labels.js';
import { controller as peerNamesController } from './peer-names.js';
import { controller as privacyUiController } from './privacy-ui.js';
import { controller as settingsCollectionsController } from './settings-collections.js';
import { progress } from './ui-progress.js';
export const controller = {};

// Settings controllers use explicitly imported navigation and identity helpers.
let identitiesRead = 0;
let blockedRead = 0;
// Settings → Blocked people: Unblock shows the peer in later searches again.
/** @param {HTMLButtonElement} button */
const unblockPeer = async (peerId, button, call = invoke) => {
  const generation = identityState.generation;
  button.disabled = true;
  let failed = false;
  try {
    await call('unblock_peer', { peerId });
  } catch (_) {
    failed = true;
  }
  if (identityCurrent(generation)) {
    await showBlocked();
    if (failed && identityCurrent(generation))
      settingsCollectionsController.settingsCollectionFailed('blocked');
  }
};
/** @param {HTMLButtonElement} retry */
const retryRevocation = async (peerId, retry, call = invoke) => {
  const generation = identityState.generation;
  retry.disabled = true;
  const status = domById('blocked-revoke-status', HTMLParagraphElement);
  try {
    const result = await call('block_peer', { peerId });
    if (identityCurrent(generation)) {
      status.textContent = strings[`settings.blocked.${result.revocation}`];
    }
  } catch (error) {
    if (identityCurrent(generation)) {
      status.textContent = `${strings['settings.blocked.failed']} ${String(error)}`;
    }
  } finally {
    if (identityCurrent(generation)) retry.disabled = false;
  }
};
const showBlocked = async (call = invoke) => {
  const generation = identityState.generation;
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
  domById('blocked-revoke-status', HTMLParagraphElement).textContent = '';
  settingsCollectionsController.renderSettingsRows(
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
      button.textContent = strings['settings.unblock'];
      button.addEventListener('click', () => {
        controller.blockedShown = unblockPeer(peerId, button);
      });
      const retry = document.createElement('button');
      retry.type = 'button';
      retry.className = 'btn';
      retry.id = `retry-revocation-${index + 1}`;
      retry.textContent = strings['settings.blocked.retry'];
      retry.addEventListener('click', () => {
        controller.revocationRetried = retryRevocation(peerId, retry);
      });
      const action = document.createElement('td');
      const buttons = document.createElement('div');
      buttons.className = 'actions';
      buttons.append(retry, button);
      action.append(buttons);
      row.append(name, action);
      return row;
    },
    (peer) => `${peer} ${peerNamesController.peerName(peer)}`,
    () => showBlocked(),
  );
  domById('blocked-none', HTMLParagraphElement).hidden = peers.length > 0;
};

const showSettingsIdentities = async (call = invoke) => {
  const generation = identityState.generation;
  const read = ++identitiesRead;
  const list = domById('settings-identities', HTMLTableSectionElement);
  const error = domById('settings-error', HTMLParagraphElement);
  // Remove waits under the error until a row for the open identity takes it.
  settingsRemove.hidden = true;
  error.after(settingsRemove);
  list.replaceChildren();
  error.hidden = true;
  try {
    const items = await call('list_identities');
    if (!identityCurrent(generation) || read !== identitiesRead) return;
    settingsCollectionsController.renderSettingsRows(
      'identities',
      knowPeers(items),
      appController.settingsIdentity,
      (item) =>
        [
          item.peerId,
          item.peerId && peerNamesController.peerName(item.peerId),
          item.slot,
          modeText(item),
        ]
          .filter(Boolean)
          .join(' '),
      () => showSettingsIdentities(),
    );
    error.hidden = true;
  } catch (_) {
    if (!identityCurrent(generation) || read !== identitiesRead) return;
    settingsCollectionsController.renderSettingsRows(
      'identities',
      null,
      appController.settingsIdentity,
      () => '',
      () => showSettingsIdentities(),
    );
    error.textContent = strings['storage.error'];
    error.hidden = false;
  }
};
controller.showSettings = () => {
  controller.nodeShown = appController.showNode();
  controller.usageShown = appController.showUsage();
  progress.trayShown = setTray(null);
  controller.privacyShown = privacyUiController.showPrivacy();
  controller.blockedShown = showBlocked();
  progress.passedShown = passesController.showPassed();
  progress.devicesShown = showDevices();
  return showSettingsIdentities();
};

export { retryRevocation, showBlocked, showSettingsIdentities, unblockPeer };
