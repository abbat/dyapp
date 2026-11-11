// Chat photo smoke data and deterministic backend responses.
(() => {
  const originalBytes = async () => {
    const source = document.createElement('canvas');
    source.width = 8;
    source.height = 8;
    const paint = source.getContext('2d');
    paint.fillStyle = '#c03030';
    paint.fillRect(0, 0, 8, 8);
    const jpeg = await new Promise((resolve) => {
      source.toBlob(resolve, 'image/jpeg');
    });
    const original = await jpeg.arrayBuffer();
    return original;
  };
  const entry = (author, id, shape) => ({
    groupId: [1],
    author,
    id: Array(16).fill(id),
    text: null,
    edited: false,
    deleted: false,
    unread: false,
    forwarded: false,
    delivery: null,
    read: false,
    sentAtMs: 1000,
    photo: shape,
  });
  const createCall = (fixture) => {
    const { peer, photo, messages, downloads, sends } = fixture;
    const handlers = {
      chat_rows: () =>
        Promise.resolve([
          {
            peerId: peer,
            state: 'open',
            unreadCount: 1,
            activeRecently: false,
            lastMessage: entry(peer, 1, { ...photo, preview: [] }),
          },
        ]),
      chat_history: () =>
        Promise.resolve({
          state: fixture.state,
          messages,
          activeRecently: false,
        }),
      chat_photo_auto: () => {
        fixture.autoAsked += 1;
        return Promise.resolve(fixture.autoAllowed);
      },
      send_chat_photo: (args) => {
        fixture.sent = args;
        const refusal = sends.shift();
        return refusal ? Promise.reject(refusal) : Promise.resolve(Array(16).fill(9));
      },
      chat_photo_original: (args) => {
        fixture.fetched = args;
        const next = downloads.shift();
        return typeof next === 'string' ? Promise.reject(next) : Promise.resolve(next);
      },
      contact_names: (args) =>
        Promise.resolve(args.peerIds.map((peerId) => ({ peerId, name: '', verified: null }))),
    };
    return (command, args) =>
      Object.hasOwn(handlers, command) ? handlers[command](args) : Promise.resolve(0);
  };
  smoke.uiSmokeChatPhotoFixture = async () => {
    const original = await originalBytes();
    const preview = [...new Uint8Array(original)];
    const peer = 'e'.repeat(64);
    const photo = {
      mime: 'image/jpeg',
      width: 400,
      height: 200,
      size: 2048,
      preview,
      hoursLeft: 5,
    };
    const fixture = {
      original,
      peer,
      photo,
      entry,
      state: 'requestReceived',
      downloads: ['notConnected', original, 'notFound'],
      fetched: null,
      autoAllowed: false,
      autoAsked: 0,
      sends: [],
      sent: null,
      messages: [entry(peer, 1, photo)],
    };
    fixture.call = createCall(fixture);
    return fixture;
  };
})();
