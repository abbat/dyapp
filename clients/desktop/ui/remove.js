// Removal dialog actions share the classic-script bindings established by app.js.
const removeConfirm = document.getElementById('remove-confirm');
const removeAction = document.getElementById('remove-action');
// Second confirmation: Remove stays disabled until the checkbox is ticked.
const removeAsk = document.getElementById('remove-ask');
const removeOffline = document.getElementById('remove-offline');
const removeRetry = document.getElementById('remove-retry');
const removeAnyway = document.getElementById('remove-anyway');
let removePublished = false;
const showRemove = async (call = invoke) => {
  const generation = identityGeneration;
  const state = current;
  const items = await call('list_identities');
  if (!identityCurrent(generation)) return;
  const copies = state.peerId && items.some((item) => item.slot !== state.slot
    && item.status === 'ready' && item.peerId === state.peerId);
  const whoRow = document.getElementById('remove-who');
  whoRow.replaceChildren(...who(whoRow, state.peerId, state.reason, modeText(state)));
  const peer = document.getElementById('remove-peer');
  peer.textContent = state.peerId || reasonText(state.reason);
  peer.parentElement.hidden = !state.peerId;
  const path = document.getElementById('remove-path');
  path.hidden = !state.keyPath;
  path.querySelector('code').textContent = state.keyPath || '';
  document.getElementById('remove-copies').textContent = !state.peerId ? ''
    : window.strings[copies ? 'remove.otherCopy' : 'remove.lastCopy'];
  // A missing key or another copy means nothing irreplaceable is lost.
  document.getElementById('remove-warning').hidden = !state.peerId || copies;
  // A published profile is withdrawn from search before the key goes.
  const view = state.status === 'ready' && await call('load_profile').catch(() => null);
  if (!identityCurrent(generation)) return;
  removePublished = Boolean(view?.published);
  document.getElementById('remove-text').hidden = !removePublished;
  removeAsk.hidden = false;
  removeOffline.hidden = true;
  removeConfirm.checked = false;
  removeAction.disabled = true;
  removeDialog.hidden = false;
  removeDialog.showModal();
  document.getElementById('remove-cancel').focus();
};
removeDialog.addEventListener('close', () => { removeDialog.hidden = true; });
for (const button of [removeIdentity, settingsRemove]) {
  button.addEventListener('click', () => {
    const generation = identityGeneration;
    showRemove().catch(() => { if (identityCurrent(generation)) showError(); });
  });
}
for (const id of ['setup-cancel', 'picker-cancel']) {
  document.getElementById(id).addEventListener('click', () => (current.status === 'ready'
    ? show(identity, 'status.ready') : show(sections[3], 'status.recovery')));
}
removeConfirm.addEventListener('change', () => { removeAction.disabled = !removeConfirm.checked; });
const removeNow = () => {
  const removed = current;
  // Nothing is open after a removal until the backend reports the next state.
  current = null;
  return act(invoke('remove_identity', { slot: removed.slot, peerId: removed.peerId || null }));
};
const revokedDialog = document.getElementById('revoked');
revokedDialog.addEventListener('close', () => { revokedDialog.hidden = true; });
document.getElementById('revoked-ok').addEventListener('click', () => revokedDialog.close());
// The runtime found the device of `slot` revoked. Nothing is asked: the owner already decided on
// another device, and the profile stays theirs, so there is no tombstone either. A failed removal
// shows the error screen; a pass tries again once the identity opens.
window.deviceRevoked = async (slot, remove = removeNow) => {
  if (current?.slot !== slot) return;
  for (const dialog of document.querySelectorAll('dialog[open]')) dialog.close();
  const state = await remove();
  if (state?.status === 'error') return;
  revokedDialog.hidden = false;
  revokedDialog.showModal();
  document.getElementById('revoked-ok').focus();
};
// The tombstone goes first; when no node takes it, the person chooses to wait or remove anyway.
const withdrawThenRemove = async (call = invoke, remove = removeNow) => {
  const generation = identityGeneration;
  if (removePublished) {
    try {
      await call('withdraw_profile');
    } catch (_) {
      if (!identityCurrent(generation)) return null;
      removeAsk.hidden = true;
      removeOffline.hidden = false;
      removeRetry.disabled = false;
      removeAnyway.disabled = false;
      document.getElementById('remove-cancel-offline').focus();
      return null;
    }
  }
  if (!identityCurrent(generation)) return null;
  return remove();
};
removeAction.addEventListener('click', () => {
  removeAction.disabled = true;
  window.removeDone = withdrawThenRemove();
});
removeRetry.addEventListener('click', () => {
  removeRetry.disabled = true;
  removeAnyway.disabled = true;
  window.removeDone = withdrawThenRemove();
});
removeAnyway.addEventListener('click', () => {
  removeAnyway.disabled = true;
  window.removeDone = removeNow();
});
for (const id of ['remove-cancel', 'remove-cancel-offline']) {
  document.getElementById(id).addEventListener('click', () => removeDialog.close());
}
