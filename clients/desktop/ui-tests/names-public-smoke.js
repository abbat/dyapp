// Scoped fixture/assertion helpers and shared smoke scenarios.
{
  // The public name: the editor, the publish list and how a second client's card and view show it.
  const publicNameFixture = () => {
    const peer = 'd'.repeat(64);
    let verified = false;
    const found = {
      peerId: peer,
      version: 1,
      place: '',
      draft: { name: 'Anna', birthYear: null, birthMonth: null, birthDay: null, country: '' },
    };
    const call = (command, args) => {
      if (command === 'search_profiles') {
        return Promise.resolve({ profiles: [found], offered: [], partial: false, failures: [] });
      }
      if (command === 'contact_names') {
        return Promise.resolve(args.peerIds.map((peerId) => ({ peerId, name: 'Anna', verified })));
      }
      if (command === 'confirm_contact') {
        verified = true;
        return Promise.resolve(found);
      }
      if (command === 'unverify_contact') {
        verified = false;
        return Promise.resolve();
      }
      return Promise.resolve(0);
    };
    return { peer, found, call };
  };
  const assertPublicNameTheViewLostTheNameOrThe = (title, peerShort) => {
    if (
      !title.textContent.startsWith('Anna') ||
      !title.textContent.endsWith(peerShort) ||
      document.getElementById('view-peer').textContent !== peerShort
    ) {
      throw new Error(`the view lost the name or the Peer ID: ${title.textContent}`);
    }
  };
  const checkPublicNameEditor = async () => {
    const input = document.getElementById('profile-name');
    const before = input.value;
    input.value = 'Anna';
    if (appBindings.profileDraft().name !== 'Anna')
      throw new Error('the draft lost the public name');
    input.value = before;
    await smoke.shot('profile-public-name');
    const fields = appBindings.publicFields({
      name: 'Anna',
      birthYear: null,
      birthMonth: null,
      birthDay: null,
    });
    if (fields[0] !== `${appApi.strings['profile.name']}: Anna`) {
      throw new Error('the publish list left out the public name');
    }
    let refused = '';
    await appBindings
      .saveProfile(() => Promise.reject('invalidName'))
      .catch((error) => {
        refused = error;
      });
    if (refused !== appApi.strings['profile.nameInvalid']) {
      throw new Error(`a refused public name was not explained: ${refused}`);
    }
    await appBindings
      .saveProfile(() => Promise.reject('invalidHeight'))
      .catch((error) => {
        refused = error;
      });
    if (refused !== appApi.strings['profile.heightInvalid']) {
      throw new Error(`a refused height was not explained: ${refused}`);
    }
    await appBindings
      .saveProfile(() => Promise.reject('invalidAbout'))
      .catch((error) => {
        refused = error;
      });
    if (refused !== appApi.strings['profile.aboutInvalid']) {
      throw new Error(`a refused about was not explained: ${refused}`);
    }
  };
  const checkPublicName = async () => {
    await openArea('profile-area');
    await checkPublicNameEditor();
    await openArea('search-area');
    const { peer, found, call } = publicNameFixture();
    await appBindings.runSearch(call);
    const card = liveRow('search-results', peer);
    const marked = (mark) => card.textContent.includes(`Anna · ${appApi.strings[mark]}`);
    if (!marked('peer.unverified')) throw new Error(`no unverified card: ${card.textContent}`);
    if (card.querySelector('.avatar').textContent !== appBindings.shortPeerId(peer).slice(1, 3)) {
      throw new Error('the avatar left the short Peer ID');
    }
    await smoke.shot('search-public-name');
    appBindings.openFound(found, card, call);
    const title = document.getElementById('view-title');
    const peerShort = appBindings.shortPeerId(peer);
    assertPublicNameTheViewLostTheNameOrThe(title, peerShort);
    await appBindings.confirmContact(call);
    if (!marked('peer.verified')) throw new Error('the card did not follow the mark');
    await appBindings.removeMark(call);
    if (!marked('peer.unverified')) throw new Error('the card did not follow the removed mark');
    document.getElementById('view-back').click();
    appBindings.clearSearch();
    document.dispatchEvent(new Event('dyapp:identity-change'));
  };
  // Mute: the action and the row follow the stored flag, and a refusal leaves both alone.
  const chatMuteFixture = () => {
    const fixture = {};
    const peer = 'd'.repeat(64);
    fixture.muted = false;
    fixture.refuse = null;
    const call = (command, args) => {
      if (command === 'chat_rows') {
        return Promise.resolve([
          {
            peerId: peer,
            state: 'open',
            lastMessage: null,
            unreadCount: 1,
            activeRecently: false,
            muted: fixture.muted,
          },
        ]);
      }
      if (command === 'chat_history') {
        return Promise.resolve({
          state: 'open',
          messages: [],
          activeRecently: false,
          muted: fixture.muted,
        });
      }
      if (command === 'contact_names') {
        return Promise.resolve([{ peerId: peer, name: '', verified: null }]);
      }
      if (command === 'set_muted') {
        if (fixture.refuse) return Promise.reject(fixture.refuse);
        ({ muted: fixture.muted } = args);
      }
      return Promise.resolve(0);
    };
    return Object.assign(fixture, { peer, call });
  };
  const checkChatMute = async () => {
    await openArea('chats-area');
    await appApi.chatsDone;
    const fixture = chatMuteFixture();
    const { peer, call } = fixture;
    await appApi.showChats(call);
    await appBindings.openChat(peer, call);
    const button = document.getElementById('chat-mute');
    const shows = (key, mark) =>
      button.textContent === appApi.strings[key] &&
      liveRow('chat-list', peer).textContent.includes(appApi.strings['chats.muted']) === mark;
    if (!shows('chats.mute', false)) throw new Error('an unmuted chat looks muted');
    await appBindings.muteChat(call);
    if (!fixture.muted || !shows('chats.unmute', true)) throw new Error('Mute did not show');
    fixture.refuse = 'disk full';
    await appBindings.muteChat(call);
    if (
      !fixture.muted ||
      !shows('chats.unmute', true) ||
      !appBindings.chatComposeStatus.textContent.includes(fixture.refuse)
    ) {
      throw new Error('a refused unmute did not explain itself');
    }
    fixture.refuse = null;
    await appBindings.muteChat(call);
    if (fixture.muted || !shows('chats.mute', false)) throw new Error('Unmute did not show');
    document.getElementById('chat-back').click();
    document.dispatchEvent(new Event('dyapp:identity-change'));
  };
  // A shared profile link renders as a card that opens the page; Share sends one to an open chat.
  const profileShareFixture = () => {
    const chatPeer = '7'.repeat(64);
    const sharedPeer = '8'.repeat(64);
    const requestPeer = '9'.repeat(64);
    const link = {
      author: chatPeer,
      groupId: [4],
      id: [5],
      text: `dyapp-profile:1:${sharedPeer}`,
      sharedPeer,
      forwarded: false,
      deleted: false,
      edited: false,
      sentAtMs: null,
    };
    const rows = [
      { peerId: chatPeer, state: 'open', lastMessage: link, unreadCount: 0 },
      { peerId: sharedPeer, state: 'open', lastMessage: null, unreadCount: 0 },
      { peerId: requestPeer, state: 'requestReceived', lastMessage: null, unreadCount: 0 },
    ];
    const found = {
      peerId: sharedPeer,
      draft: { birthYear: null, birthMonth: null, birthDay: null },
    };
    const fixture = {
      chatPeer,
      sharedPeer,
      rows,
      link,
      found,
      shareError: null,
      fetchError: null,
      sharedArgs: undefined,
    };
    fixture.call = profileShareCall(fixture);
    return fixture;
  };

  const readSharedProfile = (fixture, args) => {
    if (args.peerId !== fixture.sharedPeer) throw new Error('card fetched another peer');
    return fixture.fetchError ? Promise.reject(fixture.fetchError) : Promise.resolve(fixture.found);
  };
  const profileShareCall = (fixture) => {
    const handlers = {
      chat_history: () => Promise.resolve({ state: 'open', messages: [fixture.link] }),
      mark_chat_read: () => Promise.resolve(0),
      chat_rows: () => Promise.resolve(fixture.rows),
      shared_profile: (args) => readSharedProfile(fixture, args),
      contact_verified: () => Promise.resolve(false),
      peer_likes: () => Promise.resolve({ ours: false, matched: false }),
      share_profile: (args) => {
        fixture.sharedArgs = args;
        return fixture.shareError ? Promise.reject(fixture.shareError) : Promise.resolve([6]);
      },
    };
    return (command, args) => {
      if (typeof command === 'string' && Object.hasOwn(handlers, command)) {
        return handlers[command](args);
      }
      return Promise.reject('notConnected');
    };
  };
  const assertSharePicker = (chatPeer) => {
    if (
      !appBindings.forwardDialog.open ||
      appBindings.forwardSelect.options.length !== 1 ||
      appBindings.forwardSelect.value !== chatPeer ||
      document.getElementById('chat-forward-title').textContent !==
        appApi.strings['chats.shareTitle'] ||
      appBindings.forwardConfirm.textContent !== appApi.strings['chats.share']
    ) {
      throw new Error('Share picker included a request or the shared peer');
    }
  };
  const assertProfileShared = (fixture, sharedPeer, chatPeer) => {
    if (
      fixture.sharedArgs?.peerId !== sharedPeer ||
      fixture.sharedArgs.targetPeerId !== chatPeer ||
      appBindings.forwardDialog.open ||
      appBindings.activeChat?.peerId !== chatPeer ||
      appBindings.chatComposeStatus.textContent !== appApi.strings['chats.shareSent']
    ) {
      throw new Error('Share did not send the profile to the chosen chat');
    }
  };
  const assertProfileShareChatPreviewDidNotNameTheShared = (chatPeer, text) => {
    if (
      !document.querySelector(`#chat-list [data-peer-id="${chatPeer}"]`)?.textContent.includes(text)
    ) {
      throw new Error('chat preview did not name the shared profile');
    }
  };
  const assertProfileShareFailedProfileCardWasSilent = () => {
    if (
      !appBindings.chatComposeStatus.textContent.includes(appApi.strings['chats.profileFailed']) ||
      !appBindings.chatComposeStatus.textContent.includes('unpublished')
    ) {
      throw new Error('failed profile card was silent');
    }
  };
  const assertProfileShareFailedShareClosedThePicker = () => {
    if (
      !appBindings.forwardDialog.open ||
      appBindings.forwardConfirm.disabled ||
      !appBindings.forwardError.textContent.includes(appApi.strings['chats.shareFailed'])
    ) {
      throw new Error('failed Share closed the picker');
    }
  };
  const checkProfileShare = async () => {
    await openArea('chats-area');
    await appApi.chatsDone;
    const fixture = profileShareFixture();
    const { chatPeer, sharedPeer, rows, call } = fixture;
    appBindings.renderChatRows(rows);
    const text = `${appApi.strings['chats.profileCard']} ${appApi.shortPeerId(sharedPeer)}`;
    assertProfileShareChatPreviewDidNotNameTheShared(chatPeer, text);
    await appBindings.openChat(chatPeer, call);
    const card = appBindings.chatMessages.querySelector(`button[data-peer-id="${sharedPeer}"]`);
    if (
      !card ||
      card.textContent !== text ||
      appBindings.chatMessages.textContent.includes('dyapp-profile:')
    ) {
      throw new Error('profile link did not render as a card');
    }
    fixture.fetchError = 'unpublished';
    card.click();
    await appApi.sharedOpened;
    assertProfileShareFailedProfileCardWasSilent();
    fixture.fetchError = 'blocked';
    card.click();
    await appApi.sharedOpened;
    if (
      !appBindings.profileView.hidden ||
      !appBindings.chatComposeStatus.textContent.includes('blocked')
    ) {
      throw new Error('a blocked shared profile opened or failed silently');
    }
    fixture.fetchError = null;
    card.click();
    await appApi.sharedOpened;
    if (appBindings.profileView.hidden || appBindings.viewed?.found.peerId !== sharedPeer) {
      throw new Error('profile card did not open the page');
    }
    if (!document.getElementById('view-share')) throw new Error('profile page has no Share');
    await appBindings.openShare(sharedPeer, call);
    assertSharePicker(chatPeer);
    fixture.shareError = 'nodeFailed';
    await appBindings.shareProfile(call);
    assertProfileShareFailedShareClosedThePicker();
    fixture.shareError = null;
    await appBindings.shareProfile(call);
    assertProfileShared(fixture, sharedPeer, chatPeer);
    appBindings.closeFound();
    document.getElementById('chat-back').click();
  };
  const checkInterests = async () => {
    await openArea('profile-area');
    const input = document.getElementById('profile-interests');
    const before = input.value;
    input.value = '  Музыка  \nJazz, blues\n\n';
    const tags = appBindings.profileDraft().interests;
    if (tags.join('|') !== 'Музыка|Jazz, blues')
      throw new Error('interest lines lost tags or punctuation');
    input.value = Array(33).fill('music').join('\n');
    input.dispatchEvent(new Event('input'));
    if (input.checkValidity()) throw new Error('too many interests passed the form');
    appApi.showProfileInterests(['Музыка', 'Jazz, blues']);
    if (!input.checkValidity()) throw new Error('loaded interests retained an old error');
    const canonical = ['музыка', 'jazz, blues'];
    await appBindings.saveProfile(() =>
      Promise.resolve({
        draft: { ...appBindings.profileDraft(), interests: canonical },
        ages: null,
      }),
    );
    if (input.value !== canonical.join('\n'))
      throw new Error('save did not show canonical interests');
    const fields = appBindings.publicFields({
      ...appBindings.profileDraft(),
      interests: canonical,
    });
    if (!fields.includes(`${appApi.strings['profile.interests']}: ${canonical.join('; ')}`)) {
      throw new Error('public preview lost canonical interests');
    }
    await smoke.shot('profile-interests');
    let refusal;
    await appBindings
      .saveProfile(() => Promise.reject('invalidInterests'))
      .catch((error) => {
        refusal = error;
      });
    if (refusal !== appApi.strings['profile.interestsInvalid'])
      throw new Error('interest error not explained');
    appApi.showProfileInterests([]);
    if (appBindings.profileDraft().interests.length)
      throw new Error('clearing interests retained tags');
    input.value = before;
    await openArea('search-area');
    const search = document.getElementById('search-interest');
    // The core trims the interest and builds its condition from the offered fields.
    appBindings.showSearchFilters([19], { interest: true });
    search.value = ' Музыка ';
    const query = appBindings.searchQuery();
    if (query.interest !== ' Музыка ' || !query.offered?.includes(19))
      throw new Error('interest filter was not sent');
    await smoke.shot('search-interest');
    appBindings.showSearchFilters([], {});
    if (appBindings.searchQuery().interest) {
      throw new Error('unsupported interest filter leaked into query');
    }
    search.value = '';
  };
  smoke.uiSmokeNamesPublic = Object.freeze({
    checkPublicName,
    checkChatMute,
    checkProfileShare,
    checkInterests,
  });
}
