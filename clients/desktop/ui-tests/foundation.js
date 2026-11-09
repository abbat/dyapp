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
    const rows = await import(new URL('chat-row.js', document.baseURI).href);
    checkChatRow(rows.chatRow, localization.strings);
  };
})();
