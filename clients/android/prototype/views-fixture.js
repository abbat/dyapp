import { collections, installCollections } from '../../desktop/prototype/collection-fixture.js';
import { renderCollectionView } from '../../desktop/ui/settings-collection-view.js';

export const receivedViews = { state: null, words: null };
/** Non-shipping samples reuse prototype collection controls; production paging remains in Rust. */
export const installViewsFixture = (strings, screen) => {
  if (!screen.startsWith('settings-views')) return;
  installCollections(strings, ['views'], { views: 'received-views-rows' });
  const state = collections.get('views');
  state.items = state.items.map((item) => ({
    ...item,
    find: () => item.row().dataset.peerId,
  }));
  const known = state.items;
  state.reload = async () => known;
  if (screen === 'settings-views-empty') state.items = [];
  if (['settings-views-loading', 'settings-views-error'].includes(screen)) state.items = null;
  const words = Object.fromEntries(
    ['search', 'previous', 'next', 'refresh', 'failed', 'empty', 'page'].map((key) => [
      key,
      strings[`settings.collection.${key}`],
    ]),
  );
  renderCollectionView(state, words);
  state.search.disabled = screen === 'settings-views-loading';
  state.refresh.disabled = screen === 'settings-views-loading';
  receivedViews.state = state;
  receivedViews.words = words;
};
