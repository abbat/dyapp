// Profile links in chats: Share on a profile page and the card a link renders as. Loaded after
// chats-ui.js in the shared page scope; it reuses that file's Forward dialog.
// Forward and Share use one dialog; its title and button name the action.
const showForwardDialog = (action, targets) => {
  for (const [id, key] of [['chat-forward-title', `chats.${action}Title`], ['chat-forward-confirm', `chats.${action}`]]) {
    const element = document.getElementById(id);
    element.dataset.i18n = key;
    element.textContent = window.strings[key];
  }
  forwardSelect.replaceChildren(...targets.map((row) => {
    const option = document.createElement('option');
    option.value = row.peerId;
    option.textContent = window.peerName(row.peerId);
    return option;
  }));
  forwardConfirm.disabled = !targets.length;
  forwardError.hidden = !!targets.length;
  forwardError.textContent = targets.length ? '' : window.strings['chats.forwardNoTargets'];
  forwardDialog.hidden = false;
  forwardDialog.showModal();
  if (targets.length) forwardSelect.focus();
  else forwardCancel.focus();
};
// A profile is shared into an open chat, never a request, as an ordinary text.
const openShare = async (peerId, call = invoke) => {
  const generation = identityGeneration;
  const rows = await call('chat_rows').catch(() => []);
  if (!identityCurrent(generation) || forwardDialog.open) return;
  await loadPeerNames(rows.map((row) => row.peerId), call).catch(() => {});
  if (!identityCurrent(generation) || forwardDialog.open) return;
  forwardTarget = { share: peerId, generation };
  showForwardDialog('share', rows.filter((row) => row.state === 'open' && row.peerId !== peerId));
};
document.getElementById('view-share').addEventListener('click', () => {
  const found = viewed?.found;
  if (found) window.shareOpened = openShare(found.peerId);
});
const profileCardText = (peerId) => `${window.strings['chats.profileCard']} ${window.shortPeer(peerId)}`;
// Nothing about the shared peer is fetched until the card is pressed, and then only verified.
const openShared = async (peerId, card, call = invoke) => {
  const generation = identityGeneration;
  try {
    const [found] = await withPlaces([await call('shared_profile', { peerId })]);
    if (identityCurrent(generation) && card.isConnected) openFound(found, card, call);
  } catch (error) {
    if (identityCurrent(generation)) chatComposeStatus.textContent = `${window.strings['chats.profileFailed']} ${String(error)}`;
  }
};
const profileCard = (peerId, call) => {
  const card = document.createElement('button');
  card.type = 'button';
  card.className = 'btn';
  card.dataset.peerId = peerId;
  card.textContent = profileCardText(peerId);
  card.addEventListener('click', () => { window.sharedOpened = openShared(peerId, card, call); });
  return card;
};
const shareProfile = async (call = invoke) => {
  const target = forwardTarget;
  const targetPeerId = forwardSelect.value;
  if (!target || forwardBusy || forwardConfirm.disabled || !targetPeerId) return;
  forwardBusy = true;
  forwardCancel.disabled = true;
  forwardConfirm.disabled = true;
  forwardSelect.disabled = true;
  try {
    await call('share_profile', { peerId: target.share, targetPeerId });
  } catch (error) {
    if (forwardTarget !== target || !identityCurrent(target.generation)) return;
    forwardBusy = false;
    forwardCancel.disabled = false;
    forwardConfirm.disabled = false;
    forwardSelect.disabled = false;
    forwardError.textContent = `${window.strings['chats.shareFailed']} ${String(error)}`;
    forwardError.hidden = false;
    return;
  }
  if (forwardTarget !== target || !identityCurrent(target.generation)) return;
  closeForward();
  showArea('chats-area');
  await openChat(targetPeerId, call);
  if (identityCurrent(target.generation) && activeChat?.peerId === targetPeerId) {
    chatComposeStatus.textContent = window.strings['chats.shareSent'];
  }
};
