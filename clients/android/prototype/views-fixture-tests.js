import { checkControls, isVisible } from './forward-reachability.js';
import { receivedViews } from './views-fixture.js';
import { renderCollectionView } from '../../desktop/ui/settings-collection-view.js';

const check = (value, message) => {
  if (!value) throw new Error(message);
};
const search = (state, value) => {
  state.search.value = value;
  state.search.dispatchEvent(new Event('input'));
};
const checkPaging = (state, words) => {
  const original = state.items;
  const template = original[0].row();
  const samples = Array.from({ length: 70 }, (_, index) => {
    const target = index.toString(16).padStart(64, '0');
    const row = template.cloneNode(true);
    row.dataset.peerId = target;
    row.querySelector('code').textContent = target;
    return { find: () => target, row: () => row };
  });
  try {
    state.items = samples;
    renderCollectionView(state, words);
    const count = state.list.children.length;
    check(
      !state.next.disabled && count < samples.length,
      'prototype collection has no bounded page',
    );
    state.next.click();
    check(
      state.list.firstElementChild.dataset.peerId === samples[count].find(),
      'next page lost its original target',
    );
    search(state, samples.at(-1).find());
    check(state.list.children.length === 1, 'full-target search missed a later page');
    check(
      state.list.firstElementChild.dataset.peerId === samples.at(-1).find(),
      'search changed its target',
    );
    search(state, 'not-a-fixture-target');
    check(!state.list.children.length, 'no-match query fabricated a view');
  } finally {
    state.items = original;
    search(state, '');
  }
};

/** Post-capture controls exercise presentation only; these samples do not prove private delivery. */
export const checkViewsFixture = async () => {
  const screen = document.documentElement.dataset.screen;
  const { state, words } = receivedViews;
  const populated = ['settings-views', 'settings-views-stale'].includes(screen);
  check(
    state.list.children.length === (populated ? 1 : 0),
    'wrong received-view rows for read outcome',
  );
  const failed = ['settings-views-error', 'settings-views-stale'].includes(screen);
  check(
    isVisible(document.getElementById('received-views-error')) === failed,
    'wrong read-error state',
  );
  check(
    state.refresh.disabled === (screen === 'settings-views-loading'),
    'pending read admits another refresh',
  );
  if (populated) {
    const row = state.list.firstElementChild;
    check(
      row.querySelector('a').getAttribute('href') === '#profile-view',
      'Open has no explicit destination',
    );
    check(row.textContent.includes('2023-11-14'), 'prototype lost the UTC date');
    checkPaging(state, words);
  }
  const controls = [
    ...document.querySelectorAll(
      '#settings-views button, #settings-views input, #settings-views a',
    ),
  ].filter((node) => isVisible(node) && !node.disabled);
  await checkControls(controls);
};
