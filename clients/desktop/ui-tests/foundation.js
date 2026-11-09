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
    checkCanvas(pixels);
    const photos = await import(new URL('chat-photos.js', document.baseURI).href);
    const bytes = await checkPhotoBytes(photos.drawPhoto);
    const peers = await import(new URL('peer-photos.js', document.baseURI).href);
    await checkPeerPhotos(peers.controller, bytes);
    const rows = await import(new URL('chat-row.js', document.baseURI).href);
    checkChatRow(rows.chatRow, localization.strings);
  };
})();
