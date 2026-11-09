import { byId as domById } from './dom.js';
import { showArea } from './app.js';
import { chatState } from './chat-state.js';
import { closeForward, openChat } from './chats-ui.js';
import {
  forwardSelect,
  forwardConfirm,
  forwardError,
  forwardDialog,
  forwardCancel,
  chatComposeStatus,
} from './elements.js';
import { identityState, identityCurrent } from './identity-state.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
import { shortPeer } from './peer-labels.js';
import { loadPeerNames, controller as peerNamesController } from './peer-names.js';
import { viewed, withPlaces, openFound } from './search.js';
export const controller = {};
/** @type {Promise<void> | undefined} */
controller.shareOpened = undefined;
/** @type {Promise<void> | undefined} */
controller.sharedOpened = undefined;

// Profile links in chats reuse the chat controller's Forward dialog through imports.
// Forward and Share use one dialog; its title and button name the action.
/** @param {string} action @param {import('./chat-state.js').ChatRow[]} targets */
const showForwardDialog = (action, targets) => {
  for (const [id, key] of [
    ['chat-forward-title', `chats.${action}Title`],
    ['chat-forward-confirm', `chats.${action}`],
  ]) {
    const element = domById(id, HTMLElement);
    element.dataset.i18n = key;
    element.textContent = strings[key];
  }
  forwardSelect.replaceChildren(
    ...targets.map((row) => {
      const option = document.createElement('option');
      option.value = row.peerId;
      option.textContent = peerNamesController.peerName(row.peerId);
      return option;
    }),
  );
  forwardConfirm.disabled = !targets.length;
  forwardError.hidden = !!targets.length;
  forwardError.textContent = targets.length ? '' : strings['chats.forwardNoTargets'];
  forwardDialog.hidden = false;
  forwardDialog.showModal();
  if (targets.length) forwardSelect.focus();
  else forwardCancel.focus();
};
// A profile is shared into an open chat, never a request, as an ordinary text.
/** @param {string} peerId */
const openShare = async (peerId, call = invoke) => {
  const generation = identityState.generation;
  const rows = await call('chat_rows').catch(() => []);
  if (!identityCurrent(generation) || forwardDialog.open) return;
  await loadPeerNames(
    rows.map((row) => row.peerId),
    call,
  ).catch(() => {});
  if (!identityCurrent(generation) || forwardDialog.open) return;
  chatState.forwardTarget = { share: peerId, generation };
  showForwardDialog(
    'share',
    rows.filter((row) => row.state === 'open' && row.peerId !== peerId),
  );
};
domById('view-share', HTMLButtonElement).addEventListener('click', () => {
  const found = viewed?.found;
  if (found) controller.shareOpened = openShare(found.peerId);
});
const profileCardText = (peerId) => `${strings['chats.profileCard']} ${shortPeer(peerId)}`;
// Nothing about the shared peer is fetched until the card is pressed, and then only verified.
/** @param {HTMLButtonElement} card */
const openShared = async (peerId, card, call = invoke) => {
  const generation = identityState.generation;
  try {
    const [found] = await withPlaces([await call('shared_profile', { peerId })]);
    if (identityCurrent(generation) && card.isConnected) openFound(found, card, call);
  } catch (error) {
    if (identityCurrent(generation))
      chatComposeStatus.textContent = `${strings['chats.profileFailed']} ${String(error)}`;
  }
};
controller.profileCard = (peerId, call) => {
  const card = document.createElement('button');
  card.type = 'button';
  card.className = 'btn';
  card.dataset.peerId = peerId;
  card.textContent = profileCardText(peerId);
  card.addEventListener('click', () => {
    controller.sharedOpened = openShared(peerId, card, call);
  });
  return card;
};
controller.shareProfile = async (call = invoke) => {
  const target = chatState.forwardTarget;
  const targetPeerId = forwardSelect.value;
  if (!target || chatState.forwardBusy || forwardConfirm.disabled || !targetPeerId) return;
  chatState.forwardBusy = true;
  forwardCancel.disabled = true;
  forwardConfirm.disabled = true;
  forwardSelect.disabled = true;
  try {
    await call('share_profile', { peerId: target.share, targetPeerId });
  } catch (error) {
    if (chatState.forwardTarget !== target || !identityCurrent(target.generation)) return;
    chatState.forwardBusy = false;
    forwardCancel.disabled = false;
    forwardConfirm.disabled = false;
    forwardSelect.disabled = false;
    forwardError.textContent = `${strings['chats.shareFailed']} ${String(error)}`;
    forwardError.hidden = false;
    return;
  }
  if (chatState.forwardTarget !== target || !identityCurrent(target.generation)) return;
  closeForward();
  showArea('chats-area');
  await openChat(targetPeerId, call);
  if (identityCurrent(target.generation) && chatState.activeChat?.peerId === targetPeerId) {
    chatComposeStatus.textContent = strings['chats.shareSent'];
  }
};

export { openShare, profileCardText, showForwardDialog };
