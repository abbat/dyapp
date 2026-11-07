// Local collections are held as data; only one page of rows is mounted.
(() => {
  const states = new Map();
  const ids = { identities: 'settings-identities', devices: 'devices-list', blocked: 'blocked-list', passed: 'passed-list' };
  const text = (key) => window.strings[`settings.collection.${key}`];
  const render = (state) => {
    const term = state.search.value.trim().toLocaleLowerCase();
    const items = (state.items || []).map((item, index) => ({ item, index }))
      .filter(({ item }) => state.find(item).toLocaleLowerCase().includes(term));
    const pages = Math.max(1, Math.ceil(items.length / 32));
    state.page = Math.min(state.page, pages - 1);
    const rows = items.slice(state.page * 32, (state.page + 1) * 32);
    const remove = document.getElementById('settings-remove');
    if (state.name === 'identities' && remove) remove.hidden = true;
    state.list.replaceChildren(...rows.map(({ item, index }) => state.row(item, index)));
    state.previous.disabled = state.page === 0;
    state.next.disabled = state.page === pages - 1;
    state.status.textContent = state.items === null ? text('failed')
      : items.length ? text('page').replace('{page}', state.page + 1).replace('{pages}', pages)
        .replace('{count}', items.length) : text('empty');
    for (const button of document.querySelectorAll(`[data-settings-open="${state.name}"]`)) {
      let count = button.querySelector('.settings-count');
      if (!count) { count = document.createElement('span'); count.className = 'settings-count'; button.append(count); }
      count.textContent = state.items === null ? '?' : String(state.items.length);
    }
  };
  const create = (name) => {
    const list = document.getElementById(ids[name]);
    const field = document.createElement('div');
    field.className = 'field';
    const label = document.createElement('label');
    const search = document.createElement('input');
    search.type = 'text';
    search.id = `settings-${name}-search`;
    label.htmlFor = search.id;
    label.textContent = text('search');
    field.append(label, search);
    list.closest('table').before(field);
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
    list.closest('[data-settings-pane]').append(status, actions);
    const [previous, next, refresh] = buttons;
    const state = { name, list, search, status, previous, next, refresh, items: [], page: 0 };
    const move = (amount) => {
      state.page += amount;
      render(state);
      (amount > 0 && next.disabled ? previous : amount < 0 && previous.disabled ? next
        : amount > 0 ? next : previous).focus();
    };
    previous.addEventListener('click', () => move(-1));
    next.addEventListener('click', () => move(1));
    search.addEventListener('input', () => { state.page = 0; render(state); });
    refresh.addEventListener('click', () => {
      const generation = identityGeneration;
      refresh.disabled = true;
      window.settingsCollectionRefreshed = Promise.resolve().then(() => state.reload()).finally(() => {
        if (identityCurrent(generation)) refresh.disabled = false;
      });
    });
    states.set(name, state);
    return state;
  };
  window.renderSettingsRows = (name, items, row, find, reload) => {
    const state = states.get(name) || create(name);
    Object.assign(state, { items, row, find, reload });
    render(state);
  };
  window.settingsCollectionFailed = (name) => {
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
})();
