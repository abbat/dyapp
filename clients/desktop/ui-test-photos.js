// A photo in a chat: the list row, the placeholder, the blur until Show or acceptance, expiry, an
// undecodable preview and the text actions a photo must not offer.
const checkChatPhoto = async () => {
  const source = document.createElement('canvas');
  source.width = 8;
  source.height = 8;
  const paint = source.getContext('2d');
  paint.fillStyle = '#c03030';
  paint.fillRect(0, 0, 8, 8);
  const jpeg = await new Promise((resolve) => { source.toBlob(resolve, 'image/jpeg'); });
  const preview = [...new Uint8Array(await jpeg.arrayBuffer())];
  const peer = 'e'.repeat(64);
  const photo = { mime: 'image/jpeg', width: 400, height: 200, size: 2048, preview, hoursLeft: 5 };
  const entry = (author, id, shape) => ({ groupId: [1], author, id: Array(16).fill(id), text: null,
    edited: false, deleted: false, unread: false, forwarded: false, delivery: null, read: false,
    sentAtMs: 1000, photo: shape });
  let state = 'requestReceived';
  const messages = [entry(peer, 1, photo)];
  const call = (command, args) => {
    if (command === 'chat_rows') {
      return Promise.resolve([{ peerId: peer, state: 'open', unreadCount: 1, activeRecently: false,
        lastMessage: entry(peer, 1, { ...photo, preview: [] }) }]);
    }
    if (command === 'chat_history') {
      return Promise.resolve({ state, messages, activeRecently: false });
    }
    if (command === 'contact_names') {
      return Promise.resolve(args.peerIds.map((peerId) => ({ peerId, name: '', verified: null })));
    }
    return Promise.resolve(0);
  };
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
  for (const part of [strings['chats.photo'], 'image/jpeg', sizeText(2048),
    strings['chats.photoExpires'].replace('{hours}', 5)]) {
    if (!caption.includes(part)) throw new Error(`the caption lost ${part}: ${caption}`);
  }
  if (!box.classList.contains('blurred')) throw new Error('a stranger preview was not blurred');
  await shot('chat-photo-blurred');
  box.querySelector('button').click();
  if (box.classList.contains('blurred') || box.querySelector('button')
    || document.activeElement !== canvas) {
    throw new Error('Show did not reveal the preview and keep the focus');
  }
  await shot('chat-photo-shown');

  state = 'open';
  messages.splice(0, 1, entry(peer, 1, { ...photo, hoursLeft: 0 }),
    entry(current.peerId, 2, { ...photo, preview: [1, 2, 3], hoursLeft: null }));
  await openChat(peer, call);
  const [incoming, own] = document.querySelectorAll('#chat-messages .chat-photo');
  await own.ready;
  if (incoming.classList.contains('blurred')) throw new Error('an accepted photo stayed blurred');
  if (!incoming.textContent.includes(strings['chats.photoExpired'])) {
    throw new Error('no expired label');
  }
  if (own.querySelector('canvas') || !own.textContent.includes('image/jpeg')) {
    throw new Error('an undecodable preview did not leave the placeholder');
  }
  const words = ['chats.edit', 'chats.delete', 'chats.forward'].map((key) => strings[key]);
  const buttons = [...document.querySelectorAll('#chat-messages button')];
  if (buttons.some((button) => words.includes(button.textContent))) {
    throw new Error('a photo offered a text action');
  }
  document.getElementById('chat-back').click();
  await window.showChats(() => Promise.resolve([]));
};
globalThis.uiSmokePhotos = Object.freeze({ checkChatPhoto });
