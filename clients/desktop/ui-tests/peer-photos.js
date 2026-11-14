// Peer photo smoke phases, retaining download policy, retries and gallery assertions.
(() => {
  const createBytes = async () => {
    const source = document.createElement('canvas');
    source.width = 4;
    source.height = 4;
    source.getContext('2d').fillRect(0, 0, 4, 4);
    const jpeg = await new Promise((resolve) => {
      source.toBlob(resolve, 'image/jpeg');
    });
    return jpeg.arrayBuffer();
  };
  const createCall = (fixture) => {
    const { fetches, held, refused, chatRows, bytes } = fixture;
    const handlers = {
      fetch_peer_photos: (args) => {
        fetches.push(args);
        for (const hash of args.hashes) if (!refused.has(hash[0])) held.add(hash[0]);
        return Promise.resolve();
      },
      peer_photo: ({ hash }) =>
        held.has(hash[0]) ? Promise.resolve(bytes) : Promise.reject('notFound'),
      peer_photo_auto: () => Promise.resolve(fixture.auto),
      chat_rows: () => Promise.resolve(chatRows),
      contact_names: () =>
        Promise.resolve(
          chatRows.map(({ peerId }, index) => ({
            peerId,
            name: '',
            verified: null,
            photo: index ? [] : [9],
          })),
        ),
    };
    return (command, args) =>
      Object.hasOwn(handlers, command) ? handlers[command](args) : Promise.reject('unsupported');
  };
  const createFixture = async () => {
    const bytes = await createBytes();
    const fetches = [];
    const held = new Set();
    const refused = new Set();
    const chatRows = ['ab', 'cd'].map((byte) => ({
      peerId: byte.repeat(32),
      state: 'open',
      lastMessage: null,
      unreadCount: 0,
      activeRecently: false,
    }));
    const fixture = { auto: false, bytes, fetches, held, refused, chatRows };
    const call = createCall(fixture);
    const person = (byte, photos, fullPhotos) => ({
      peerId: byte.repeat(32),
      version: 1,
      place: '',
      fullPhotos,
      draft: { photos, gender: 0, birthYear: null, birthMonth: null, birthDay: null },
    });
    const { strings } = appApi;
    const found = person('ab', [[1], [2]], [[3], [4]]);
    const card = appBindings.resultCard(found, 'peer-photo-card');
    const avatar = card.querySelector('.avatar');
    const letters = avatar.textContent;

    return Object.assign(fixture, {
      call,
      person,
      strings,
      found,
      card,
      avatar,
      letters,
    });
  };
  const checkCardPhotos = async (fixture) => {
    const { refused, found, card, call, avatar, letters, fetches } = fixture;
    refused.add(1);
    await appApi.showCardPhotos([found], [card], call);
    if (avatar.querySelector('canvas') || avatar.textContent !== letters) {
      throw new Error('a refused thumbnail did not leave the letters');
    }
    refused.clear();
    await appApi.showCardPhotos([found], [card], call);
    if (!avatar.querySelector('canvas'))
      throw new Error('the next view did not retry the thumbnail');
    if (fetches.some(({ hashes, full }) => full || String(hashes) !== '1')) {
      throw new Error('a list asked for more than the first thumbnail');
    }
  };
  const checkChatPhotos = async (fixture) => {
    const { call, chatRows } = fixture;
    await openArea('chats-area');
    await appApi.chatsDone;
    await appApi.showChats(call);
    await appApi.chatPhotosShown;
    const [shownRow, plainRow] = chatRows.map(({ peerId }) => liveRow('chat-list', peerId));
    if (
      !shownRow.querySelector('.avatar canvas') ||
      plainRow.querySelector('canvas') ||
      !plainRow.querySelector('.avatar').textContent
    ) {
      throw new Error('a chat row did not show its photo, or letters without one');
    }
  };
  const checkMeteredPhotos = async (fixture) => {
    const { card, fetches, held, refused, found, call, strings } = fixture;
    await openArea('search-area');
    byId('search-results').append(card);
    const main = document.querySelector('#view-photos .main');
    const thumbs = document.querySelectorAll.bind(document, '#view-photos .thumbs button');
    const load = byId('view-photo-load');
    fetches.length = 0;
    held.clear();
    refused.add(3);
    appBindings.openFound(found, card, call);
    await appApi.peerPhotosShown;
    if (fetches.some(({ full }) => full) || !main.querySelector('canvas') || load.hidden) {
      throw new Error(
        'on a metered network the view did not wait for a tap with the thumbnail shown',
      );
    }
    if (
      thumbs().length !== 2 ||
      !thumbs()[0].hasAttribute('aria-current') ||
      main.getAttribute('aria-label') !==
        strings['profile.photoLabel'].replace('{n}', '1').replace('{count}', '2')
    ) {
      throw new Error('the gallery did not name the photo or mark it current');
    }
    Object.assign(fixture, { main, thumbs, load });
  };
  const checkFullPhotoRetry = async (fixture) => {
    const { refused, load, main, fetches, thumbs, strings } = fixture;
    load.click();
    await appApi.peerPhotoLoaded;
    if (
      byId('view-status').textContent !== strings['view.photoFailed'] ||
      load.hidden ||
      load.disabled ||
      document.activeElement !== load
    ) {
      throw new Error('a failed full photo did not say so and offer the button again');
    }
    refused.clear();
    load.click();
    await appApi.peerPhotoLoaded;
    if (
      !load.hidden ||
      document.activeElement !== main ||
      !fetches.at(-1).full ||
      String(fetches.at(-1).hashes) !== '3'
    ) {
      throw new Error('a tap did not load the full photo and move the focus to it');
    }
    await smoke.shot('peer-photos');
    thumbs()[1].click();
    await appApi.peerPhotoChosen;
    if (
      load.hidden ||
      thumbs()[0].hasAttribute('aria-current') ||
      !thumbs()[1].hasAttribute('aria-current')
    ) {
      throw new Error('the second photo was not chosen or waited for no tap');
    }
  };
  const checkAutomaticPhotos = async (fixture) => {
    const { held, refused, person, card, call, main, load, found, fetches } = fixture;
    fixture.auto = true;
    held.clear();
    refused.add(5);
    const missing = person('cd', [[5]], [[6]]);
    appBindings.openFound(missing, card, call);
    if (main.querySelector('canvas')) throw new Error('the last profile stayed under the new name');
    await appApi.peerPhotosShown;
    if (!load.hidden || !held.has(6)) throw new Error('a free network did not load the full photo');
    refused.clear();
    held.delete(6);
    refused.add(6);
    await appApi.showFoundPhotos(missing, call);
    if (!main.querySelector('canvas') || !load.hidden) {
      throw new Error('the next view did not retry, or a free network offered a tap');
    }
    refused.clear();
    await appApi.showFoundPhotos(found, call);
    if (fetches.at(-1).hashes[0][0] !== 3 || !fetches.at(-1).full) {
      throw new Error('a free network did not fetch the full photo without a tap');
    }
  };
  const checkThumbnailNames = async (fixture) => {
    const { person, call, strings } = fixture;
    await appApi.showFoundPhotos(person('ef', [[1], [2]], [[], []]), call);
    const [fallback, photo] = document.querySelectorAll('#view-photos .thumbs button');
    const label = (n) => strings['profile.photoLabel'].replace('{n}', n).replace('{count}', '2');
    if (
      !fallback.textContent ||
      fallback.querySelector('canvas') ||
      fallback.getAttribute('aria-label') !== `${fallback.textContent}: ${label('1')}` ||
      !photo.querySelector('canvas') ||
      photo.textContent ||
      photo.getAttribute('aria-label') !== label('2')
    ) {
      throw new Error('Thumbnail names lost visible letters or retained hidden ones');
    }
  };
  const checkMissingPhoto = async (fixture) => {
    const { held, refused, person, call, main, card } = fixture;
    held.clear();
    refused.add(1);
    await appApi.showFoundPhotos(person('ef', [[1]], [[]]), call);
    if (
      main.querySelector('canvas') ||
      main.getAttribute('aria-hidden') !== 'true' ||
      !main.textContent ||
      document.querySelectorAll('#view-photos .thumbs button').length
    ) {
      throw new Error('a missing photo did not show the letters');
    }
    await checkThumbnailNames(fixture);
    byId('view-back').click();
    card.remove();
  };
  smoke.uiSmokePeerPhotos = async () => {
    const fixture = await createFixture();
    await checkCardPhotos(fixture);
    await checkChatPhotos(fixture);
    await checkMeteredPhotos(fixture);
    await checkFullPhotoRetry(fixture);
    await checkAutomaticPhotos(fixture);
    await checkMissingPhoto(fixture);
  };
})();
