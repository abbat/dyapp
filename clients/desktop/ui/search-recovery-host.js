import { byId } from './dom.js';
import { identityCurrent, identityState } from './identity-state.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
import { searchRecoveryCard } from './search-recovery.js';

const root = byId('profile-search-recovery', HTMLElement);
const area = byId('profile-area', HTMLElement);
/** @type {ReturnType<typeof searchRecoveryCard> | null} */
let card = null;
/** @type {((view: import('./profile-fields.js').ProfileView) => void) | null} */
let metadata = null;
const retire = () => {
  const original = card;
  card = null;
  if (original) void original.retire();
};
/** Read metadata only: a recovery completion must never refill unsaved form inputs. */
/** @param {() => boolean} current */
const refreshMetadata = async (current) => {
  try {
    const view = await invoke('load_profile');
    if (current()) metadata?.(view);
  } catch {
    // The confirmed recovery receipt remains visible; a metadata read cannot undo it.
  }
};
/** @param {(view: import('./profile-fields.js').ProfileView) => void} updated */
export const openSearchRecovery = (updated) => {
  metadata = updated;
  retire();
  const owner = identityState.current;
  if (owner?.status !== 'ready' || document.hidden) return;
  const generation = identityState.generation;
  /** @type {ReturnType<typeof searchRecoveryCard>} */
  let original;
  const current = () =>
    card === original && identityCurrent(generation) && identityState.current === owner;
  original = searchRecoveryCard(root, owner.peerId, invoke, current, strings, () => {
    void refreshMetadata(current);
  });
  card = original;
  void original.prepare();
};
document.addEventListener('dyapp:identity-change', retire);
document.addEventListener('dyapp:area-change', retire);
document.addEventListener('visibilitychange', () => {
  retire();
  if (!document.hidden && metadata && area.getClientRects().length > 0) {
    openSearchRecovery(metadata);
  }
});
