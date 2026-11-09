import { identityCurrent, identityState } from './identity-state.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
import { viewed } from './search.js';
export const controller = {};

// Saved state is local. An unknown state can only be retried, never toggled.
const favoritePanel = document.getElementById('view-favorite');
const favoriteButton = document.getElementById('view-favorite-toggle');
const favoriteStatus = document.getElementById('view-favorite-status');
let favoriteView = null;
const favoriteCurrent = (target) =>
  favoriteView === target && viewed === target.view && identityCurrent(target.generation);
const renderFavorite = (target) => {
  favoriteButton.disabled = target.busy;
  const key = target.busy
    ? target.saved === null
      ? 'loading'
      : 'saving'
    : target.saved === null
      ? 'retry'
      : target.saved
        ? 'remove'
        : 'add';
  favoriteButton.textContent = strings[`favorites.${key}`];
  favoriteStatus.textContent = target.status ? strings[`favorites.${target.status}`] : '';
  favoriteStatus.hidden = !target.status;
};
controller.clearFavorite = () => {
  favoriteView = null;
  favoritePanel.hidden = true;
  favoriteButton.disabled = true;
};
controller.showFavorite = async (call = invoke) => {
  controller.clearFavorite();
  const view = viewed;
  if (!view || view.found.peerId === identityState.current?.peerId) return;
  const target = {
    view,
    generation: identityState.generation,
    call,
    saved: null,
    busy: true,
    status: '',
  };
  favoriteView = target;
  favoritePanel.hidden = false;
  renderFavorite(target);
  try {
    const saved = await call('favorite_state', { peerId: view.found.peerId });
    if (!favoriteCurrent(target)) return;
    if (typeof saved !== 'boolean') throw new Error('invalid favorite state');
    target.saved = saved;
  } catch {
    if (!favoriteCurrent(target)) return;
    target.status = 'loadFailed';
  } finally {
    if (favoriteCurrent(target)) {
      target.busy = false;
      renderFavorite(target);
    }
  }
};
controller.toggleFavorite = async () => {
  const target = favoriteView;
  if (!target || !favoriteCurrent(target) || target.busy) return;
  if (target.saved === null) return controller.showFavorite(target.call);
  const favorite = !target.saved;
  target.busy = true;
  target.status = '';
  renderFavorite(target);
  try {
    await target.call('set_favorite', { peerId: target.view.found.peerId, favorite });
    if (!favoriteCurrent(target)) return;
    target.saved = favorite;
    target.status = favorite ? 'saved' : 'removed';
  } catch (error) {
    if (!favoriteCurrent(target)) return;
    target.status = String(error) === 'blocked' ? 'blocked' : 'changeFailed';
  } finally {
    if (favoriteCurrent(target)) {
      target.busy = false;
      renderFavorite(target);
    }
  }
};
favoriteButton.addEventListener('click', () => {
  controller.favoriteChanged = controller.toggleFavorite();
});
document.addEventListener('dyapp:identity-change', controller.clearFavorite);
