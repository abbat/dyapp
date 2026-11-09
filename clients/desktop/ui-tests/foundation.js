(() => {
  const checkCatalogue = async (localization, catalogue) => {
    const before = [localization.locale, localization.pseudoLocale, localization.strings];
    const fetch = window.fetch;
    let rejected = false;
    try {
      window.fetch = async () => new Response(JSON.stringify(catalogue));
      await localization.loadLocale(async () => [false, false]);
    } catch (error) {
      rejected = error instanceof Error && error.message.startsWith('Invalid translation');
    } finally {
      window.fetch = fetch;
    }
    if (
      !rejected ||
      localization.locale !== before[0] ||
      localization.pseudoLocale !== before[1] ||
      localization.strings !== before[2]
    )
      throw new Error('Invalid catalogue accepted or changed locale state');
  };
  const checkInterpolation = (localization) => {
    for (const [key, names] of [
      ['stickers.message', ['emoji', 'name']],
      ['profile.photoLabel', ['n', 'count']],
    ]) {
      for (const name of names) {
        if (!localization.strings[key].includes(`{${name}}`))
          throw new Error(`Catalogue changed interpolation token: ${key} ${name}`);
      }
    }
  };
  const checkCanvas = (pixels) => {
    const getContext = HTMLCanvasElement.prototype.getContext;
    let rejected = false;
    try {
      HTMLCanvasElement.prototype.getContext = () => null;
      pixels.stickerPixels({ width: 1, height: 1, pixels: [0, 0, 0, 255] }, 'fixture');
    } catch (error) {
      rejected = error instanceof Error && error.message === 'Sticker canvas unavailable';
    } finally {
      HTMLCanvasElement.prototype.getContext = getContext;
    }
    if (!rejected) throw new Error('Unavailable sticker canvas was accepted');
  };
  const checkPhotoFailure = async (draw) => {
    const decode = window.createImageBitmap;
    const canvas = document.createElement('canvas');
    let closed = 0;
    let rejected = false;
    try {
      window.createImageBitmap = async () => ({
        close: () => {
          closed += 1;
        },
      });
      canvas.getContext = () => null;
      await draw(canvas, [], 'image/png');
    } catch (error) {
      rejected = error instanceof Error && error.message === 'Photo canvas unavailable';
    } finally {
      window.createImageBitmap = decode;
    }
    if (!rejected || closed !== 1) throw new Error('Photo canvas failure leaked its bitmap');
  };
  const checkPhotoBytes = async (draw) => {
    const source = document.createElement('canvas');
    source.width = 1;
    source.height = 1;
    const context = source.getContext('2d');
    context.fillStyle = 'rgb(10,20,30)';
    context.fillRect(0, 0, 1, 1);
    const blob = await new Promise((resolve) => source.toBlob(resolve, 'image/png'));
    if (!blob) throw new Error('Photo fixture PNG unavailable');
    const bytes = await blob.arrayBuffer();
    for (const value of [bytes, new Uint8Array(bytes), Array.from(new Uint8Array(bytes))]) {
      const canvas = document.createElement('canvas');
      await draw(canvas, value, 'image/png', 1);
      const pixel = canvas.getContext('2d').getImageData(0, 0, 1, 1).data;
      if (canvas.width !== 1 || canvas.height !== 1 || pixel.join() !== '10,20,30,255')
        throw new Error('Photo bytes lost their pixel content');
    }
    await checkPhotoFailure(draw);
    return bytes;
  };
  const checkPeerPhotos = async (controller, bytes) => {
    const cards = [0, 1].map(() => {
      const card = document.createElement('div');
      const avatar = document.createElement('span');
      avatar.className = 'avatar';
      card.append(avatar);
      return card;
    });
    const calls = [];
    await controller.showCardPhotos(
      [{ draft: { photos: [[1]] } }, { draft: { photos: [] } }],
      cards,
      async (command, args) => {
        calls.push([command, args]);
        if (command === 'peer_photo') return bytes;
      },
    );
    const expected = [
      ['fetch_peer_photos', { hashes: [[1]], full: false }],
      ['peer_photo', { hash: [1] }],
    ];
    if (
      JSON.stringify(calls) !== JSON.stringify(expected) ||
      !cards[0].querySelector('canvas') ||
      cards[1].querySelector('canvas')
    )
      throw new Error('Peer thumbnail lost its hash or card ownership');
  };
  const checkUnavailableMedia = async () => {
    const { identityState } = await import(new URL('identity-state.js', document.baseURI).href);
    const picker = await import(new URL('sticker-picker.js', document.baseURI).href);
    const manager = await import(new URL('stickers.js', document.baseURI).href);
    const cache = await import(new URL('media-cache.js', document.baseURI).href);
    const before = identityState.current;
    let calls = 0;
    const call = async () => {
      calls += 1;
    };
    try {
      identityState.current = null;
      await picker.controller.stickerPicker.open({ peerId: 'fixture-peer', call });
      await manager.controller.stickerCollection.open(call);
      await cache.controller.showMediaCache(call);
      if (
        calls ||
        document.getElementById('sticker-picker').open ||
        document.getElementById('sticker-manager').open ||
        !document.getElementById('media-cache-enabled').disabled
      )
        throw new Error('Media operation accepted an unavailable identity');
    } finally {
      identityState.current = before;
    }
  };
  const checkCurrentPhotoRemoval = async (controller, grid, call, writes) => {
    const dialog = document.getElementById('photo-remove-dialog');
    const confirm = document.getElementById('photo-remove-confirm');
    controller.showProfilePhotos([[2]], call);
    grid.querySelector('.remove').click();
    document.getElementById('photo-remove-cancel').click();
    confirm.click();
    await controller.profilePhotoChanged;
    if (writes()) throw new Error('Cancelled photo confirmation removed a photo');
    grid.querySelector('.remove').click();
    const { identityState } = await import(new URL('identity-state.js', document.baseURI).href);
    identityState.generation += 1;
    confirm.click();
    await controller.profilePhotoChanged;
    if (writes()) throw new Error('Photo confirmation ignored its opening generation');
    grid.querySelector('.remove').click();
    dialog.dispatchEvent(new Event('close'));
    if (!dialog.open || dialog.hidden) throw new Error('Old close event hid a new photo dialog');
    confirm.click();
    confirm.click();
    await controller.profilePhotoChanged;
    if (writes() !== 1) throw new Error('Current photo confirmation did not remove exactly once');
  };
  const checkPhotoRemovalOwner = async () => {
    const { identityState } = await import(new URL('identity-state.js', document.baseURI).href);
    const app = await import(new URL('app.js', document.baseURI).href);
    const { controller } = await import(new URL('profile-photos.js', document.baseURI).href);
    const before = {
      current: identityState.current,
      generation: identityState.generation,
      busy: app.profileBusy,
    };
    const grid = document.getElementById('profile-photos');
    let writes = 0;
    const call = async (command) => {
      if (command === 'remove_profile_photo') writes += 1;
      if (command === 'profile_photo') throw new Error('Fixture has no image bytes');
      return { draft: { photos: [] } };
    };
    try {
      identityState.current = {
        status: 'ready',
        peerId: 'a'.repeat(64),
        mode: 'file',
        slot: 'first',
      };
      identityState.generation += 1;
      app.setProfileBusy(false);
      controller.showProfilePhotos([[1]], call);
      await Promise.all([
        controller.ownAvatarsShown,
        ...[...grid.children].map((tile) => tile.ready),
      ]);
      grid.querySelector('.remove').click();
      identityState.current = {
        status: 'ready',
        peerId: 'b'.repeat(64),
        mode: 'file',
        slot: 'second',
      };
      identityState.generation += 1;
      document.dispatchEvent(new Event('dyapp:identity-change'));
      document.getElementById('photo-remove-confirm').click();
      await controller.profilePhotoChanged;
      if (writes) throw new Error('Stale photo confirmation changed another identity');
      await checkCurrentPhotoRemoval(controller, grid, call, () => writes);
    } finally {
      document.getElementById('photo-remove-dialog').close();
      controller.showProfilePhotos([], call);
      app.setProfileBusy(before.busy);
      identityState.current = before.current;
      identityState.generation = before.generation;
    }
  };
  const checkChatRow = (build, strings) => {
    const message = {
      groupId: [],
      author: 'a'.repeat(64),
      id: [],
      text: 'row preview',
      edited: false,
      deleted: false,
      unread: false,
      forwarded: false,
      delivery: null,
      read: false,
      sentAtMs: 1,
      photo: null,
      sticker: null,
      sharedPeer: null,
    };
    let opened = 0;
    const row = build(
      {
        peerId: 'b'.repeat(64),
        state: 'requestReceived',
        lastMessage: message,
        unreadCount: 2,
        activeRecently: true,
        muted: true,
      },
      () => {
        opened += 1;
      },
    );
    if (
      row.dataset.peerId !== 'b'.repeat(64) ||
      !row.textContent.includes(strings['chats.state.requestReceived']) ||
      !row.textContent.includes(strings['chats.active']) ||
      !row.textContent.includes(strings['chats.muted']) ||
      row.querySelector('.caption.ellipsis').textContent !== 'row preview' ||
      row.querySelector('.meta .badge').textContent !== '2' ||
      !row.querySelector('.meta .caption').textContent
    )
      throw new Error('Chat row lost its identity, preview or status metadata');
    row.click();
    if (opened !== 1) throw new Error('Chat row did not retain its opening callback');
  };
  smoke.checkFoundationContracts = async () => {
    const localization = await import(new URL('locale.js', document.baseURI).href);
    const pixels = await import(new URL('sticker-pixels.js', document.baseURI).href);
    for (const catalogue of [null, [], 'text', { invalid: 1 }])
      await checkCatalogue(localization, catalogue);
    checkInterpolation(localization);
    checkCanvas(pixels);
    const photos = await import(new URL('chat-photos.js', document.baseURI).href);
    const bytes = await checkPhotoBytes(photos.drawPhoto);
    const peers = await import(new URL('peer-photos.js', document.baseURI).href);
    await checkPeerPhotos(peers.controller, bytes);
    await checkUnavailableMedia();
    await checkPhotoRemovalOwner();
    const rows = await import(new URL('chat-row.js', document.baseURI).href);
    checkChatRow(rows.chatRow, localization.strings);
  };
})();
