(() => {
  const checkCollectionFocus = (name, list, previous, next, check) => {
    check(
      list.children.length === 1 && next.disabled && document.activeElement === previous,
      `Last ${name} collection page lost focus: ${JSON.stringify({
        rows: list.children.length,
        nextDisabled: next.disabled,
        previousDisabled: previous.disabled,
        active: document.activeElement?.id || document.activeElement?.tagName,
        focused: document.hasFocus(),
        hidden: previous.closest('[hidden]')?.id,
      })}`,
    );
  };
  const checkSettingsCollection = async (name, id, items, row, selected, check) => {
    await documentFocused();
    openSettings(name);
    let refreshes = 0;
    const reload = async () => {
      refreshes += 1;
    };
    appApi.renderSettingsRows(name, items, row, String, reload);
    const list = document.getElementById(id),
      search = document.getElementById(`settings-${name}-search`);
    list.closest('.card').hidden = false; // The fixture supplies data even for unpublished Devices.
    const previous = document.getElementById(`settings-${name}-previous`);
    const next = document.getElementById(`settings-${name}-next`);
    check(
      list.children.length === 32 && previous.disabled && !next.disabled,
      'Collection first page unbounded',
    );
    check(
      document.querySelector(`[data-settings-open="${name}"] .settings-count`).textContent === '65',
      'Collection count is not total',
    );
    next.focus();
    next.click();
    next.click();
    checkCollectionFocus(name, list, previous, next, check);
    list.querySelector('button').click();
    check(selected() === 64, 'Paged action targeted wrong item');
    previous.click();
    check(list.children.length === 32, 'Previous page did not render');
    search.value = 'ENTRY-064';
    search.dispatchEvent(new Event('input', { bubbles: true }));
    check(
      list.children.length === 1 && previous.disabled && next.disabled,
      'Search did not reset page',
    );
    list.querySelector('button').click();
    check(selected() === 64, 'Filtered action targeted wrong item');
    search.value = 'missing';
    search.dispatchEvent(new Event('input', { bubbles: true }));
    check(!list.children.length, 'Empty search retained rows');
    document.getElementById(`settings-${name}-refresh`).click();
    await appApi.settingsCollectionRefreshed;
    check(refreshes === 1, 'Collection Refresh did not invoke loader');
    appApi.renderSettingsRows(name, null, row, String, reload);
    check(
      document.getElementById(`settings-${name}-status`).textContent ===
        appApi.strings['settings.collection.failed'],
      'Read error is not visible on list',
    );
  };
  smoke.checkSettingsLists = async () => {
    await openArea('settings');
    await Promise.all([appApi.devicesShown, appApi.blockedShown, appApi.passedShown]);
    const check = (ok, reason) => {
      if (!ok) throw Error(reason);
    };
    const ids = {
      identities: 'settings-identities',
      devices: 'devices-list',
      blocked: 'blocked-list',
      passed: 'passed-list',
    };
    const items = Array.from(
      { length: 65 },
      (_, index) => `entry-${String(index).padStart(3, '0')}`,
    );
    const availability = Object.values(ids)
      .map((id) => document.getElementById(id).closest('.card'))
      .map((card) => [card, card.hidden]);
    let selected = null;
    const row = (item, index) => {
      const tr = document.createElement('tr'),
        td = document.createElement('td');
      const button = document.createElement('button');
      button.type = 'button';
      button.textContent = item;
      button.addEventListener('click', () => {
        selected = index;
      });
      td.append(button);
      tr.append(td);
      return tr;
    };
    try {
      for (const [name, id] of Object.entries(ids)) {
        await checkSettingsCollection(name, id, items, row, () => selected, check);
      }
    } finally {
      for (const [card, hidden] of availability) card.hidden = hidden;
      document.dispatchEvent(new Event('dyapp:identity-change'));
    }
  };
})();
