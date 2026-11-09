import { byId as domById, eventDetail } from './dom.js';
import { identityCurrent, identityState } from './identity-state.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
import { loadPeerNames, controller as peerNamesController } from './peer-names.js';
import { withPlaces, searchMain, openFound } from './search.js';
export const controller = {};
/** @type {Promise<void> | undefined} */
controller.savedOpened = undefined;
/** @type {Promise<void> | undefined} */
controller.savedRemoved = undefined;
/** @type {Promise<void> | undefined} */
controller.savedShown = undefined;

// The list reads local IDs and labels; only pressing a row fetches a verified profile.
const savedPanel = domById('saved-profiles', HTMLDetailsElement);
const savedRows = domById('saved-profile-rows', HTMLUListElement);
const savedStatus = domById('saved-profile-status', HTMLParagraphElement);
const savedRefresh = domById('saved-profile-refresh', HTMLButtonElement);
/**
 * @typedef {{ run: number, generation: number, call: import('./ipc.js').Invoke }} SavedContext
 * @typedef {{ peerId: string, context: SavedContext, row: HTMLLIElement }} SavedRow
 * @typedef {SavedRow & { open: HTMLButtonElement, selection: number }} SavedOpening
 * @typedef {SavedRow & { open: HTMLButtonElement, remove: HTMLButtonElement }} SavedActions
 */
let savedRun = 0;
let savedSelection = 0;
let savedCall = invoke;
/** @param {SavedContext} context */
const savedCurrent = (context) =>
  context.run === savedRun && identityCurrent(context.generation) && savedPanel.open;
/** @param {string} key */
const savedMessage = (key) => {
  savedStatus.textContent = key ? strings[`favorites.${key}`] : '';
};
/** @param {SavedOpening} opening */
const openSavedProfile = async ({ peerId, context, row, open, selection }) => {
  const [found] = await withPlaces([await context.call('shared_profile', { peerId })]);
  if (
    selection === savedSelection &&
    savedCurrent(context) &&
    row.isConnected &&
    !searchMain.hidden &&
    !domById('search-area', HTMLElement).hidden
  )
    openFound(found, open, context.call);
};
/** @param {SavedRow} selection */
const removeSavedProfile = async ({ peerId, context, row }) => {
  await context.call('set_favorite', { peerId, favorite: false });
  if (!savedCurrent(context) || !row.isConnected) return;
  await controller.showSaved(context.call);
  if (savedPanel.open && identityCurrent(context.generation)) savedRefresh.focus();
};
/** @param {boolean} opening @param {number} selection @param {SavedContext} context @param {HTMLLIElement} row */
const savedRowFailure = (opening, selection, context, row) => {
  if ((!opening || selection === savedSelection) && savedCurrent(context) && row.isConnected) {
    savedMessage(opening ? 'openFailed' : 'changeFailed');
  }
};
/** @param {SavedActions} actions */
const savedRowAction = ({ peerId, context, row, open, remove }) => {
  let busy = false;
  /** @param {boolean} opening */
  const run = async (opening) => {
    if (busy || !savedCurrent(context) || !row.isConnected) return;
    const selection = opening ? ++savedSelection : savedSelection;
    busy = true;
    open.disabled = true;
    remove.disabled = true;
    savedMessage(opening ? 'opening' : 'saving');
    try {
      if (opening) await openSavedProfile({ peerId, context, row, open, selection });
      else await removeSavedProfile({ peerId, context, row });
    } catch {
      savedRowFailure(opening, selection, context, row);
    } finally {
      if (savedCurrent(context) && row.isConnected) {
        busy = false;
        open.disabled = false;
        remove.disabled = false;
      }
    }
  };
  return run;
};
/**
 * @param {string} peerId
 * @param {SavedContext} context
 */
const savedRow = (peerId, context) => {
  const row = document.createElement('li');
  row.className = 'actions';
  row.dataset.peerId = peerId;
  const open = document.createElement('button');
  open.type = 'button';
  open.className = 'btn';
  open.textContent = peerNamesController.peerName(peerId);
  const remove = document.createElement('button');
  remove.type = 'button';
  remove.className = 'btn quiet';
  remove.textContent = strings['favorites.remove'];
  remove.setAttribute(
    'aria-label',
    `${strings['favorites.remove']} ${peerNamesController.peerName(peerId)}`,
  );
  const run = savedRowAction({ peerId, context, row, open, remove });
  open.addEventListener('click', () => {
    controller.savedOpened = run(true);
  });
  remove.addEventListener('click', () => {
    controller.savedRemoved = run(false);
  });
  row.append(open, remove);
  return row;
};
controller.showSaved = async (call = savedCall) => {
  savedCall = call;
  const context = { run: ++savedRun, generation: identityState.generation, call };
  savedRows.replaceChildren();
  savedRefresh.disabled = true;
  savedMessage('listLoading');
  try {
    const peers = await call('favorite_peers');
    if (!savedCurrent(context)) return;
    await loadPeerNames(peers, call);
    if (!savedCurrent(context)) return;
    savedRows.replaceChildren(...peers.map((peer) => savedRow(peer, context)));
    savedMessage(peers.length ? '' : 'empty');
  } catch {
    if (savedCurrent(context)) savedMessage('listFailed');
  } finally {
    if (savedCurrent(context)) savedRefresh.disabled = false;
  }
};
controller.refreshSaved = () => {
  if (savedPanel.open && !searchMain.hidden && !domById('search-area', HTMLElement).hidden) {
    controller.savedShown = controller.showSaved();
  }
};
const closeSaved = () => {
  ++savedRun;
  savedPanel.open = false;
  savedRows.replaceChildren();
  savedMessage('');
  savedRefresh.disabled = false;
};
savedPanel.addEventListener('toggle', () => {
  if (savedPanel.open) {
    if (!savedRefresh.disabled) controller.savedShown = controller.showSaved();
  } else {
    ++savedRun;
    savedRefresh.disabled = false;
  }
});
savedRefresh.addEventListener('click', () => {
  controller.savedShown = controller.showSaved();
});
document.addEventListener('dyapp:identity-change', () => {
  closeSaved();
  savedCall = invoke;
});
document.addEventListener('dyapp:area-change', (event) => {
  if (eventDetail(event) !== 'search-area') closeSaved();
});
