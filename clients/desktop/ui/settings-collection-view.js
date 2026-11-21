import { all as domAll, find as domFind } from './dom.js';
/**
 * @typedef {keyof typeof collectionIds} CollectionName
 * @typedef {{ find: () => string, row: (index: number) => HTMLElement }} DeferredRow
 * @typedef {{ name: CollectionName | 'forward', list: HTMLElement, search: HTMLInputElement,
 *   status: HTMLParagraphElement, previous: HTMLButtonElement, next: HTMLButtonElement,
 *   refresh: HTMLButtonElement, items: DeferredRow[] | null, page: number,
 *   reload?: () => void | Promise<unknown> }} CollectionState
 * @typedef {Record<'search' | 'previous' | 'next' | 'refresh' | 'failed' | 'empty' | 'page', string>} Words
 */
export const collectionIds = {
  identities: 'settings-identities',
  devices: 'devices-list',
  blocked: 'blocked-list',
  passed: 'passed-list',
};
/** @param {CollectionState} state */
const activateTable = (state) => {
  const table = state.list.closest('table');
  if (!table) throw new Error('Settings collection requires a table');
  // Activate the responsive component and its table semantics together.
  table.classList.add('collection-table');
  table.setAttribute('role', 'table');
  state.list.setAttribute('role', 'rowgroup');
};
/** @param {CollectionState} state @param {Words} words */
export const renderCollectionView = (state, words) => {
  const term = state.search.value.trim().toLocaleLowerCase();
  const items = (state.items || [])
    .map((item, index) => ({ item, index }))
    .filter(({ item }) => item.find().toLocaleLowerCase().includes(term));
  const pages = Math.max(1, Math.ceil(items.length / 32));
  state.page = Math.min(state.page, pages - 1);
  const rows = items.slice(state.page * 32, (state.page + 1) * 32);
  activateTable(state);
  state.list.replaceChildren(
    ...rows.map(({ item, index }) => {
      const row = item.row(index);
      row.setAttribute('role', 'row');
      for (const cell of row.children) cell.setAttribute('role', 'cell');
      return row;
    }),
  );
  state.previous.disabled = state.page === 0;
  state.next.disabled = state.page === pages - 1;
  state.status.textContent = pageStatus(state, items.length, pages, words);
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
/** @param {CollectionState} state @param {number} count @param {number} pages @param {Words} words */
const pageStatus = (state, count, pages, words) => {
  if (state.items === null) return words.failed;
  if (!count) return words.empty;
  return words.page
    .replace('{page}', String(state.page + 1))
    .replace('{pages}', String(pages))
    .replace('{count}', String(count));
};
/** @param {CollectionName} name @param {HTMLElement} list @param {Words} words */
export const createCollectionView = (name, list, words) => {
  const field = document.createElement('div');
  field.className = 'field';
  const label = document.createElement('label');
  const search = document.createElement('input');
  search.type = 'text';
  search.id = `settings-${name}-search`;
  label.htmlFor = search.id;
  label.textContent = words.search;
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
  /** @type {(keyof Words)[]} */
  const buttonKeys = ['previous', 'next', 'refresh'];
  const buttons = buttonKeys.map((key) => {
    const button = document.createElement('button');
    button.type = 'button';
    button.id = `settings-${name}-${key}`;
    button.className = 'btn';
    button.textContent = words[key];
    actions.append(button);
    return button;
  });
  const pane = list.closest('[data-settings-pane]');
  if (!(pane instanceof HTMLElement)) throw new Error('Settings collection requires a pane');
  pane.dataset.collectionPane = name;
  pane.append(status, actions);
  const [previous, next, refresh] = buttons;
  /** @type {CollectionState} */
  const state = { name, list, search, status, previous, next, refresh, items: [], page: 0 };
  return state;
};
/** @param {CollectionState} state @param {(state: CollectionState) => void} render
 * @param {AbortSignal} [signal] */
export const bindCollectionNavigation = (state, render, signal) => {
  const { previous, next, search } = state;
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
  previous.addEventListener('click', () => move(-1), { signal });
  next.addEventListener('click', () => move(1), { signal });
  search.addEventListener(
    'input',
    () => {
      state.page = 0;
      render(state);
    },
    { signal },
  );
};
