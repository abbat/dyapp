import { all as domAll, byId as domById, eventDetail } from './dom.js';
import { showArea } from './app.js';
import { identityCurrent, identityState } from './identity-state.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
import { shortPeerId } from './peer-labels.js';
import { receivedViewState } from './received-view-state.js';
import { openFound, withPlaces } from './search.js';
import { controller as settingsNav } from './settings-nav.js';

export const controller = {};
/** @type {Promise<void> | undefined} */
controller.viewsShown = undefined;
/** @type {Promise<void> | undefined} */
controller.viewOpened = undefined;
/** @type {Promise<void> | undefined} */
controller.viewsReturned = undefined;
const pane = domById('settings-views', HTMLDivElement);
const query = domById('received-views-query', HTMLInputElement);
const rows = domById('received-views-rows', HTMLUListElement);
const status = domById('received-views-status', HTMLParagraphElement);
const pageLabel = domById('received-views-page', HTMLParagraphElement);
const previous = domById('received-views-previous', HTMLButtonElement);
const next = domById('received-views-next', HTMLButtonElement);
const refresh = domById('received-views-refresh', HTMLButtonElement);
/** @type {ReturnType<typeof receivedViewState> | null} */
let model = null;
let generation = -1;
let selection = 0;
let lastCall = invoke;
const visible = () => pane.getClientRects().length > 0 && !document.hidden;

/** Return through actual Settings navigation, then focus only the current replacement row. */
const returnToViews = async (owner = generation, peer = '') => {
  if (!identityCurrent(owner)) return;
  showArea('settings');
  settingsNav.openSettingsPane('views');
  await controller.viewsShown;
  if (!identityCurrent(owner) || !visible()) return;
  const button = [...domAll(rows, 'button', HTMLButtonElement)].find(
    (item) => item.dataset.peerId === peer,
  );
  (button ?? refresh).focus();
};
/**
 * @param {{ target: string, label: string }} entry
 * @param {import('./ipc.js').ReceivedViewPage} page
 * @param {ReturnType<typeof receivedViewState>} original
 */
const row = (entry, page, original) => {
  const owner = generation;
  const call = lastCall;
  const item = document.createElement('li');
  item.className = 'card';
  const peer = document.createElement('code');
  peer.textContent = entry.target;
  const date = document.createElement('p');
  date.className = 'caption';
  date.textContent = strings['views.date'].replace('{date}', entry.label);
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'btn';
  button.textContent = strings['views.open'];
  button.dataset.peerId = entry.target;
  button.setAttribute('aria-label', `${strings['views.open']} ${shortPeerId(entry.target)}`);
  button.disabled = original.state.busy;
  /** @param {number} opening */
  const current = (opening) => opening === selection && original.admits(page) && button.isConnected;
  const open = async () => {
    if (!original.admits(page) || !button.isConnected || button.disabled) return;
    const opening = ++selection;
    button.disabled = true;
    try {
      const [found] = await withPlaces([await call('shared_profile', { peerId: entry.target })]);
      if (!current(opening)) return;
      openFound(found, button, call, () => {
        controller.viewsReturned = returnToViews(owner, entry.target);
      });
    } catch {
      if (current(opening)) status.textContent = strings['favorites.openFailed'];
    } finally {
      if (current(opening)) button.disabled = false;
    }
  };
  button.addEventListener('click', () => {
    controller.viewOpened = open();
  });
  item.append(peer, date, button);
  return item;
};
const render = () => {
  if (!model) return;
  const original = model;
  const { page, busy, failed } = original.state;
  refresh.disabled = busy;
  previous.disabled = busy || page?.previous == null;
  next.disabled = busy || page?.next == null;
  rows.replaceChildren(...(page?.entries.map((entry) => row(entry, page, original)) ?? []));
  status.textContent = failed
    ? strings['settings.collection.failed']
    : busy
      ? strings['views.loading']
      : !page?.total
        ? strings['views.empty']
        : !page.matched
          ? strings['settings.collection.empty']
          : '';
  pageLabel.textContent = page
    ? strings['settings.collection.page']
        .replace('{page}', String(page.number))
        .replace('{pages}', String(page.pages))
        .replace('{count}', String(page.matched))
    : '';
};
/** @param {import('./ipc.js').Invoke} [call] */
controller.showReceivedViews = async (call = lastCall) => {
  const changed = call !== lastCall;
  lastCall = call;
  if (!model || changed || generation !== identityState.generation) {
    model?.cancel();
    generation = identityState.generation;
    const owner = generation;
    model = receivedViewState(
      (term, cursor) => call('received_profile_views', { query: term, cursor }),
      () => identityCurrent(owner) && visible(),
      render,
    );
  }
  await model.load(query.value, model.state.cursor);
};
/** @param {import('./ipc.js').CollectionCursor | null} [cursor] */
const load = (cursor = null) => {
  ++selection;
  controller.viewsShown = model?.load(query.value, cursor);
};
query.addEventListener('input', () => load());
previous.addEventListener('click', () => {
  if (!previous.disabled) load(model?.state.page?.previous);
});
next.addEventListener('click', () => {
  if (!next.disabled) load(model?.state.page?.next);
});
refresh.addEventListener('click', () => load(model?.state.cursor));
document.addEventListener('dyapp:settings-pane', (event) => {
  ++selection;
  if (eventDetail(event) === 'views') controller.viewsShown = controller.showReceivedViews();
  else model?.cancel();
});
document.addEventListener('dyapp:area-change', () => {
  ++selection;
  model?.cancel();
});
document.addEventListener('visibilitychange', () => {
  if (document.hidden) {
    ++selection;
    model?.cancel();
  }
});
document.addEventListener('dyapp:identity-change', () => {
  ++selection;
  model?.cancel();
  model = null;
  query.value = '';
  rows.replaceChildren();
  status.textContent = '';
  pageLabel.textContent = '';
});
