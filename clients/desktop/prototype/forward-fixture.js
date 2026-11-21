import { byId } from '../ui/dom.js';
import { renderCollectionView, bindCollectionNavigation } from '../ui/settings-collection-view.js';

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

const createLifetime = (screen, list) => {
  const lifetime = new AbortController();
  let active = true;
  const retire = () => {
    active = false;
    lifetime.abort();
  };
  const navigation = new MutationObserver((records) => {
    if (records.length) retire();
  });
  lifetime.signal.addEventListener('abort', () => navigation.disconnect(), { once: true });
  navigation.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['data-screen'],
  });
  const current = () => {
    if (navigation.takeRecords().length) retire();
    return active && list.isConnected && document.documentElement.dataset.screen === screen;
  };
  return { current, retire, options: { signal: lifetime.signal } };
};

const chosenFromQuery = (query) => {
  const chosen = query.get('forwardTo') || '1'.padStart(64, '0');
  const value = Number.parseInt(chosen, 16);
  return Number.isInteger(value) &&
    value >= 1 &&
    value <= 40 &&
    chosen === value.toString(16).padStart(64, '0')
    ? chosen
    : '1'.padStart(64, '0');
};

const createSelection = (screen) => {
  const query = new URLSearchParams(location.search);
  const list = byId('chat-forward-list', HTMLTableSectionElement);
  const selected = byId('chat-forward-selected-peer', HTMLElement);
  const name = byId('chat-forward-selected-name', HTMLElement);
  const lifetime = createLifetime(screen, list);
  const { current, retire } = lifetime;
  const ready = ['chat-forward-dialog', 'chat-forward-send-error'].includes(screen);
  let chosen = chosenFromQuery(query);
  const navigate = (destination) => {
    if (!current()) return;
    const url = new URL(location.href);
    url.searchParams.set('forwardTo', chosen);
    url.hash = destination;
    retire();
    location.assign(url);
  };
  const choose = (peer, label) => {
    if (!current() || !ready) return;
    chosen = peer;
    selected.textContent = peer;
    name.textContent = label;
    for (const radio of list.querySelectorAll('input')) radio.checked = radio.value === peer;
  };
  return {
    ...lifetime,
    query,
    list,
    selected,
    name,
    navigate,
    choose,
    get chosen() {
      return chosen;
    },
  };
};

const createRecords = (selection) =>
  Array.from({ length: 40 }, (_, index) => {
    const { choose, options } = selection;
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
    radio.checked = selection.chosen === peer;
    radio.addEventListener('change', () => choose(peer, label), options);
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
        radio.checked = selection.chosen === peer;
        return row;
      },
    };
  });

const createCollection = (selection, screen) => ({
  name: 'forward',
  list: selection.list,
  page: 0,
  search: byId('chat-forward-query', HTMLInputElement),
  status: byId('chat-forward-count', HTMLParagraphElement),
  previous: byId('chat-forward-previous', HTMLButtonElement),
  next: byId('chat-forward-next', HTMLButtonElement),
  refresh: byId('chat-forward-refresh', HTMLButtonElement),
  items: screen === 'chat-forward-empty' ? [] : createRecords(selection),
});

const bindActions = (selection, state, screen, render) => {
  const { current, navigate, options } = selection;
  const confirm = byId('chat-forward-confirm', HTMLButtonElement);
  const cancel = byId('chat-forward-cancel', HTMLButtonElement);
  const ready = ['chat-forward-dialog', 'chat-forward-send-error'].includes(screen);
  const pending = screen === 'chat-forward-pending';
  bindCollectionNavigation(state, render, options.signal);
  state.refresh.addEventListener(
    'click',
    () => {
      if (current() && ready) {
        state.page = 0;
        state.search.value = '';
        render(state);
      }
    },
    options,
  );
  confirm.disabled = !ready;
  cancel.disabled = pending;
  confirm.addEventListener(
    'click',
    () => {
      if (ready) navigate('chat-forward-sent');
    },
    options,
  );
  cancel.addEventListener(
    'click',
    () => {
      if (!pending) navigate('chat');
    },
    options,
  );
  byId('chat-forward-retry', HTMLButtonElement).addEventListener(
    'click',
    () => navigate('chat-forward-dialog'),
    options,
  );
};

const bindMenu = (selection, screen) => {
  const { current, query, retire, options } = selection;
  byId('msg-forward', HTMLButtonElement).addEventListener(
    'click',
    () => {
      if (!current()) return;
      query.set('forwardSource', screen === 'chat-forward-received-menu' ? 'received' : 'own');
      const url = new URL(location.href);
      url.search = query.toString();
      url.hash = 'chat-forward-dialog';
      retire();
      location.assign(url);
    },
    options,
  );
  if (screen === 'chat-forward-received-menu') {
    byId('forward-received-menu', HTMLElement).append(byId('msg-menu', HTMLElement));
    byId('msg-edit', HTMLButtonElement).hidden = true;
    byId('msg-delete', HTMLButtonElement).hidden = true;
  }
};

const showDestination = (selection, words, screen) => {
  if (!['chat-forward-sent', 'chat-forward-queued'].includes(screen)) return;
  byId('chat-presence', HTMLElement).hidden = true;
  byId('chat-profile', HTMLElement).dataset.peerId = selection.chosen;
  const avatar = byId('chat-profile', HTMLElement).querySelector('.avatar.p2');
  if (avatar) {
    avatar.classList.remove('photo', 'p2');
    avatar.textContent = 'R';
  }
  const heading = byId('chat-profile', HTMLElement).querySelector('.grow > b');
  if (heading) heading.textContent = selection.name.textContent;
  byId('chat-forward-body', HTMLElement).textContent =
    `${words['forward.marker']}: ${words[selection.query.get('forwardSource') === 'received' ? 'chat.m2' : 'chat.m3']}`;
};

/** Static fixtures only: this module never contacts nodes or stages messages.
 * @param {Record<string, string>} words @param {string} screen */
export const installForwardFixture = (words, screen) => {
  const selection = createSelection(screen);
  const state = createCollection(selection, screen);
  const collectionWords = Object.fromEntries(
    ['search', 'previous', 'next', 'refresh', 'failed', 'empty', 'page'].map((key) => [
      key,
      words[`settings.collection.${key}`],
    ]),
  );
  const render = (value) => {
    if (selection.current()) renderCollectionView(value, collectionWords);
  };
  bindActions(selection, state, screen, render);
  bindMenu(selection, screen);
  render(state);
  selection.selected.textContent = selection.chosen;
  selection.name.textContent = `Robin ${Number.parseInt(selection.chosen, 16)}`;
  showDestination(selection, words, screen);
  return { state, choose: selection.choose, current: selection.current, retire: selection.retire };
};
