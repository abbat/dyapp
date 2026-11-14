import { collections } from '../prototype/collection-fixture.js';
import { showState } from '../prototype/keyboard.js';
import { renderCollectionView } from '../ui/settings-collection-view.js';
import { beginFixture, endFixture } from './prototype-keyboard.js';

const check = (condition, reason) => {
  if (!condition) throw new Error(reason);
};
const words = () =>
  Object.fromEntries(
    ['search', 'previous', 'next', 'refresh', 'failed', 'empty', 'page'].map((key) => [
      key,
      window.strings[`settings.collection.${key}`],
    ]),
  );

// The controlled thenable queues completion before MutationObserver delivery.
// No timer or guessed microtask count decides which owner receives the result.
export const checkRefreshOwner = async (name, outcome) => {
  const state = collections.get(name);
  check(Boolean(state), `Missing collection fixture: ${name}`);
  const saved = {
    reload: state.reload,
    items: state.items,
    page: state.page,
    search: state.search.value,
    disabled: state.refresh.disabled,
    rows: [...state.list.children],
  };
  await beginFixture('collection');
  try {
    const screen = name === 'identities' ? 'settings-identities-screen' : `settings-${name}`;
    showState(screen);
    await new Promise(queueMicrotask);
    state.reload = () => ({
      // biome-ignore lint/suspicious/noThenProperty: this controlled thenable tests completion before observer delivery.
      then(resolve, reject) {
        if (outcome === 'resolve') resolve([]);
        else reject(new Error('Old collection refresh failed'));
        showState('settings');
        showState(screen);
      },
    });
    state.refresh.click();
    await state.refreshed;
    check(state.items === saved.items, 'Queued refresh crossed a synchronous view change');
    const rows = [...state.list.children];
    check(
      rows.length === saved.rows.length && rows.every((row, index) => row === saved.rows[index]),
      'Queued refresh replaced the original collection rows',
    );
    check(!state.refresh.disabled, 'Stale refresh left the collection disabled');
  } finally {
    state.reload = saved.reload;
    state.items = saved.items;
    state.page = saved.page;
    state.search.value = saved.search;
    renderCollectionView(state, words());
    state.refresh.disabled = saved.disabled;
    endFixture();
  }
};
