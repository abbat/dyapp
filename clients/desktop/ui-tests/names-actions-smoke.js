// Scoped fixture/assertion helpers and shared smoke scenarios.
{
  const setFavoriteFixture = (fixture, args) => {
    fixture.writes.push(args);
    if (fixture.changeError) return Promise.reject(fixture.changeError);
    if (fixture.hold)
      return new Promise((resolve) => {
        fixture.finish = () => {
          fixture.saved = args.favorite;
          resolve();
        };
      });
    fixture.saved = args.favorite;
    return Promise.resolve();
  };
  const favoriteFixture = () => {
    const card = document.createElement('button');
    document.getElementById('search-results').append(card);
    const found = {
      peerId: '6'.repeat(64),
      draft: { birthYear: null, birthMonth: null, birthDay: null },
    };
    const fixture = {
      saved: false,
      readError: true,
      changeError: null,
      hold: false,
      finish: undefined,
      release: undefined,
      writes: [],
    };
    const call = (command, args) => {
      if (command === 'favorite_state')
        return fixture.readError ? Promise.reject('disk') : Promise.resolve(fixture.saved);
      if (command === 'set_favorite') return setFavoriteFixture(fixture, args);
      if (command === 'like_peer') throw new Error('favorite sent a like');
      if (command === 'peer_likes') return Promise.resolve({ ours: false, matched: false });
      return Promise.resolve(false);
    };
    return Object.assign(fixture, { card, found, call });
  };

  const heldFavoriteRead = (fixture) => (command, args) =>
    command === 'favorite_state'
      ? new Promise((resolve) => {
          fixture.release = resolve;
        })
      : fixture.call(command, args);
  const assertFavoriteChangeRefused = (fixture, button, status) => {
    if (
      fixture.saved ||
      button.disabled ||
      status.textContent !== appApi.strings['favorites.changeFailed']
    ) {
      throw new Error('failed favorite change lost retry or claimed success');
    }
  };
  const checkFavoriteStale = async (fixture, button) => {
    const { card, found, call } = fixture;
    fixture.hold = true;
    const closedWrite = appApi.toggleFavorite();
    appBindings.closeFound();
    fixture.finish();
    await closedWrite;
    if (!document.getElementById('view-favorite').hidden)
      throw new Error('late write reopened favorite controls');
    fixture.saved = false;
    fixture.hold = false;
    appBindings.openFound(found, card, heldFavoriteRead(fixture));
    const stale = appApi.favoriteShown;
    const next = { ...found, peerId: '5'.repeat(64) };
    appBindings.openFound(next, card, call);
    await appApi.favoriteShown;
    fixture.release(true);
    await stale;
    if (button.textContent !== appApi.strings['favorites.add'])
      throw new Error('stale read changed another profile');
    appBindings.openFound(found, card, heldFavoriteRead(fixture));
    const switched = appApi.favoriteShown;
    document.dispatchEvent(new Event('dyapp:identity-change'));
    fixture.release(true);
    await switched;
    if (!document.getElementById('view-favorite').hidden)
      throw new Error('favorite state crossed identity change');
    appBindings.closeFound();
    card.remove();
  };
  const checkFavoriteAction = async () => {
    await openArea('search-area');
    const fixture = favoriteFixture();
    const { card, found, call, writes } = fixture;
    appBindings.openFound(found, card, call);
    await appApi.favoriteShown;
    const button = document.getElementById('view-favorite-toggle');
    const status = document.getElementById('view-favorite-status');
    if (button.textContent !== appApi.strings['favorites.retry'])
      throw new Error('unknown saved state cannot retry');
    await appApi.toggleFavorite();
    if (writes.length) throw new Error('retrying unknown state mutated a favorite');
    fixture.readError = false;
    await appApi.toggleFavorite();
    fixture.changeError = 'disk';
    await appApi.toggleFavorite();
    assertFavoriteChangeRefused(fixture, button, status);
    fixture.changeError = null;
    fixture.hold = true;
    const pending = appApi.toggleFavorite();
    await appApi.toggleFavorite();
    if (writes.length !== 2 || !button.disabled)
      throw new Error('favorite allowed duplicate writes');
    fixture.finish();
    await pending;
    if (!fixture.saved || button.textContent !== appApi.strings['favorites.remove'])
      throw new Error('saved favorite not shown');
    await smoke.shot('profile-favorite-saved');
    fixture.hold = false;
    await appApi.toggleFavorite();
    if (fixture.saved || button.textContent !== appApi.strings['favorites.add'])
      throw new Error('favorite removal failed');

    await checkFavoriteStale(fixture, button);
  };
  const savedProfileFixture = () => {
    const peer = '4'.repeat(64);
    const found = { peerId: peer, draft: { birthYear: null, birthMonth: null, birthDay: null } };
    const fixture = {
      peer,
      found,
      peers: [peer],
      unavailable: true,
      refuseRemove: false,
      fetches: 0,
      releaseProfile: undefined,
      holdProfile: false,
    };
    fixture.call = savedProfileCall(fixture);
    return fixture;
  };

  const readSavedProfile = (fixture) => {
    ++fixture.fetches;
    if (fixture.holdProfile)
      return new Promise((resolve) => {
        fixture.releaseProfile = resolve;
      });
    return fixture.unavailable ? Promise.reject('unpublished') : Promise.resolve(fixture.found);
  };
  const removeSavedProfile = (fixture, args) => {
    if (args.favorite) throw new Error('saved row tried to add instead of remove');
    if (fixture.refuseRemove) return Promise.reject('disk');
    fixture.peers = fixture.peers.filter((id) => id !== args.peerId);
    return Promise.resolve();
  };
  const savedProfileCall = (fixture) => {
    const handlers = {
      favorite_peers: () => Promise.resolve(fixture.peers),
      contact_names: (args) =>
        Promise.resolve(
          args.peerIds.map((peerId) => ({
            peerId,
            name: `Saved locally ${'x'.repeat(50)}`,
            verified: null,
          })),
        ),
      shared_profile: () => readSavedProfile(fixture),
      set_favorite: (args) => removeSavedProfile(fixture, args),
      block_peer: (args) => {
        fixture.peers = fixture.peers.filter((id) => id !== args.peerId);
        return Promise.resolve({ revocation: 'confirmed' });
      },
      favorite_state: (args) => Promise.resolve(fixture.peers.includes(args.peerId)),
      peer_likes: () => Promise.resolve({ ours: false, matched: false }),
      like_peer: () => {
        throw new Error('saved list sent a like');
      },
    };
    return (command, args) =>
      typeof command === 'string' && Object.hasOwn(handlers, command)
        ? handlers[command](args)
        : Promise.resolve(false);
  };
  const heldSavedList = (call) => {
    const held = {};
    held.call = (command, args) =>
      command === 'favorite_peers'
        ? new Promise((resolve) => {
            held.release = resolve;
          })
        : call(command, args);
    return held;
  };
  const assertSavedLocal = (fixture, rows) => {
    if (
      fixture.fetches ||
      rows.children.length !== 1 ||
      !rows.textContent.includes('Saved locally')
    ) {
      throw new Error('saved list fetched profiles or lost local labels');
    }
  };
  const assertSavedRemovalRefused = (rows, status) => {
    if (
      rows.children.length !== 1 ||
      status.textContent !== appApi.strings['favorites.changeFailed']
    ) {
      throw new Error('failed removal changed saved rows');
    }
  };
  const assertSavedEmpty = (fixture, rows, status) => {
    if (
      rows.children.length ||
      fixture.fetches !== 1 ||
      status.textContent !== appApi.strings['favorites.empty']
    ) {
      throw new Error('saved removal fetched a profile or missed empty state');
    }
  };
  const checkSavedProfileNavigation = async (fixture, panel, rows, buttons) => {
    const { peer, found, call } = fixture;
    fixture.peers = [peer];
    fixture.unavailable = false;
    await appApi.showSaved(call);
    buttons()[0].click();
    await appApi.savedOpened;
    if (appBindings.profileView.hidden || appBindings.viewed?.found.peerId !== peer)
      throw new Error('saved row did not open verified profile');
    document.getElementById('view-back').click();
    await appApi.savedShown;
    buttons()[0].click();
    await appApi.savedOpened;
    await appBindings.blockViewed(call);
    await appApi.savedShown;
    if (rows.children.length || !appBindings.profileView.hidden)
      throw new Error('blocked saved row remained visible');
    fixture.peers = [peer];
    await appApi.showSaved(call);
    fixture.holdProfile = true;
    buttons()[0].click();
    const pendingOpen = appApi.savedOpened;
    appBindings.showArea('profile-area');
    fixture.releaseProfile(found);
    await pendingOpen;
    if (document.getElementById('profile-area').hidden)
      throw new Error('late saved profile changed navigation');
    await openArea('search-area');
    panel.open = true;
    const held = heldSavedList(call);
    const pendingList = appApi.showSaved(held.call);
    document.dispatchEvent(new Event('dyapp:identity-change'));
    held.release([peer]);
    await pendingList;
    if (panel.open || rows.children.length) throw new Error('saved list crossed identity change');
  };
  const checkSavedProfiles = async () => {
    await openArea('search-area');
    const fixture = savedProfileFixture();
    const { call } = fixture;
    const panel = document.getElementById('saved-profiles');
    const rows = document.getElementById('saved-profile-rows');
    const status = document.getElementById('saved-profile-status');
    panel.open = true;
    await appApi.showSaved(call);
    await smoke.shot('saved-profiles-local');
    assertSavedLocal(fixture, rows);
    const buttons = () => rows.firstElementChild.querySelectorAll('button');
    buttons()[0].click();
    await appApi.savedOpened;
    if (status.textContent !== appApi.strings['favorites.openFailed'] || buttons()[1].disabled) {
      throw new Error('unavailable saved profile cannot retry or remove');
    }
    fixture.refuseRemove = true;
    buttons()[1].click();
    await appApi.savedRemoved;
    assertSavedRemovalRefused(rows, status);
    fixture.refuseRemove = false;
    buttons()[1].click();
    await appApi.savedRemoved;
    assertSavedEmpty(fixture, rows, status);
    await checkSavedProfileNavigation(fixture, panel, rows, buttons);
  };
  const assertContactNotesUnknownNoteWasEditableOrFailedSilently = (text, writes, status) => {
    if (
      !text.disabled ||
      writes.length ||
      status.textContent !== appApi.strings['notes.loadFailed']
    ) {
      throw new Error('unknown note was editable or failed silently');
    }
  };
  const assertContactNotesFailedNoteSaveLostDraftRetry = (text, save, status) => {
    if (
      text.value !== 'Case\nЛичная заметка' ||
      save.disabled ||
      status.textContent !== appApi.strings['notes.saveFailed']
    )
      throw new Error('failed note save lost draft/retry');
  };
  const assertContactNotesPrivateNoteLeakedIntoPreviewSearch = (saved) => {
    if (
      appBindings.publicFields(appBindings.profileDraft()).join('\n').includes(saved) ||
      JSON.stringify(appBindings.searchQuery()).includes(saved)
    ) {
      throw new Error('private note leaked into preview/search');
    }
  };
  const writeContactNoteFixture = (fixture, writes, args) => {
    writes.push(args);
    if (fixture.writeError) return Promise.reject(fixture.writeError);
    if (fixture.hold)
      return new Promise((resolve) => {
        fixture.finish = () => {
          fixture.saved = args.note;
          resolve();
        };
      });
    fixture.saved = args.note;
    return Promise.resolve();
  };
  const contactNoteFixture = () => {
    const fixture = {};
    const card = document.createElement('button');
    document.getElementById('search-results').append(card);
    const found = {
      peerId: '5'.repeat(64),
      draft: { birthYear: null, birthMonth: null, birthDay: null },
    };
    fixture.saved = '';
    fixture.readError = true;
    fixture.writeError = null;
    fixture.hold = false;
    fixture.finish = undefined;
    const writes = [];
    const call = (command, args) => {
      if (command === 'contact_note')
        return fixture.readError ? Promise.reject('disk') : Promise.resolve(fixture.saved);
      if (command === 'set_contact_note') return writeContactNoteFixture(fixture, writes, args);
      if (['like_peer', 'send_chat_text', 'save_profile', 'publish_profile'].includes(command)) {
        throw new Error('private note reached public/contact command');
      }
      if (command === 'peer_likes') return Promise.resolve({ ours: false, matched: false });
      return Promise.resolve(false);
    };
    const text = document.getElementById('view-note-text');
    const save = document.getElementById('view-note-save');
    const status = document.getElementById('view-note-status');
    const panel = document.getElementById('view-note');
    const edit = (value) => {
      text.value = value;
      text.dispatchEvent(new Event('input'));
    };
    appBindings.openFound(found, card, call);
    return Object.assign(fixture, { card, found, writes, call, text, save, status, panel, edit });
  };
  const checkContactNoteStale = async (fixture) => {
    const { card, found, call, text, panel, edit } = fixture;
    edit('New draft');
    fixture.hold = true;
    const lateWrite = appApi.saveContactNote();
    appBindings.closeFound();
    appBindings.openFound({ ...found, peerId: '4'.repeat(64) }, card, call);
    await appApi.noteShown;
    fixture.finish();
    await lateWrite;
    if (text.value) throw new Error('late write replaced another profile note');
    fixture.hold = false;
    const finishReadCall = smoke.pendingSmokeCall();
    const lateRead = appApi.showContactNote(() => finishReadCall.call());
    document.dispatchEvent(new Event('dyapp:identity-change'));
    finishReadCall.resolve('Other identity secret');
    await lateRead;
    if (!panel.hidden || text.value) throw new Error('late note read crossed identity change');
    appBindings.closeFound();
    appBindings.openFound({ ...found, peerId: appBindings.current.peerId }, card, call);
    await appApi.noteShown;
    if (!panel.hidden) throw new Error('own profile showed contact note');
    appBindings.closeFound();
    card.remove();
  };
  const checkContactNotes = async () => {
    await openArea('search-area');
    const fixture = contactNoteFixture();
    const { card, found, writes, call, text, save, status, edit } = fixture;
    await appApi.noteShown;
    await appApi.saveContactNote();
    assertContactNotesUnknownNoteWasEditableOrFailedSilently(text, writes, status);
    fixture.readError = false;
    document.getElementById('view-note-retry').click();
    await appApi.noteShown;
    edit('Case\nЛичная заметка');
    fixture.writeError = 'disk';
    await appApi.saveContactNote();
    assertContactNotesFailedNoteSaveLostDraftRetry(text, save, status);
    fixture.writeError = null;
    fixture.hold = true;
    const pending = appApi.saveContactNote();
    await appApi.saveContactNote();
    if (writes.length !== 2 || !save.disabled || !text.disabled)
      throw new Error('duplicate note writes');
    fixture.finish();
    await pending;
    fixture.hold = false;
    if (fixture.saved !== 'Case\nЛичная заметка') throw new Error('note case/lines changed');
    await smoke.shot('profile-private-note');
    assertContactNotesPrivateNoteLeakedIntoPreviewSearch(fixture.saved);
    appBindings.closeFound();
    appBindings.openFound(found, card, call);
    await appApi.noteShown;
    if (text.value !== fixture.saved) throw new Error('reopened note was lost');
    edit('Unsaved edit');
    fixture.writeError = 'disk';
    await appApi.saveContactNote(true);
    if (text.value !== 'Unsaved edit') throw new Error('failed clear lost unsaved edit');
    fixture.writeError = null;
    await appApi.saveContactNote(true);
    if (fixture.saved || text.value) throw new Error('clear note failed');
    edit('😀'.repeat(2001));
    const count = writes.length;
    await appApi.saveContactNote();
    if (writes.length !== count) throw new Error('over-limit note reached write');
    await checkContactNoteStale(fixture);
  };
  const assertPassActionsPassDuplicatedOrHidBusyState = (writes) => {
    if (
      writes !== 1 ||
      !document.getElementById('view-pass').disabled ||
      appBindings.viewStatus.textContent !== appApi.strings['view.pass.saving']
    )
      throw new Error('pass duplicated or hid busy state');
  };
  const assertPassActionsFailedUndoLostRetryState = (card) => {
    if (
      document.getElementById('view-unpass').disabled ||
      !card.hidden ||
      !appBindings.viewStatus.textContent.includes('disk')
    )
      throw new Error('failed undo lost retry/state');
  };
  const assertPassActionsOldPassChangedReopenedBusyView = (card) => {
    if (
      card.hidden ||
      !document.getElementById('view-pass').disabled ||
      appBindings.viewStatus.textContent !== appApi.strings['view.pass.saving']
    )
      throw new Error('old pass changed reopened busy view');
  };
  const checkPassStale = async (card, found, read, likes, pendingCall, finishCall) => {
    const old = appBindings.sendPass(true, pendingCall);
    appBindings.closeFound();
    appBindings.openFound(found, card, read);
    await Promise.resolve();
    const finishNewCall = smoke.pendingSmokeCall();
    const newer = appBindings.sendPass(false, () => finishNewCall.call());
    finishCall.resolve(likes(true));
    await old;
    assertPassActionsOldPassChangedReopenedBusyView(card);
    finishNewCall.resolve(likes());
    await newer;
    const failOldCall = smoke.pendingSmokeCall();
    const oldFailure = appBindings.sendPass(true, () => failOldCall.call());
    appBindings.closeFound();
    appBindings.openFound(found, card, read);
    await Promise.resolve();
    failOldCall.reject('old error');
    await oldFailure;
    if (appBindings.viewStatus.textContent || document.getElementById('view-pass').disabled)
      throw new Error('late pass error changed reopened view');
    appBindings.closeFound();
    card.remove();
  };
  const checkPassActions = async () => {
    await openArea('search-area');
    const card = document.createElement('button');
    document.getElementById('search-results').append(card);
    const found = {
      peerId: '3'.repeat(64),
      draft: { birthYear: null, birthMonth: null, birthDay: null },
    };
    const likes = (passed = false) => ({ ours: false, matched: false, passed });
    const read = (command) => Promise.resolve(command === 'peer_likes' ? likes() : false);
    appBindings.openFound(found, card, read);
    const finishCall = smoke.pendingSmokeCall();
    let writes = 0;
    const pendingCall = () => {
      writes += 1;
      return finishCall.call();
    };
    const pending = appBindings.sendPass(true, pendingCall);
    await appBindings.sendPass(true, pendingCall);
    assertPassActionsPassDuplicatedOrHidBusyState(writes);
    finishCall.resolve(likes(true));
    await pending;
    if (!card.hidden || document.getElementById('view-unpass').hidden)
      throw new Error('pass not shown');
    await appBindings.sendPass(false, () => Promise.reject('disk'));
    assertPassActionsFailedUndoLostRetryState(card);
    await appBindings.sendPass(false, () => Promise.resolve(likes()));
    if (card.hidden || !document.getElementById('view-unpass').hidden)
      throw new Error('undo not shown');
    await checkPassStale(card, found, read, likes, pendingCall, finishCall);
  };
  const assertProfileActionGuardsOldLikeUnlockedReopenedView = () => {
    if (
      !document.getElementById('view-like').disabled ||
      appBindings.viewStatus.textContent !== appApi.strings['view.sending']
    )
      throw new Error('old like unlocked reopened view');
  };
  const profileActionFixture = () => {
    const card = document.createElement('button');
    document.getElementById('search-results').append(card);
    const found = {
      peerId: '2'.repeat(64),
      draft: { birthYear: null, birthMonth: null, birthDay: null },
    };
    const likes = (ours) => ({ ours, matched: ours, passed: false });
    const read = (command) => Promise.resolve(command === 'peer_likes' ? likes(false) : false);
    const finishReadCall = smoke.pendingSmokeCall();
    const heldRead = (command) =>
      command === 'peer_likes' ? finishReadCall.call() : read(command);
    appBindings.openFound(found, card, heldRead);
    const oldRead = finishReadCall.resolve;
    appBindings.closeFound();
    appBindings.openFound(found, card, read);
    return { card, found, likes, read, finishReadCall, heldRead, oldRead };
  };
  const checkProfileActionGuards = async () => {
    await openArea('search-area');
    const { card, found, likes, read, finishReadCall, heldRead, oldRead } = profileActionFixture();
    await Promise.resolve();
    oldRead(likes(true));
    await Promise.resolve();
    if (!document.getElementById('view-unlike').hidden)
      throw new Error('old likes read changed reopened view');
    appBindings.closeFound();
    appBindings.openFound(found, card, heldRead);
    await appBindings.sendLike(true, () => Promise.resolve(likes(true)));
    finishReadCall.resolve(likes(false));
    await Promise.resolve();
    if (document.getElementById('view-unlike').hidden)
      throw new Error('initial read overwrote local like');
    const finishLikeCall = smoke.pendingSmokeCall();
    let writes = 0;
    const heldLike = () => {
      writes += 1;
      return finishLikeCall.call();
    };
    const pending = appBindings.sendLike(false, heldLike);
    await appBindings.sendLike(true, heldLike);
    await appBindings.sendPass(true, heldLike);
    if (writes !== 1) throw new Error('like/pass allowed overlapping writes');
    appBindings.closeFound();
    appBindings.openFound(found, card, read);
    await Promise.resolve();
    const finishNewCall = smoke.pendingSmokeCall();
    const newer = appBindings.sendLike(true, () => finishNewCall.call());
    finishLikeCall.resolve(likes(false));
    await pending;
    assertProfileActionGuardsOldLikeUnlockedReopenedView();
    finishNewCall.resolve(likes(true));
    await newer;
    if (document.getElementById('view-unlike').hidden) throw new Error('new like not shown');
    appBindings.closeFound();
    card.remove();
  };
  smoke.uiSmokeNamesActions = Object.freeze({
    checkFavoriteAction,
    checkSavedProfiles,
    checkContactNotes,
    checkPassActions,
    checkProfileActionGuards,
  });
}
