// Contact names: the page's copy of each peer's local or public name, and the dialog that sets the
// local one. A public name is the peer's own claim, so it always shows its mark and short Peer ID.
const peerNames = new Map();
window.peerName = (id) => {
  const short = window.shortPeer(id);
  const entry = peerNames.get(id);
  if (!entry?.name) return short;
  if (entry.verified === null) return entry.name;
  const mark = window.strings[entry.verified ? 'peer.verified' : 'peer.unverified'];
  return `${entry.name} · ${mark} · ${short}`;
};
window.peerPhoto = (id) => peerNames.get(id)?.photo;
// A failed read keeps the last names: every peer still shows at least its short Peer ID.
const loadPeerNames = async (peerIds, call) => {
  if (!peerIds.length) return;
  const generation = identityGeneration;
  try {
    const names = await call('contact_names', { peerIds });
    if (!identityCurrent(generation) || !Array.isArray(names)) return;
    for (const entry of names) peerNames.set(entry.peerId, entry);
  } catch { /* the page falls back to the short Peer ID */ }
};
document.addEventListener('dyapp:identity-change', () => peerNames.clear());

const renameDialog = document.getElementById('chat-rename-dialog');
const renameInput = document.getElementById('chat-rename-input');
const renameError = document.getElementById('chat-rename-error');
const renameSave = document.getElementById('chat-rename-save');
let renameTarget = null;
const closeRename = () => {
  if (renameDialog.open) renameDialog.close();
  renameDialog.hidden = true;
  renameTarget = null;
  renameSave.disabled = false;
};
renameDialog.addEventListener('close', () => { if (!renameDialog.open) closeRename(); });
document.getElementById('chat-rename-cancel').addEventListener('click', closeRename);
document.getElementById('chat-rename').addEventListener('click', () => {
  if (!activeChat) return;
  const { peerId } = activeChat;
  const entry = peerNames.get(peerId);
  renameTarget = { peerId, generation: identityGeneration };
  renameInput.value = entry?.verified === null ? entry.name : '';
  renameError.hidden = true;
  renameDialog.hidden = false;
  renameDialog.showModal();
  renameInput.focus();
});
const saveRename = async (call = invoke) => {
  const target = renameTarget;
  if (!target) return;
  const { peerId, generation } = target;
  renameSave.disabled = true;
  renameError.hidden = true;
  try {
    await call('set_contact_name', { peerId, name: renameInput.value });
  } catch (error) {
    if (renameTarget !== target) return;
    renameSave.disabled = false;
    renameError.textContent = String(error) === 'invalidName'
      ? window.strings['chats.renameInvalid']
      : `${window.strings['chats.renameFailed']} ${error}`;
    renameError.hidden = false;
    return;
  }
  if (renameTarget === target) closeRename();
  await loadPeerNames([peerId], call);
  if (!identityCurrent(generation)) return;
  renderChatRows(visibleChatRows);
  if (activeChat?.peerId === peerId) await openChat(peerId, call);
};
document.getElementById('chat-rename-form').addEventListener('submit', (event) => {
  event.preventDefault();
  window.chatRenamed = saveRename();
});
globalThis.uiSmokeRename = Object.freeze({ saveRename });
