(() => {
  const byId = (id) => document.getElementById(id);
  const check = (value, reason) => {
    if (!value) throw new Error(reason);
  };
  const emptyPage = {
    total: 0,
    matched: 0,
    number: 0,
    pages: 0,
    entries: [],
    previous: null,
    next: null,
  };
  const fixture = () => {
    const peer = '9'.repeat(64);
    const calls = [];
    const cursor = { query: '', offset: 37 };
    const reply = {
      total: 71,
      matched: 71,
      number: 1,
      pages: 3,
      entries: [{ target: peer, label: '2024-02-29' }],
      previous: null,
      next: cursor,
    };
    let read = () => Promise.resolve(reply);
    const call = (command, args) => {
      calls.push([command, args]);
      if (command === 'received_profile_views') return read();
      if (command === 'shared_profile')
        return Promise.resolve({ peerId: args.peerId, draft: { attributes: {} } });
      if (command === 'prepare_profile_view') return Promise.resolve('received-opening');
      if (command === 'peer_likes')
        return Promise.resolve({ ours: false, matched: false, passed: false });
      if (command === 'contact_note') return Promise.resolve('');
      return Promise.resolve(false);
    };
    return {
      peer,
      calls,
      cursor,
      reply,
      call,
      setRead: (value) => {
        read = value;
      },
    };
  };
  const query = async (value) => {
    byId('received-views-query').value = value;
    byId('received-views-query').dispatchEvent(new Event('input'));
    await appApi.receivedViews.viewsShown;
  };
  const checkReceivedViews = async () => {
    await openArea('settings', 'views');
    const test = fixture();
    await appApi.receivedViews.showReceivedViews(test.call);
    check(
      test.calls.every(([command]) => command === 'received_profile_views'),
      'list fetched a profile or sent a view',
    );
    const date = byId('received-views-rows').textContent;
    check(
      date.includes('2024-02-29') && date.includes(test.peer),
      'core date or full target was lost',
    );
    byId('received-views-next').click();
    await appApi.receivedViews.viewsShown;
    check(test.calls.at(-1)[1].cursor === test.cursor, 'UI calculated its own page offset');
    await query('exact-target-query');
    check(
      test.calls.at(-1)[1].query === 'exact-target-query' && test.calls.at(-1)[1].cursor === null,
      'search did not reset the core cursor',
    );
    test.setRead(() => Promise.reject(new Error('controlled read failure')));
    byId('received-views-refresh').click();
    await appApi.receivedViews.viewsShown;
    check(
      byId('received-views-rows').textContent.includes(test.peer),
      'failed refresh discarded known rows',
    );
    check(
      byId('received-views-status').textContent === appApi.strings['settings.collection.failed'],
      'failed read was hidden',
    );
    test.setRead(() =>
      Promise.resolve({
        total: 0,
        matched: 0,
        number: 0,
        pages: 0,
        entries: [],
        previous: null,
        next: null,
      }),
    );
    byId('received-views-refresh').click();
    await appApi.receivedViews.viewsShown;
    check(!byId('received-views-rows').children.length, 'explicit empty retry retained stale rows');
  };
  const checkReceivedViewOpening = async (afterReturn = async () => {}, nativeKeys = false) => {
    await openArea('settings', 'views');
    const test = fixture();
    await appApi.receivedViews.showReceivedViews(test.call);
    await query('retained-query');
    const row = byId('received-views-rows').querySelector('button');
    if (nativeKeys) {
      await documentFocused();
      byId('received-views-query').focus();
      await pressKey('Tab', 'focusin', row);
    }
    await activate(row, nativeKeys);
    await appApi.receivedViews.viewOpened;
    await appApi.profileViewDone;
    checkViewSubmission(test.calls);
    const back = byId('view-back');
    if (nativeKeys) back.focus();
    await activate(back, nativeKeys);
    await appApi.receivedViews.viewsReturned;
    check(
      !byId('settings-views').hidden && byId('received-views-query').value === 'retained-query',
      'Back lost the collection query',
    );
    check(
      document.activeElement?.dataset.peerId === test.peer,
      'Back did not focus the current row',
    );
    await afterReturn();
    await checkRetiredReceivedView(test);
  };
  const checkViewSubmission = (calls) => {
    check(
      calls.filter(([command]) => command === 'shared_profile').length === 1,
      'Open fetched another profile',
    );
    check(
      calls.filter(([command]) => command === 'submit_profile_view').length === 1,
      'explicit opening did not submit once',
    );
  };
  const checkRetiredReceivedView = async (test) => {
    const stale = byId('received-views-rows').querySelector('button');
    const held = smoke.pendingSmokeCall();
    test.setRead(held.call);
    byId('received-views-refresh').click();
    const done = appApi.receivedViews.viewsShown;
    await openArea('profile-area');
    held.resolve(emptyPage);
    await done;
    const before = test.calls.length;
    stale.click();
    check(test.calls.length === before, 'retired row admitted a profile fetch');
    check(
      byId('received-views-rows').textContent.includes(test.peer),
      'hidden completion replaced known rows',
    );
    appBindings.closeFound();
    byId('received-views-query').value = '';
    await appApi.receivedViews.showReceivedViews(invokeTest);
  };
  const checkHiddenReceivedView = async () => {
    await openArea('settings', 'views');
    const test = fixture();
    await appApi.receivedViews.showReceivedViews(test.call);
    const shell = document.getElementById('identity');
    const wasHidden = shell.hidden;
    const button = byId('received-views-rows').querySelector('button');
    const before = test.calls.length;
    try {
      shell.hidden = true;
      button.click();
      await appApi.receivedViews.viewOpened;
      check(test.calls.length === before, 'hidden ancestor admitted a received-row profile fetch');
    } finally {
      shell.hidden = wasHidden;
      await openArea('profile-area');
      await appApi.receivedViews.showReceivedViews(invokeTest);
    }
  };
  const withCaptureAppearance = async (theme, scale, show) => {
    check(['light', 'dark'].includes(theme), 'unknown capture theme');
    check(['100', '200'].includes(scale), 'unknown capture text scale');
    const root = document.documentElement;
    const originalTheme = root.dataset.theme;
    /** @type {[HTMLElement, string, number][]} */
    const originals = [root, document.body].map((element) => [
      element,
      element.style.fontSize,
      parseFloat(getComputedStyle(element).fontSize),
    ]);
    try {
      root.dataset.theme = theme;
      for (const [element, , pixels] of originals)
        element.style.fontSize = `${(pixels * Number(scale)) / 100}px`;
      await document.fonts.ready;
      await frames();
      for (const [element, , pixels] of originals)
        check(
          parseFloat(getComputedStyle(element).fontSize) === (pixels * Number(scale)) / 100,
          'capture text scale was not applied',
        );
      await show();
    } finally {
      for (const [element, size] of originals) element.style.fontSize = size;
      if (originalTheme) root.dataset.theme = originalTheme;
      else delete root.dataset.theme;
    }
  };
  smoke.withCaptureAppearance = withCaptureAppearance;
  const transition = (name, action) => smoke.timedChecks({ [name]: action })[name]();
  const pressKey = async (key, eventType, target) => {
    const listener = new AbortController();
    try {
      const accepted = new Promise((resolve) => {
        target.addEventListener(
          eventType,
          (event) => {
            if (event.isTrusted) resolve(undefined);
          },
          { signal: listener.signal },
        );
      });
      await transition(`received-views-key-${key}-${target.id || target.dataset.peerId}`, () =>
        Promise.all([accepted, invokeTest('ui_test_key', { key })]),
      );
    } finally {
      listener.abort();
    }
  };
  const activate = async (button, nativeKeys) => {
    if (nativeKeys) await pressKey('Return', 'click', button);
    else button.click();
  };
  const captureLoading = async (shot) => {
    const held = smoke.pendingSmokeCall();
    const loading = fixture();
    loading.setRead(held.call);
    const completion = appApi.receivedViews.showReceivedViews(loading.call);
    try {
      check(byId('received-views-refresh').disabled, 'loading did not disable refresh');
      await shot('loading');
    } finally {
      held.resolve(loading.reply);
      await transition('received-views-loading-completed', () => completion);
    }
    await shot('populated');
    loading.setRead(() => Promise.reject(new Error('controlled capture refresh failure')));
    byId('received-views-refresh').click();
    await transition('received-views-refresh-completed', () => appApi.receivedViews.viewsShown);
    check(byId('received-views-rows').textContent.includes(loading.peer), 'retained row lost');
    await shot('retained-error');
  };
  const captureEmptyStates = async (shot) => {
    const expose = async (state, response) => {
      const test = fixture();
      test.setRead(() => response);
      await transition(`received-views-${state}-ready`, () =>
        appApi.receivedViews.showReceivedViews(test.call),
      );
      await shot(state);
      check(
        test.calls.every(([command]) => command === 'received_profile_views'),
        'capture list read fetched a profile or sent a view',
      );
    };
    await expose('initial-error', Promise.reject(new Error('controlled capture read failure')));
    await expose('empty', Promise.resolve(emptyPage));
    byId('received-views-query').value = 'missing-target';
    await expose('no-query-match', Promise.resolve({ ...emptyPage, total: 71 }));
  };
  const captureNextPage = async (shot, nativeKeys) => {
    byId('received-views-query').value = '';
    const page = fixture();
    await transition('received-views-page-ready', () =>
      appApi.receivedViews.showReceivedViews(page.call),
    );
    page.setRead(() =>
      Promise.resolve({
        ...page.reply,
        number: 2,
        previous: page.cursor,
        next: null,
      }),
    );
    const next = byId('received-views-next');
    if (nativeKeys) {
      await documentFocused();
      byId('received-views-rows').querySelector('button').focus();
      await pressKey('Tab', 'focusin', next);
    }
    await activate(next, nativeKeys);
    await transition('received-views-next-completed', () => appApi.receivedViews.viewsShown);
    check(page.calls.at(-1)[1].cursor === page.cursor, 'capture lost the core cursor');
    await shot('page-transition');
  };
  smoke.checkReceivedViewsCapture = (theme, scale) =>
    withCaptureAppearance(theme, scale, async () => {
      const nativeKeys = navigator.platform.startsWith('Linux');
      const textMode = scale === '100' ? 'normal' : 'large';
      const shot = (state) => {
        const name = `received-views-${state}-${theme}-${textMode}`;
        return transition(name, () => smoke.shot(name));
      };
      try {
        await openArea('settings', 'views');
        byId('received-views-query').value = '';
        await captureLoading(shot);
        await captureEmptyStates(shot);
        await captureNextPage(shot, nativeKeys);
        await transition('received-views-profile-return-completed', () =>
          checkReceivedViewOpening(() => shot('profile-return'), nativeKeys),
        );
      } finally {
        byId('received-views-query').value = '';
        await openArea('profile-area');
        await appApi.receivedViews.showReceivedViews(invokeTest);
      }
    });
  smoke.uiSmokeReceivedViews = {
    checkReceivedViews,
    checkReceivedViewOpening,
    checkHiddenReceivedView,
  };
})();
