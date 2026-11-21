(() => {
  const deferred = () => {
    const gate = {};
    gate.promise = new Promise((resolve, reject) => Object.assign(gate, { resolve, reject }));
    return gate;
  };
  const beginRefresh = (value, row, gate) => {
    const started = deferred();
    appApi.renderSettingsRows('blocked', [value], row, String, () => {
      started.resolve();
      return gate.promise;
    });
    const refresh = document.getElementById('settings-blocked-refresh');
    refresh.click();
    return { refresh, started: started.promise, done: appApi.settingsCollectionRefreshed };
  };
  const checkRefreshEpoch = async (outcome, row, check) => {
    const oldGate = deferred(),
      newGate = deferred();
    const old = beginRefresh('old', row, oldGate);
    const settled = old.done.then(
      () => check(outcome === 'resolve', 'Unexpected stale refresh success'),
      () => check(outcome === 'reject', 'Unexpected stale refresh failure'),
    );
    let current;
    const ready = smoke.timedChecks({
      collectionOldReloadStarted: () => old.started,
      collectionCurrentReloadStarted: () => current.started,
      collectionStaleReloadCompleted: () => settled,
      collectionCurrentReloadCompleted: () => current.done,
      collectionReloadCleanup: () => Promise.allSettled([settled, current?.done]),
    });
    try {
      await ready.collectionOldReloadStarted();
      appBindings.identityGeneration += 1;
      document.dispatchEvent(new Event('dyapp:identity-change'));
      current = beginRefresh('new', row, newGate);
      await ready.collectionCurrentReloadStarted();
      check(current.refresh.disabled, 'New collection refresh is not pending');
      if (outcome === 'resolve') oldGate.resolve();
      else oldGate.reject(Error('stale reload failure'));
      await ready.collectionStaleReloadCompleted();
      check(current.refresh.disabled, 'stale refresh reenabled new view');
      check(
        document.getElementById('blocked-list').textContent === 'new',
        'Stale refresh changed rows',
      );
      newGate.resolve();
      await ready.collectionCurrentReloadCompleted();
      check(!current.refresh.disabled, 'Current collection refresh stayed disabled');
    } finally {
      oldGate.resolve();
      newGate.resolve();
      await ready.collectionReloadCleanup();
    }
  };
  const checkRefreshOwner = async (row, check) => {
    const generation = appBindings.identityGeneration;
    try {
      openSettings('blocked');
      for (const outcome of ['resolve', 'reject']) await checkRefreshEpoch(outcome, row, check);
    } finally {
      appBindings.identityGeneration = generation;
      document.dispatchEvent(new Event('dyapp:identity-change'));
    }
  };
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
  const checkCollectionSemantics = (list, check) => {
    check(list.closest('table').getAttribute('role') === 'table', 'Collection table role lost');
    check(list.getAttribute('role') === 'rowgroup', 'Collection row group role lost');
    for (const row of list.children) {
      check(row.getAttribute('role') === 'row', 'Collection row role lost');
      for (const cell of row.children) {
        check(cell.getAttribute('role') === 'cell', 'Collection cell role lost');
      }
    }
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
    checkCollectionSemantics(list, check);
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
  const captureModerationHints = async () => {
    for (const name of ['blocked', 'passed']) {
      openSettings(name);
      await smoke.shot(`settings-${name}`);
    }
  };
  smoke.checkSettingsLists = async () => {
    await openArea('settings');
    await Promise.all([appApi.devicesShown, appApi.blockedShown, appApi.passedShown]);
    await captureModerationHints();
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
      await checkRefreshOwner(row, check);
    } finally {
      for (const [card, hidden] of availability) card.hidden = hidden;
      document.dispatchEvent(new Event('dyapp:identity-change'));
    }
  };
})();
