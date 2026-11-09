import { byId as domById } from './dom.js';
import { who } from './app.js';
import { identityState, identityCurrent } from './identity-state.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
import { controller as peerNamesController } from './peer-names.js';
import { controller as settingsCollectionsController } from './settings-collections.js';
import { progress } from './ui-progress.js';
export const controller = {};

// Settings → Passed people: Undo pass shows the peer in later searches and Liked you again.
let passedRead = 0;
/** @param {HTMLButtonElement} button @param {import('./ipc.js').Invoke} call */
const unpassPeer = async (peerId, button, call) => {
  const generation = identityState.generation;
  button.disabled = true;
  let failed = false;
  try {
    await call('pass_peer', { peerId, passed: false });
  } catch (_) {
    failed = true;
  }
  if (!identityCurrent(generation)) return;
  await controller.showPassed(call);
  if (failed && identityCurrent(generation))
    settingsCollectionsController.settingsCollectionFailed('passed');
};
controller.showPassed = async (call = invoke) => {
  const generation = identityState.generation;
  const read = ++passedRead;
  let peers = [];
  let failed = false;
  try {
    peers = await call('passed_peers');
  } catch (_) {
    failed = true;
  }
  if (!identityCurrent(generation) || read !== passedRead) return;
  settingsCollectionsController.renderSettingsRows(
    'passed',
    failed ? null : peers,
    (peerId, index) => {
      const row = document.createElement('tr');
      const name = document.createElement('td');
      name.className = 'who';
      name.append(...who(row, peerId, null, ''));
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'btn';
      button.id = `unpass-${index + 1}`;
      button.textContent = strings['settings.unpass'];
      button.addEventListener('click', () => {
        progress.passedShown = unpassPeer(peerId, button, call);
      });
      const action = document.createElement('td');
      action.append(button);
      row.append(name, action);
      return row;
    },
    (peer) => `${peer} ${peerNamesController.peerName(peer)}`,
    () => controller.showPassed(),
  );
  domById('passed-none', HTMLParagraphElement).hidden = peers.length > 0;
};
