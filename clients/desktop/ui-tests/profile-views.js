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
  smoke.uiSmokeProfileViews = { checkProfileViews };
})();
