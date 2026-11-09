import { all as domAll, byId as domById, query as domQuery } from './dom.js';
import { who, modeText, reasonText, showError, show, identity, sections, act } from './app.js';
import { removeDialog, removeIdentity, settingsRemove } from './elements.js';
import { identityState, identityCurrent } from './identity-state.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
export const controller = {};
/** @type {Promise<{ status: string; } | null> | Promise<import('./identity-state.js').ReadyIdentity | import('./identity-state.js').RecoveryIdentity | import('./identity-state.js').SetupIdentity | { status: string; }> | undefined} */
controller.removeDone = undefined;
export const removalState = { published: false };

// Removal actions use explicit identity state, dialog elements and navigation imports.
const removeConfirm = domById('remove-confirm', HTMLInputElement);
const removeAction = domById('remove-action', HTMLButtonElement);
// Second confirmation: Remove stays disabled until the checkbox is ticked.
const removeAsk = domById('remove-ask', HTMLDivElement);
const removeOffline = domById('remove-offline', HTMLDivElement);
const removeRetry = domById('remove-retry', HTMLButtonElement);
const removeAnyway = domById('remove-anyway', HTMLButtonElement);

const showRemove = async (call = invoke) => {
  const generation = identityState.generation;
  const state = identityState.current;
  const items = await call('list_identities');
  if (!identityCurrent(generation)) return;
  const copies =
    state.peerId &&
    items.some(
      (item) => item.slot !== state.slot && item.status === 'ready' && item.peerId === state.peerId,
    );
  const whoRow = domById('remove-who', HTMLParagraphElement);
  whoRow.replaceChildren(...who(whoRow, state.peerId, state.reason, modeText(state)));
  const peer = domById('remove-peer', HTMLElement);
  peer.textContent = state.peerId || reasonText(state.reason);
  peer.parentElement.hidden = !state.peerId;
  const path = domById('remove-path', HTMLParagraphElement);
  path.hidden = !state.keyPath;
  domQuery(path, 'code', HTMLElement).textContent = state.keyPath || '';
  domById('remove-copies', HTMLParagraphElement).textContent = !state.peerId
    ? ''
    : strings[copies ? 'remove.otherCopy' : 'remove.lastCopy'];
  // A missing key or another copy means nothing irreplaceable is lost.
  domById('remove-warning', HTMLParagraphElement).hidden = !state.peerId || copies;
  // A published profile is withdrawn from search before the key goes.
  const view = state.status === 'ready' && (await call('load_profile').catch(() => null));
  if (!identityCurrent(generation)) return;
  removalState.published = Boolean(view?.published);
  domById('remove-text', HTMLParagraphElement).hidden = !removalState.published;
  removeAsk.hidden = false;
  removeOffline.hidden = true;
  removeConfirm.checked = false;
  removeAction.disabled = true;
  removeDialog.hidden = false;
  removeDialog.showModal();
  domById('remove-cancel', HTMLButtonElement).focus();
};
// A queued close from the previous opening must not hide a newly reopened modal.
removeDialog.addEventListener('close', () => {
  if (!removeDialog.open) removeDialog.hidden = true;
});
for (const button of [removeIdentity, settingsRemove]) {
  button.addEventListener('click', () => {
    const generation = identityState.generation;
    showRemove().catch(() => {
      if (identityCurrent(generation)) showError();
    });
  });
}
for (const id of ['setup-cancel', 'picker-cancel']) {
  domById(id, HTMLElement).addEventListener('click', () =>
    identityState.current.status === 'ready'
      ? show(identity, 'status.ready')
      : show(sections[3], 'status.recovery'),
  );
}
removeConfirm.addEventListener('change', () => {
  removeAction.disabled = !removeConfirm.checked;
});
const removeNow = () => {
  const removed = identityState.current;
  // Nothing is open after a removal until the backend reports the next state.
  identityState.current = null;
  return act(invoke('remove_identity', { slot: removed.slot, peerId: removed.peerId || null }));
};
const revokedDialog = domById('revoked', HTMLDialogElement);
revokedDialog.addEventListener('close', () => {
  revokedDialog.hidden = true;
});
domById('revoked-ok', HTMLButtonElement).addEventListener('click', () => revokedDialog.close());
// The runtime found the device of `slot` revoked. Nothing is asked: the owner already decided on
// another device, and the profile stays theirs, so there is no tombstone either. A failed removal
// shows the error screen; a pass tries again once the identity opens.
controller.deviceRevoked = async (slot, remove = removeNow) => {
  if (identityState.current?.slot !== slot) return;
  for (const dialog of domAll(document, 'dialog[open]', HTMLDialogElement)) dialog.close();
  const state = await remove();
  if (state?.status === 'error') return;
  revokedDialog.hidden = false;
  revokedDialog.showModal();
  domById('revoked-ok', HTMLButtonElement).focus();
};
// The tombstone goes first; when no node takes it, the person chooses to wait or remove anyway.
const withdrawThenRemove = async (call = invoke, remove = removeNow) => {
  const generation = identityState.generation;
  if (removalState.published) {
    try {
      await call('withdraw_profile');
    } catch (_) {
      if (!identityCurrent(generation)) return null;
      removeAsk.hidden = true;
      removeOffline.hidden = false;
      removeRetry.disabled = false;
      removeAnyway.disabled = false;
      domById('remove-cancel-offline', HTMLButtonElement).focus();
      return null;
    }
  }
  if (!identityCurrent(generation)) return null;
  return remove();
};
removeAction.addEventListener('click', () => {
  removeAction.disabled = true;
  controller.removeDone = withdrawThenRemove();
});
removeRetry.addEventListener('click', () => {
  removeRetry.disabled = true;
  removeAnyway.disabled = true;
  controller.removeDone = withdrawThenRemove();
});
removeAnyway.addEventListener('click', () => {
  removeAnyway.disabled = true;
  controller.removeDone = removeNow();
});
for (const id of ['remove-cancel', 'remove-cancel-offline']) {
  domById(id, HTMLElement).addEventListener('click', () => removeDialog.close());
}

export {
  removeAnyway,
  removeAsk,
  removeOffline,
  removeRetry,
  revokedDialog,
  showRemove,
  withdrawThenRemove,
};
