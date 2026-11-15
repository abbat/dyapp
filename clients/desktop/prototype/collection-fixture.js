import {
  collectionIds,
  renderCollectionView,
  bindCollectionNavigation,
} from '../ui/settings-collection-view.js';

export const collections = new Map();
let epoch = 0;
const invalidate = (records) => {
  if (!records.length) return;
  epoch += records.length;
  for (const state of collections.values()) state.refresh.disabled = false;
};
const navigation = new MutationObserver(invalidate);
navigation.observe(document.documentElement, {
  attributes: true,
  attributeFilter: ['data-screen'],
});
const syncNavigation = () => invalidate(navigation.takeRecords());

const bindRefresh = (state, render) => {
  state.refresh.addEventListener('click', () => {
    syncNavigation();
    const generation = epoch;
    const screen = document.documentElement.dataset.screen;
    const current = () => {
      syncNavigation();
      return (
        generation === epoch &&
        screen === document.documentElement.dataset.screen &&
        state.list.isConnected
      );
    };
    state.refresh.disabled = true;
    state.refreshed = Promise.resolve()
      .then(() => (current() ? state.reload() : undefined))
      .then(
        (items) => {
          if (!current()) return;
          state.items = items;
          render(state);
        },
        () => {
          if (!current()) return;
          state.items = null;
          render(state);
        },
      )
      .finally(() => {
        if (current()) state.refresh.disabled = false;
      });
  });
};
const create = (name, id, words) => {
  const render = (state) => renderCollectionView(state, words);
  const list = document.getElementById(id);
  // Reusing nodes preserves the prototype keyboard module's bound dialog openers.
  const records = [...list.children].map((row) => ({
    find: () => row.textContent,
    row: () => row,
  }));
  const state = {
    name,
    list,
    search: document.getElementById(`settings-${name}-search`),
    status: document.getElementById(`settings-${name}-status`),
    previous: document.getElementById(`settings-${name}-previous`),
    next: document.getElementById(`settings-${name}-next`),
    refresh: document.getElementById(`settings-${name}-refresh`),
    items: records,
    page: 0,
    reload: async () => records,
    refreshed: undefined,
  };
  bindCollectionNavigation(state, render);
  bindRefresh(state, render);
  collections.set(name, state);
  render(state);
};
export const installCollections = (strings, names = Object.keys(collectionIds)) => {
  const words = Object.fromEntries(
    ['search', 'previous', 'next', 'refresh', 'failed', 'empty', 'page'].map((key) => [
      key,
      strings[`settings.collection.${key}`],
    ]),
  );
  for (const name of names) create(name, collectionIds[name], words);
};
