import { byId } from '../../desktop/ui/dom.js';
import {
  renderCollectionView,
  bindCollectionNavigation,
} from '../../desktop/ui/settings-collection-view.js';

export const forwardScreens = [
  'chat-forward-dialog',
  'chat-forward-loading',
  'chat-forward-empty',
  'chat-forward-error',
  'chat-forward-send-error',
  'chat-forward-pending',
  'chat-forward-sent',
  'chat-forward-queued',
  'chat-forward-received-menu',
];

/** Static fixtures only: this module never contacts nodes or stages messages.
 * @param {Record<string, string>} words @param {string} screen */
export const installForwardFixture = (words, screen) => {
  const query = new URLSearchParams(location.search);
  const list = byId('chat-forward-list', HTMLTableSectionElement);
  const selected = byId('chat-forward-selected-peer', HTMLElement);
  const name = byId('chat-forward-selected-name', HTMLElement);
  const confirm = byId('chat-forward-confirm', HTMLButtonElement);
  const cancel = byId('chat-forward-cancel', HTMLButtonElement);
  const ready = ['chat-forward-dialog', 'chat-forward-send-error'].includes(screen);
  const pending = screen === 'chat-forward-pending';
  let active = true;
  let chosen = query.get('forwardTo') || '1'.padStart(64, '0');
  const navigation = new MutationObserver((records) => {
    if (records.length) active = false;
  });
  navigation.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['data-screen'],
  });
  const current = () => {
    if (navigation.takeRecords().length) active = false;
    return active && list.isConnected && document.documentElement.dataset.screen === screen;
  };
  const chosenNumber = Number.parseInt(chosen, 16);
  if (
    !Number.isInteger(chosenNumber) ||
    chosenNumber < 1 ||
    chosenNumber > 40 ||
    chosen !== chosenNumber.toString(16).padStart(64, '0')
  )
    chosen = '1'.padStart(64, '0');
  /** @param {string} destination */
  const navigate = (destination) => {
    if (!current()) return;
    active = false;
    const url = new URL(location.href);
    url.searchParams.set('forwardTo', chosen);
    url.hash = destination;
    location.assign(url);
  };
  /** @param {string} peer @param {string} label */
  const choose = (peer, label) => {
    if (!current() || !ready) return;
    chosen = peer;
    selected.textContent = peer;
    name.textContent = label;
    for (const radio of list.querySelectorAll('input')) radio.checked = radio.value === peer;
  };
  const records = Array.from({ length: 40 }, (_, index) => {
    const peer = (index + 1).toString(16).padStart(64, '0');
    const label = `Robin ${index + 1}`;
    const row = document.createElement('tr');
    const cell = row.insertCell();
    const choice = document.createElement('label');
    choice.className = 'choice';
    const radio = document.createElement('input');
    radio.type = 'radio';
    radio.name = 'chat-forward-target';
    radio.value = peer;
    radio.checked = chosen === peer;
    radio.addEventListener('change', () => choose(peer, label));
    const body = document.createElement('span');
    const title = document.createElement('b');
    title.textContent = label;
    const code = document.createElement('code');
    code.textContent = peer;
    body.append(title, document.createElement('br'), code);
    choice.append(radio, body);
    cell.append(choice);
    return {
      find: () => `${label} ${peer}`,
      row: () => {
        radio.checked = chosen === peer;
        return row;
      },
    };
  });
  /** @type {import('../../desktop/ui/settings-collection-view.js').CollectionState} */
  const state = {
    name: 'forward',
    list,
    page: 0,
    search: byId('chat-forward-query', HTMLInputElement),
    status: byId('chat-forward-count', HTMLParagraphElement),
    previous: byId('chat-forward-previous', HTMLButtonElement),
    next: byId('chat-forward-next', HTMLButtonElement),
    refresh: byId('chat-forward-refresh', HTMLButtonElement),
    items: screen === 'chat-forward-empty' ? [] : records,
  };
  const collectionWords = Object.fromEntries(
    ['search', 'previous', 'next', 'refresh', 'failed', 'empty', 'page'].map((key) => [
      key,
      words[`settings.collection.${key}`],
    ]),
  );
  /** @param {typeof state} value */
  const render = (value) => {
    if (current()) renderCollectionView(value, collectionWords);
  };
  bindCollectionNavigation(state, render);
  state.refresh.addEventListener('click', () => {
    if (current() && ready) {
      state.page = 0;
      state.search.value = '';
      render(state);
    }
  });
  confirm.disabled = !ready;
  cancel.disabled = pending;
  confirm.addEventListener('click', () => {
    if (ready) navigate('chat-forward-sent');
  });
  cancel.addEventListener('click', () => {
    if (!pending) navigate('chat');
  });
  byId('chat-forward-retry', HTMLButtonElement).addEventListener('click', () =>
    navigate('chat-forward-dialog'),
  );
  for (const id of ['msg-forward']) {
    byId(id, HTMLButtonElement).addEventListener('click', () => {
      if (!current()) return;
      query.set('forwardSource', screen === 'chat-forward-received-menu' ? 'received' : 'own');
      const url = new URL(location.href);
      url.search = query.toString();
      url.hash = 'chat-forward-dialog';
      active = false;
      location.assign(url);
    });
  }
  if (screen === 'chat-forward-received-menu') {
    byId('forward-received-menu', HTMLElement).append(byId('msg-menu', HTMLElement));
    byId('msg-edit', HTMLButtonElement).hidden = true;
    byId('msg-delete', HTMLButtonElement).hidden = true;
  }
  render(state);
  const chosenIndex = Number.parseInt(chosen, 16) - 1;
  selected.textContent = chosen;
  name.textContent = `Robin ${chosenIndex < 0 ? 1 : chosenIndex + 1}`;
  if (['chat-forward-sent', 'chat-forward-queued'].includes(screen)) {
    byId('chat-presence', HTMLElement).hidden = true;
    byId('chat-profile', HTMLElement).dataset.peerId = chosen;
    const heading = byId('chat-profile', HTMLElement).querySelector('.grow > b');
    if (heading) heading.textContent = name.textContent;
    byId('chat-forward-body', HTMLElement).textContent =
      `${words['forward.marker']}: ${words[query.get('forwardSource') === 'received' ? 'chat.m2' : 'chat.m3']}`;
  }
  return { state, choose, current };
};
