(() => {
  const byId = (id) => document.getElementById(id);
  const check = (value, reason) => {
    if (!value) throw new Error(reason);
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
  const checkReceivedViewOpening = async () => {
    await openArea('settings', 'views');
    const test = fixture();
    await appApi.receivedViews.showReceivedViews(test.call);
    await query('retained-query');
    byId('received-views-rows').querySelector('button').click();
    await appApi.receivedViews.viewOpened;
    await appApi.profileViewDone;
    check(
      test.calls.filter(([command]) => command === 'shared_profile').length === 1,
      'Open fetched another profile',
    );
    check(
      test.calls.filter(([command]) => command === 'submit_profile_view').length === 1,
      'explicit opening did not submit once',
    );
    byId('view-back').click();
    await appApi.receivedViews.viewsReturned;
    check(
      !byId('settings-views').hidden && byId('received-views-query').value === 'retained-query',
      'Back lost the collection query',
    );
    check(
      document.activeElement?.dataset.peerId === test.peer,
      'Back did not focus the current row',
    );
    const stale = byId('received-views-rows').querySelector('button');
    const held = smoke.pendingSmokeCall();
    test.setRead(held.call);
    byId('received-views-refresh').click();
    const done = appApi.receivedViews.viewsShown;
    await openArea('profile-area');
    held.resolve({
      total: 0,
      matched: 0,
      number: 0,
      pages: 0,
      entries: [],
      previous: null,
      next: null,
    });
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
  smoke.uiSmokeReceivedViews = {
    checkReceivedViews,
    checkReceivedViewOpening,
    checkHiddenReceivedView,
  };
})();
