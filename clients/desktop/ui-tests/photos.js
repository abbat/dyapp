// A photo in a chat: the list row, the placeholder, the blur until Show or acceptance, expiry, an
// undecodable preview, the text actions a photo must not offer, the tap on the original and
// sending a photo without its metadata.
const checkChatPhoto = async () => {
  const fixture = await window.uiSmokeChatPhotoFixture();
  const { original, peer, photo, entry, downloads, sends, messages, call } = fixture;
  const { strings } = window;
  await openArea('chats-area');
  await window.chatsDone;
  await window.showChats(call);
  if (!liveRow('chat-list', peer).textContent.includes(strings['chats.photo'])) {
    throw new Error('the list row did not name the photo');
  }

  await openChat(peer, call);
  const box = document.querySelector('#chat-messages .chat-photo');
  await box.ready;
  const canvas = box.querySelector('canvas');
  const pixel = canvas.getContext('2d').getImageData(4, 4, 1, 1).data;
  if (canvas.width !== 240 || canvas.height !== 120 || pixel[3] === 0) {
    throw new Error(`the preview was not drawn: ${canvas.width}x${canvas.height}`);
  }
  const caption = box.querySelector('.caption').textContent;
  for (const part of [
    strings['chats.photo'],
    'image/jpeg',
    sizeText(2048),
    strings['chats.photoExpires'].replace('{hours}', 5),
  ]) {
    if (!caption.includes(part)) throw new Error(`the caption lost ${part}: ${caption}`);
  }
  if (!box.classList.contains('blurred')) throw new Error('a stranger preview was not blurred');
  await shot('chat-photo-blurred');
  if (!document.getElementById('chat-attach').disabled) throw new Error('a request enabled Attach');
  box.querySelector('.photo-show').click();
  if (
    box.classList.contains('blurred') ||
    box.querySelector('.photo-show') ||
    document.activeElement !== canvas
  ) {
    throw new Error('Show did not reveal the preview and keep the focus');
  }
  await shot('chat-photo-shown');

  fixture.state = 'open';
  messages.splice(
    0,
    1,
    entry(peer, 1, { ...photo, hoursLeft: 0 }),
    entry(current.peerId, 2, { ...photo, preview: [1, 2, 3], hoursLeft: null }),
  );
  await openChat(peer, call);
  const [incoming, own] = document.querySelectorAll('#chat-messages .chat-photo');
  await own.ready;
  if (incoming.classList.contains('blurred')) throw new Error('an accepted photo stayed blurred');
  if (!incoming.textContent.includes(strings['chats.photoExpired'])) {
    throw new Error('no expired label');
  }
  if (!own.querySelector('canvas').hidden || !own.textContent.includes('image/jpeg')) {
    throw new Error('an undecodable preview did not leave the placeholder');
  }
  const words = ['chats.edit', 'chats.delete', 'chats.forward'].map((key) => strings[key]);
  const buttons = [...document.querySelectorAll('#chat-messages button')];
  if (buttons.some((button) => words.includes(button.textContent))) {
    throw new Error('a photo offered a text action');
  }
  if (incoming.querySelector('.photo-download')) throw new Error('expired, yet offered');

  const button = own.querySelector('.photo-download');
  const finished = (expected) =>
    new Promise((resolve) => {
      const seen = () => own.querySelector('.photo-status').textContent === expected;
      const check = () => {
        if (seen()) resolve();
        else setTimeout(check, 10);
      };
      check();
    });
  button.click();
  await finished(strings['chats.photoFailed']);
  if (
    fixture.fetched.id[0] !== 2 ||
    fixture.fetched.author !== current.peerId ||
    !button.isConnected
  ) {
    throw new Error('a failed fetch left no retry');
  }
  button.click();
  await finished('');
  const shown = own.querySelector('canvas');
  if (shown.hidden || shown.width !== 8 || own.querySelector('.photo-download')) {
    throw new Error('the original was not shown at its real size');
  }
  await shot('chat-photo-original');
  await openChat(peer, call);
  const kept = document.querySelectorAll('#chat-messages .chat-photo')[1];
  await kept.ready;
  if (kept.querySelector('canvas').width !== 8 || kept.querySelector('.photo-download')) {
    throw new Error('a redraw lost the original');
  }
  messages.splice(1, 1, entry(current.peerId, 3, { ...photo, preview: [], hoursLeft: null }));
  await openChat(peer, call);
  const gone = document.querySelectorAll('#chat-messages .chat-photo')[1];
  gone.querySelector('.photo-download').click();
  const missing = () =>
    gone.querySelector('.photo-status').textContent === strings['chats.photoGone'];
  await new Promise((resolve) => {
    const check = () => {
      if (missing()) resolve();
      else setTimeout(check, 10);
    };
    check();
  });
  if (gone.querySelector('.photo-download')) throw new Error('a missing original offered a retry');

  // Policy off leaves the button; a later pass that allows it fetches once, without a tap.
  downloads.push(original);
  messages.splice(1, 1, entry(current.peerId, 4, { ...photo, preview: [], hoursLeft: null }));
  await openChat(peer, call);
  const waiting = () => document.querySelectorAll('#chat-messages .chat-photo')[1];
  const seen = (check) =>
    new Promise((resolve) => {
      const again = () => {
        if (check()) resolve();
        else setTimeout(again, 10);
      };
      again();
    });
  await new Promise((resolve) => {
    setTimeout(resolve, 50);
  });
  if (!waiting().querySelector('.photo-download') || fixture.fetched.id[0] === 4) {
    throw new Error('a refused policy still fetched');
  }
  fixture.autoAllowed = true;
  await openChat(peer, call);
  await seen(() => waiting().querySelector('canvas').width === 8);
  if (downloads.length) throw new Error('the allowed photo was not fetched');

  // A kept original needs no node: no expiry in its caption, and it opens under any policy.
  fixture.autoAllowed = false;
  downloads.push(original);
  messages.splice(
    1,
    1,
    entry(current.peerId, 5, {
      ...photo,
      preview: [],
      hoursLeft: 0,
      kept: true,
    }),
  );
  await openChat(peer, call);
  if (waiting().textContent.includes(strings['chats.photoExpired'])) {
    throw new Error('a kept original showed as expired');
  }
  await seen(() => waiting().querySelector('canvas').width === 8);

  if (document.getElementById('chat-attach').hidden) throw new Error('an open chat hid Attach');
  const secret = [...new TextEncoder().encode('GPS-SECRET')];
  const plain = [...new Uint8Array(original)];
  const tagged = new File(
    [
      new Uint8Array([
        ...plain.slice(0, 2),
        0xff,
        0xe1,
        0,
        secret.length + 2,
        ...secret,
        ...plain.slice(2),
      ]),
    ],
    'a.jpg',
    { type: 'image/jpeg' },
  );
  await sendChatPhoto(tagged, call);
  const holds = (bytes) => bytes.some((_, at) => secret.every((byte, i) => bytes[at + i] === byte));
  if (
    fixture.sent.peerId !== peer ||
    fixture.sent.width !== 8 ||
    fixture.sent.photo[0] !== 0xff ||
    !fixture.sent.preview.length
  ) {
    throw new Error(`the photo was not sent: ${fixture.sent.width}`);
  }
  if (holds(fixture.sent.photo) || holds(fixture.sent.preview))
    throw new Error('the metadata left the computer');
  const status = document.getElementById('chat-compose-status');
  const refused = async (file, key) => {
    await sendChatPhoto(file, call);
    if (status.textContent !== strings[key]) throw new Error(`no ${key}: ${status.textContent}`);
  };
  sends.push('full');
  await refused(tagged, 'chats.photoFull');
  sends.push('notConnected');
  await refused(tagged, 'chats.notConnected');
  await refused(new File(['GIF89a'], 'a.gif', { type: 'image/gif' }), 'chats.photoUnsupported');
  await refused(
    new File([new Uint8Array(4)], 'a.png', { type: 'image/png' }),
    'chats.photoUnsupported',
  );
  if (document.getElementById('chat-attach').disabled) throw new Error('Attach stayed off');
  document.getElementById('chat-back').click();
  await window.showChats(() => Promise.resolve([]));

  await window.uiSmokePhotoSettings();
};
// The profile editor: the status line for a bad file, adding, the limit at six, moving with focus
// kept, removal behind its dialog, the form busy state, and the photo line of the publish dialog.
const checkProfilePhotos = window.uiSmokeProfilePhotos;
// Other people's photos: lists ask only for thumbnails, the view for full images under the download
// setting, and a refused blob keeps the letters and is asked for again on the next view.
const checkPeerPhotos = window.uiSmokePeerPhotos;
const addRealPhoto = async () => {
  const source = document.createElement('canvas');
  source.width = 640;
  source.height = 640;
  source.getContext('2d').fillRect(0, 0, 640, 640);
  const jpeg = await new Promise((resolve) => {
    source.toBlob(resolve, 'image/jpeg');
  });
  await openArea('profile-area');
  await addProfilePhoto(new File([jpeg], 'photo.jpg', { type: 'image/jpeg' }));
  if (byId('profile-photos').children.length !== 1) {
    throw new Error(`the photo was not added: ${byId('profile-status').textContent}`);
  }
};
const shownSide = () => document.querySelector('#view-photos .main canvas')?.width ?? 0;
const photoSteps = {
  // Over a real node: the thumbnail on the card and in the view, then the full image.
  'photo-seen': async (peer) => {
    await liveFound(peer);
    await window.peerPhotosShown;
    const thumbnail = shownSide();
    if (!thumbnail) throw new Error(`no thumbnail in the view: ${byId('view-status').textContent}`);
    const load = byId('view-photo-load');
    if (!load.hidden) {
      load.click();
      await window.peerPhotoLoaded;
    }
    if (shownSide() !== 640) {
      throw new Error(`no full image, side ${shownSide()}: ${byId('view-status').textContent}`);
    }
    byId('view-back').click();
    await livePoll(
      () => document.querySelector(`#search-results [data-peer-id="${peer}"] .avatar canvas`),
      'the thumbnail on the card',
    );
  },
};
globalThis.uiSmokePhotos = Object.freeze({
  addRealPhoto,
  checkChatPhoto,
  checkProfilePhotos,
  checkPeerPhotos,
  photoSteps,
});
