import { byId as domById } from './dom.js';
import {
  collectionIds as ids,
  createCollectionView,
  renderCollectionView,
  bindCollectionNavigation,
} from './settings-collection-view.js';
import { identityState, identityCurrent } from './identity-state.js';
import { settingsRemove } from './elements.js';
import { strings } from './locale.js';
export const controller = {};
/** @type {Promise<unknown> | undefined} */
controller.settingsCollectionRefreshed = undefined;

/**
 * @typedef {keyof typeof ids} CollectionName
 * @typedef {import('./settings-collection-view.js').CollectionState} CollectionState
 */
/** @type {Map<CollectionName, CollectionState>} */
const states = new Map();
/** @param {string} key */
const text = (key) => strings[`settings.collection.${key}`];
const collectionWords = () => ({
  search: strings['settings.collection.search'],
  previous: strings['settings.collection.previous'],
  next: strings['settings.collection.next'],
  refresh: strings['settings.collection.refresh'],
  failed: strings['settings.collection.failed'],
  empty: strings['settings.collection.empty'],
  page: strings['settings.collection.page'],
});
/** @param {CollectionState} state */
const render = (state) => {
  if (state.name === 'identities') settingsRemove.hidden = true;
  renderCollectionView(state, collectionWords());
};
/** @param {CollectionName} name */
const create = (name) => {
  const state = createCollectionView(name, domById(ids[name], HTMLElement), collectionWords());
  bindControls(state);
  states.set(name, state);
  return state;
};
/** @param {CollectionState} state */
const bindControls = (state) => {
  const { refresh } = state;
  bindCollectionNavigation(state, render);
  refresh.addEventListener('click', () => {
    const generation = identityState.generation;
    refresh.disabled = true;
    controller.settingsCollectionRefreshed = Promise.resolve()
      .then(() => state.reload?.())
      .finally(() => {
        if (identityCurrent(generation)) refresh.disabled = false;
      });
  });
};
/**
 * @template Item
 * @param {CollectionName} name
 * @param {Item[] | null} items
 * @param {(item: Item, index: number) => HTMLElement} row
 * @param {(item: Item) => string} find
 * @param {() => void | Promise<unknown>} reload
 */
controller.renderSettingsRows = (name, items, row, find, reload) => {
  const state = states.get(name) || create(name);
  state.items =
    items?.map((item) => ({ find: () => find(item), row: (index) => row(item, index) })) ?? null;
  state.reload = reload;
  render(state);
};
/** @param {CollectionName} name */
controller.settingsCollectionFailed = (name) => {
  const state = states.get(name);
  if (state) state.status.textContent = text('failed');
};
document.addEventListener('dyapp:identity-change', () => {
  for (const state of states.values()) {
    state.items = null;
    state.page = 0;
    state.search.value = '';
    state.refresh.disabled = false;
    render(state);
  }
});
