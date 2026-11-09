import { byId as domById } from './dom.js';
import { chatState } from './chat-state.js';
import { renderChatRows, visibleChatRows, openChat } from './chats-ui.js';
import { identityState, identityCurrent } from './identity-state.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
import { shortPeer } from './peer-labels.js';
export const controller = {};
/** @type {Promise<void> | undefined} */
controller.chatRenamed = undefined;

// Contact names: the page's copy of each peer's local or public name, and the dialog that sets the
// local one. A public name is the peer's own claim, so it always shows its mark and short Peer ID.
/** @type {Map<string, import('./ipc.js').PeerName>} */
const peerNames = new Map();
controller.peerName = (id) => {
  const short = shortPeer(id);
  const entry = peerNames.get(id);
  if (!entry?.name) return short;
  if (entry.verified === null) return entry.name;
  const mark = strings[entry.verified ? 'peer.verified' : 'peer.unverified'];
  return `${entry.name} · ${mark} · ${short}`;
};
controller.peerPhoto = (id) => peerNames.get(id)?.photo;
// A failed read keeps the last names: every peer still shows at least its short Peer ID.
/** @param {import('./ipc.js').Invoke} call */
const loadPeerNames = async (peerIds, call) => {
  if (!peerIds.length) return;
  const generation = identityState.generation;
  try {
    const names = await call('contact_names', { peerIds });
    if (!identityCurrent(generation) || !Array.isArray(names)) return;
    for (const entry of names) peerNames.set(entry.peerId, entry);
  } catch {
    /* the page falls back to the short Peer ID */
  }
};
document.addEventListener('dyapp:identity-change', () => peerNames.clear());

const renameDialog = domById('chat-rename-dialog', HTMLDialogElement);
const renameInput = domById('chat-rename-input', HTMLInputElement);
const renameError = domById('chat-rename-error', HTMLParagraphElement);
const renameSave = domById('chat-rename-save', HTMLButtonElement);
/** @type {{ peerId: string, generation: number } | null} */
let renameTarget = null;
const closeRename = () => {
  if (renameDialog.open) renameDialog.close();
  renameDialog.hidden = true;
  renameTarget = null;
  renameSave.disabled = false;
};
renameDialog.addEventListener('close', () => {
  if (!renameDialog.open) closeRename();
});
domById('chat-rename-cancel', HTMLButtonElement).addEventListener('click', closeRename);
domById('chat-rename', HTMLButtonElement).addEventListener('click', () => {
  if (!chatState.activeChat) return;
  const { peerId } = chatState.activeChat;
  const entry = peerNames.get(peerId);
  renameTarget = { peerId, generation: identityState.generation };
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
    renameError.textContent =
      String(error) === 'invalidName'
        ? strings['chats.renameInvalid']
        : `${strings['chats.renameFailed']} ${error}`;
    renameError.hidden = false;
    return;
  }
  if (renameTarget === target) closeRename();
  await loadPeerNames([peerId], call);
  if (!identityCurrent(generation)) return;
  renderChatRows(visibleChatRows);
  if (chatState.activeChat?.peerId === peerId) await openChat(peerId, call);
};
domById('chat-rename-form', HTMLFormElement).addEventListener('submit', (event) => {
  event.preventDefault();
  controller.chatRenamed = saveRename();
});
controller.uiSmokeRename = Object.freeze({ saveRename });

export { closeRename, loadPeerNames, peerNames };
