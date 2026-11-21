import { bindCollectionNavigation } from '../../desktop/ui/settings-collection-view.js';

const check = (value, reason) => {
  if (!value) throw new Error(reason);
};

const get = (id) => {
  const node = document.getElementById(id);
  if (!node) throw new Error(`Missing forward fixture: ${id}`);
  return node;
};

const checkState = () => {
  const screenName = document.documentElement.dataset.screen;
  const ready = ['chat-forward-dialog', 'chat-forward-send-error'].includes(screenName);
  check(
    get('chat-forward-confirm').disabled === !ready,
    'Forward admission does not match the state',
  );
  check(
    get('chat-forward-cancel').disabled === (screenName === 'chat-forward-pending'),
    'Cancel must be available during reads and blocked during submission',
  );
  if (['chat-forward-sent', 'chat-forward-queued'].includes(screenName)) {
    const profile = get('chat-profile');
    check(
      [...profile.querySelectorAll('.avatar.photo')].every((node) => node.hidden),
      'Destination retained the source photo',
    );
    check(
      profile.dataset.peerId === get('chat-forward-selected-peer').textContent,
      'Destination header and confirmed Peer ID differ',
    );
    const messages = [...get('chat-thread').querySelectorAll('.msg')].filter(
      (node) => !node.hidden,
    );
    check(
      messages.length === 1 && messages[0].contains(get('chat-forward-body')),
      'Source conversation leaked into the destination',
    );
  }
  if (screenName === 'chat-forward-received-menu') {
    check(
      get('msg-edit').hidden && get('msg-delete').hidden,
      'Received text gained author actions',
    );
    check(
      get('forward-received-menu').contains(get('msg-forward')),
      'Received Forward menu is missing',
    );
  }
  return ready;
};

const checkSelection = () => {
  const list = get('chat-forward-list');
  const selected = get('chat-forward-selected-peer');
  const query = get('chat-forward-query');
  const next = get('chat-forward-next');
  const previous = get('chat-forward-previous');
  const radios = () => [...list.querySelectorAll('input[type=radio]')];
  check(radios().length === 32, 'Forward first page must be bounded');
  next.click();
  check(radios().length === 8, 'Forward second page must contain the remaining targets');
  const chosen = radios()[0];
  chosen.click();
  check(selected.textContent === chosen.value, 'Selected full Peer ID must follow the radio');
  previous.click();
  check(!radios().some((radio) => radio.checked), 'Another page retained an old selection');
  next.click();
  check(
    radios().filter((radio) => radio.checked).length === 1 &&
      radios().find((radio) => radio.checked).value === chosen.value,
    'Selection did not survive paging',
  );
  query.value = chosen.value;
  query.dispatchEvent(new Event('input', { bubbles: true }));
  check(radios().length === 1 && radios()[0].checked, 'Full Peer ID search lost the selection');
  query.value = 'not-a-target';
  query.dispatchEvent(new Event('input', { bubbles: true }));
  check(
    radios().length === 0 && selected.textContent === chosen.value,
    'Search changed the chosen target',
  );
  query.value = '';
  query.dispatchEvent(new Event('input', { bubbles: true }));
  const before = selected.textContent;
  const url = location.href;
  const screen = document.documentElement.dataset.screen;
  // Both mutations happen before observer delivery: a same-screen return is a new opening.
  document.documentElement.dataset.screen = 'chat';
  document.documentElement.dataset.screen = screen;
  radios()[0].click();
  get('chat-forward-confirm').click();
  get('chat-forward-cancel').click();
  check(selected.textContent === before, 'Retired radio changed the selected recipient');
  check(location.href === url, 'Retired confirmation navigated');
};

const checkBindingRetirement = () => {
  const button = () => document.createElement('button');
  const state = {
    name: 'forward',
    list: document.createElement('tbody'),
    page: 0,
    items: [],
    search: document.createElement('input'),
    status: document.createElement('p'),
    previous: button(),
    next: button(),
    refresh: button(),
  };
  let rendered = 0;
  const lifetime = new AbortController();
  bindCollectionNavigation(
    state,
    () => {
      rendered++;
    },
    lifetime.signal,
  );
  state.next.click();
  check(state.page === 1 && rendered === 1, 'Live collection navigation did not render');
  lifetime.abort();
  state.previous.click();
  state.search.dispatchEvent(new Event('input'));
  state.next.click();
  check(state.page === 1 && rendered === 1, 'Retired collection bindings still act');
};

// Runs after the static snapshot; retiring the opening cannot alter the captured state.
export const checkForwardFixture = () => {
  checkBindingRetirement();
  if (checkState()) checkSelection();
};
