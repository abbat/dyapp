import { all as domAll, byId as domById, find as domFind } from './dom.js';
import { identityState, identityCurrent } from './identity-state.js';
import { settingsRemove } from './elements.js';
import { strings } from './locale.js';
export const controller = {};
/** @type {Promise<unknown> | undefined} */
controller.settingsCollectionRefreshed = undefined;

// Local collections are held as data; only one page of rows is mounted.
/**
 * @typedef {keyof typeof ids} CollectionName
 * @typedef {{ find: () => string, row: (index: number) => HTMLElement }} DeferredRow
 * @typedef {{ name: CollectionName, list: HTMLElement, search: HTMLInputElement,
 *   status: HTMLParagraphElement, previous: HTMLButtonElement, next: HTMLButtonElement,
 *   refresh: HTMLButtonElement, items: DeferredRow[] | null, page: number,
 *   reload?: () => void | Promise<unknown> }} CollectionState
 */
/** @type {Map<CollectionName, CollectionState>} */
const states = new Map();
const ids = {
  identities: 'settings-identities',
  devices: 'devices-list',
  blocked: 'blocked-list',
  passed: 'passed-list',
};
/** @param {string} key */
const text = (key) => strings[`settings.collection.${key}`];
/** @param {CollectionState} state */
const render = (state) => {
  const term = state.search.value.trim().toLocaleLowerCase();
  const items = (state.items || [])
    .map((item, index) => ({ item, index }))
    .filter(({ item }) => item.find().toLocaleLowerCase().includes(term));
  const pages = Math.max(1, Math.ceil(items.length / 32));
  state.page = Math.min(state.page, pages - 1);
  const rows = items.slice(state.page * 32, (state.page + 1) * 32);
  if (state.name === 'identities') settingsRemove.hidden = true;
  state.list.replaceChildren(...rows.map(({ item, index }) => item.row(index)));
  state.previous.disabled = state.page === 0;
  state.next.disabled = state.page === pages - 1;
  state.status.textContent = pageStatus(state, items.length, pages);
  for (const button of domAll(document, `[data-settings-open="${state.name}"]`, HTMLElement)) {
    let count = domFind(button, '.settings-count', HTMLElement);
    if (!count) {
      count = document.createElement('span');
      count.className = 'settings-count';
      button.append(count);
    }
    count.textContent = state.items === null ? '?' : String(state.items.length);
  }
};
/** @param {CollectionState} state @param {number} count @param {number} pages */
const pageStatus = (state, count, pages) => {
  if (state.items === null) return text('failed');
  if (!count) return text('empty');
  return text('page')
    .replace('{page}', String(state.page + 1))
    .replace('{pages}', String(pages))
    .replace('{count}', String(count));
};
/** @param {CollectionName} name */
const create = (name) => {
  const list = domById(ids[name], HTMLElement);
  const field = document.createElement('div');
  field.className = 'field';
  const label = document.createElement('label');
  const search = document.createElement('input');
  search.type = 'text';
  search.id = `settings-${name}-search`;
  label.htmlFor = search.id;
  label.textContent = text('search');
  field.append(label, search);
  const table = list.closest('table');
  if (!table) throw new Error('Settings collection requires a table');
  table.before(field);
  const status = document.createElement('p');
  status.className = 'notice';
  status.id = `settings-${name}-status`;
  status.setAttribute('role', 'status');
  status.setAttribute('aria-live', 'polite');
  const actions = document.createElement('div');
  actions.className = 'actions';
  const buttons = ['previous', 'next', 'refresh'].map((key) => {
    const button = document.createElement('button');
    button.type = 'button';
    button.id = `settings-${name}-${key}`;
    button.className = 'btn';
    button.textContent = text(key);
    actions.append(button);
    return button;
  });
  const pane = list.closest('[data-settings-pane]');
  if (!pane) throw new Error('Settings collection requires a pane');
  pane.append(status, actions);
  const [previous, next, refresh] = buttons;
  /** @type {CollectionState} */
  const state = { name, list, search, status, previous, next, refresh, items: [], page: 0 };
  bindControls(state);
  states.set(name, state);
  return state;
};
/** @param {CollectionState} state */
const bindControls = (state) => {
  const { previous, next, refresh, search } = state;
  /** @param {number} amount */
  const move = (amount) => {
    state.page += amount;
    render(state);
    (amount > 0 && next.disabled
      ? previous
      : amount < 0 && previous.disabled
        ? next
        : amount > 0
          ? next
          : previous
    ).focus();
  };
  previous.addEventListener('click', () => move(-1));
  next.addEventListener('click', () => move(1));
  search.addEventListener('input', () => {
    state.page = 0;
    render(state);
  });
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
