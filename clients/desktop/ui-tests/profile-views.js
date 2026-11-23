// Real profile-page integration, with explicit preparation completions instead of delays.
(() => {
  const profileCalls = (sent, prepare) => (command, args) => {
    if (command.endsWith('_profile_view')) {
      sent.push([command, args]);
      return command === 'prepare_profile_view' ? prepare() : Promise.resolve(null);
    }
    if (command === 'peer_likes') return Promise.resolve({ ours: false, matched: false });
    if (command === 'contact_note') return Promise.resolve('');
    return Promise.resolve(false);
  };
  const checkProfileViews = async () => {
    await openArea('search-area');
    const found = { peerId: '9'.repeat(64), draft: { attributes: {} } };
    const card = document.createElement('button');
    appBindings.searchResults.append(card);
    const sent = [];
    let prepare;
    const call = profileCalls(sent, () => prepare());
    try {
      const pending = smoke.pendingSmokeCall();
      prepare = pending.call;
      appBindings.openFound(found, card, call);
      const oldDone = appApi.profileViewDone;
      if (typeof pending.resolve !== 'function')
        throw new Error('profile page did not prepare a view');
      appBindings.closeFound();
      pending.resolve('old-opening');
      await oldDone;
      if (sent.some(([command]) => command === 'submit_profile_view'))
        throw new Error('closed profile admitted a late view');
      if (
        !sent.some(
          ([command, args]) => command === 'retire_profile_view' && args.token === 'old-opening',
        )
      )
        throw new Error('late opening was not retired');

      sent.length = 0;
      prepare = () => Promise.resolve('current-opening');
      appBindings.openFound(found, card, call);
      await appApi.profileViewDone;
      const submitted = sent.filter(([command]) => command === 'submit_profile_view');
      if (submitted.length !== 1 || submitted[0][1].token !== 'current-opening')
        throw new Error('explicit profile did not submit its original opening once');
      appBindings.closeFound();

      sent.length = 0;
      const leaving = smoke.pendingSmokeCall();
      prepare = leaving.call;
      appBindings.openFound(found, card, call);
      const leavingDone = appApi.profileViewDone;
      await openArea('profile-area');
      leaving.resolve('hidden-opening');
      await leavingDone;
      if (sent.some(([command]) => command === 'submit_profile_view'))
        throw new Error('leaving the profile area admitted a view');
    } finally {
      appBindings.closeFound();
      card.remove();
    }
  };
  const checkHiddenProfileView = async () => {
    await openArea('search-area');
    const shell = document.getElementById('identity');
    const card = document.createElement('button');
    appBindings.searchResults.append(card);
    const held = smoke.pendingSmokeCall();
    const sent = [];
    const wasHidden = shell.hidden;
    try {
      appBindings.openFound(
        { peerId: '9'.repeat(64), draft: { attributes: {} } },
        card,
        profileCalls(sent, held.call),
      );
      const done = appApi.profileViewDone;
      shell.hidden = true;
      held.resolve('hidden-ancestor-opening');
      await done;
      if (sent.some(([command]) => command === 'submit_profile_view'))
        throw new Error('hidden identity ancestor admitted a disclosure');
    } finally {
      shell.hidden = wasHidden;
      appBindings.closeFound();
      card.remove();
    }
  };
  const checkRecovery = (condition, reason) => {
    if (!condition) throw new Error(reason);
  };
  const recoveryCardFixture = async () => {
    const { searchRecoveryCard } = await import(
      new URL('search-recovery.js', document.baseURI).href
    );
    const root = document.createElement('section');
    root.id = 'search-recovery-smoke';
    document.getElementById('profile-area').append(root);
    const pending = smoke.pendingSmokeCall();
    const calls = [];
    let completed;
    const completion = new Promise((resolve) => {
      completed = resolve;
    });
    const saved = { version: 8, hidden: true, searchPending: true };
    const call = (command, args) => {
      calls.push([command, args]);
      if (command === 'prepare_search_recovery') return Promise.resolve('original-review');
      if (command === 'confirm_search_recovery') return pending.call();
      return Promise.resolve();
    };
    const card = searchRecoveryCard(
      root,
      '9'.repeat(64),
      call,
      () => true,
      appApi.strings,
      completed,
    );
    return { card, root, calls, pending, saved, completion };
  };
  const checkSearchRecoveryCard = async () => {
    await openArea('profile-area');
    const { card, root, calls, pending, saved, completion } = await recoveryCardFixture();
    const input = document.getElementById('profile-name');
    const previous = input.value;
    input.value = 'unsaved recovery draft';
    try {
      await card.prepare();
      const action = root.querySelector('#search-recovery-smoke-action');
      checkRecovery(
        calls.length === 1 && action.textContent === appApi.strings['searchRecovery.confirm'],
        'Recovery signed without consent or lost its localized confirmation',
      );
      action.click();
      await smoke.waitForSignal('recovery confirmation entry', pending.started);
      action.click();
      if (calls.length !== 2) throw new Error('Busy recovery admitted a duplicate confirmation');
      pending.resolve(saved);
      await smoke.waitForSignal('recovery confirmed receipt rendered', completion);
      if (card.model.state.saved !== saved || input.value !== 'unsaved recovery draft')
        throw new Error('Recovery lost its receipt or overwrote unsaved profile input');
      if (action.textContent !== appApi.strings['searchRecovery.retry'])
        throw new Error('Confirmed pending recovery did not offer delivery retry');
      const captured = action.onclick;
      root.hidden = true;
      captured();
      if (calls.length !== 2) throw new Error('Hidden recovery admitted a captured retry');
      await card.retire();
      captured();
      if (calls.length !== 3 || calls[2][1].token !== 'original-review')
        throw new Error('Recovery retirement did not release only its original token');
    } finally {
      await card.retire();
      input.value = previous;
      root.remove();
    }
  };
  smoke.uiSmokeProfileViews = {
    checkProfileViews,
    checkHiddenProfileView,
    checkSearchRecoveryCard,
  };
})();
