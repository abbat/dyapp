// A photo in a chat: the list row, the placeholder, the blur until Show or acceptance, expiry, an
// undecodable preview, the text actions a photo must not offer, the tap on the original and
// sending a photo without its metadata.
const checkChatPhoto = async () => {
  const source = document.createElement('canvas');
  source.width = 8;
  source.height = 8;
  const paint = source.getContext('2d');
  paint.fillStyle = '#c03030';
  paint.fillRect(0, 0, 8, 8);
  const jpeg = await new Promise((resolve) => { source.toBlob(resolve, 'image/jpeg'); });
  const original = await jpeg.arrayBuffer();
  const preview = [...new Uint8Array(original)];
  const peer = 'e'.repeat(64);
  const photo = { mime: 'image/jpeg', width: 400, height: 200, size: 2048, preview, hoursLeft: 5 };
  const entry = (author, id, shape) => ({ groupId: [1], author, id: Array(16).fill(id), text: null,
    edited: false, deleted: false, unread: false, forwarded: false, delivery: null, read: false,
    sentAtMs: 1000, photo: shape });
  let state = 'requestReceived';
  const downloads = ['notConnected', original, 'notFound'];
  let fetched = null;
  let autoAllowed = false;
  const sends = [];
  let sent = null;
  const messages = [entry(peer, 1, photo)];
  const call = (command, args) => {
    if (command === 'chat_rows') {
      return Promise.resolve([{ peerId: peer, state: 'open', unreadCount: 1, activeRecently: false,
        lastMessage: entry(peer, 1, { ...photo, preview: [] }) }]);
    }
    if (command === 'chat_history') {
      return Promise.resolve({ state, messages, activeRecently: false });
    }
    if (command === 'chat_photo_auto') return Promise.resolve(autoAllowed);
    if (command === 'send_chat_photo') {
      sent = args;
      const refusal = sends.shift();
      return refusal ? Promise.reject(refusal) : Promise.resolve(Array(16).fill(9));
    }
    if (command === 'chat_photo_original') {
      fetched = args;
      const next = downloads.shift();
      return typeof next === 'string' ? Promise.reject(next) : Promise.resolve(next);
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
  if (!document.getElementById('chat-attach').hidden) throw new Error('a request offered Attach');
  box.querySelector('.photo-show').click();
  if (box.classList.contains('blurred') || box.querySelector('.photo-show')
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
  const finished = (expected) => new Promise((resolve) => {
    const seen = () => own.querySelector('.photo-status').textContent === expected;
    const check = () => { if (seen()) resolve(); else setTimeout(check, 10); };
    check();
  });
  button.click();
  await finished(strings['chats.photoFailed']);
  if (fetched.id[0] !== 2 || fetched.author !== current.peerId || !button.isConnected) {
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
  const missing = () => gone.querySelector('.photo-status').textContent
    === strings['chats.photoGone'];
  await new Promise((resolve) => {
    const check = () => { if (missing()) resolve(); else setTimeout(check, 10); };
    check();
  });
  if (gone.querySelector('.photo-download')) throw new Error('a missing original offered a retry');

  // Policy off leaves the button; a later pass that allows it fetches once, without a tap.
  downloads.push(original);
  messages.splice(1, 1, entry(current.peerId, 4, { ...photo, preview: [], hoursLeft: null }));
  await openChat(peer, call);
  const waiting = () => document.querySelectorAll('#chat-messages .chat-photo')[1];
  const seen = (check) => new Promise((resolve) => {
    const again = () => { if (check()) resolve(); else setTimeout(again, 10); };
    again();
  });
  await new Promise((resolve) => { setTimeout(resolve, 50); });
  if (!waiting().querySelector('.photo-download') || fetched.id[0] === 4) {
    throw new Error('a refused policy still fetched');
  }
  autoAllowed = true;
  await openChat(peer, call);
  await seen(() => waiting().querySelector('canvas').width === 8);
  if (downloads.length) throw new Error('the allowed photo was not fetched');

  // A kept original needs no node: no expiry in its caption, and it opens under any policy.
  autoAllowed = false;
  downloads.push(original);
  messages.splice(1, 1, entry(current.peerId, 5, {
    ...photo, preview: [], hoursLeft: 0, kept: true,
  }));
  await openChat(peer, call);
  if (waiting().textContent.includes(strings['chats.photoExpired'])) {
    throw new Error('a kept original showed as expired');
  }
  await seen(() => waiting().querySelector('canvas').width === 8);

  if (document.getElementById('chat-attach').hidden) throw new Error('an open chat hid Attach');
  const secret = [...new TextEncoder().encode('GPS-SECRET')];
  const plain = [...new Uint8Array(original)];
  const tagged = new File([new Uint8Array([...plain.slice(0, 2), 0xff, 0xe1, 0, secret.length + 2,
    ...secret, ...plain.slice(2)])], 'a.jpg', { type: 'image/jpeg' });
  await sendChatPhoto(tagged, call);
  const holds = (bytes) => bytes.some((_, at) => secret.every((byte, i) => bytes[at + i] === byte));
  if (sent.peerId !== peer || sent.width !== 8 || sent.photo[0] !== 0xff || !sent.preview.length) {
    throw new Error(`the photo was not sent: ${sent.width}`);
  }
  if (holds(sent.photo) || holds(sent.preview)) throw new Error('the metadata left the computer');
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
  await refused(new File([new Uint8Array(4)], 'a.png', { type: 'image/png' }),
    'chats.photoUnsupported');
  if (document.getElementById('chat-attach').disabled) throw new Error('Attach stayed off');
  document.getElementById('chat-back').click();
  await window.showChats(() => Promise.resolve([]));

  await openArea('settings');
  await window.photoModeShown;
  const mode = (value) => document.querySelector(`input[name="photo-mode"][value="${value}"]`);
  if (!mode('byNetwork').checked) throw new Error('the photo mode did not default to by network');
  mode('onTap').click();
  await window.photoModeSaved;
  if (await invoke('auto_download_mode') !== 'onTap') throw new Error('the photo mode was not saved');
  await shot('settings-photos');
  mode('byNetwork').click();
  await window.photoModeSaved;

  const keep = (value) => document.querySelector(`input[name="photo-keep"][value="${value}"]`);
  const dialog = document.getElementById('photo-keep-dialog');
  if (!keep(256).checked) throw new Error('kept originals did not default to 256 MiB');
  keep(1024).click();
  await window.photoModeSaved;
  if (dialog.open || await invoke('kept_originals_mib') !== 1024) {
    throw new Error('a higher limit was not saved at once');
  }
  keep(256).click();
  if (!dialog.open) throw new Error('a lower limit did not ask first');
  document.getElementById('photo-keep-cancel').click();
  await seen(() => keep(1024).checked);
  if (await invoke('kept_originals_mib') !== 1024) throw new Error('Cancel changed the limit');
  keep(256).click();
  document.getElementById('photo-keep-confirm').click();
  await window.photoModeSaved;
  await seen(() => keep(256).checked);
  if (await invoke('kept_originals_mib') !== 256) throw new Error('the lower limit was not saved');
};
// The profile editor: the status line for a bad file, adding, the limit at six, moving with focus
// kept, removal behind its dialog, the form busy state, and the photo line of the publish dialog.
const checkProfilePhotos = async () => {
  const source = document.createElement('canvas');
  source.width = 4;
  source.height = 4;
  source.getContext('2d').fillRect(0, 0, 4, 4);
  const jpeg = await new Promise((resolve) => { source.toBlob(resolve, 'image/jpeg'); });
  const file = new File([jpeg], 'photo.jpg', { type: 'image/jpeg' });
  const bytes = await jpeg.arrayBuffer();
  const hashes = [];
  const commands = [];
  let refusal = null;
  const call = (command, args) => {
    commands.push(command);
    if (command === 'profile_photo') return Promise.resolve(bytes);
    if (refusal) return Promise.reject(refusal);
    if (command === 'add_profile_photo') hashes.push([hashes.length + 1]);
    if (command === 'remove_profile_photo') hashes.splice(args.index, 1);
    if (command === 'move_profile_photo') hashes.splice(args.to, 0, ...hashes.splice(args.from, 1));
    return Promise.resolve({ draft: { photos: [...hashes] } });
  };
  const { strings } = window;
  const status = byId('profile-status');
  const add = byId('profile-photo-add');
  const grid = byId('profile-photos');
  await openArea('profile-area');
  window.showProfilePhotos([], call);
  if (grid.children.length || add.disabled || !byId('profile-photo-limit').hidden) {
    throw new Error('an empty editor did not offer a photo');
  }

  await addProfilePhoto(new File(['text'], 'a.txt', { type: 'text/plain' }), call);
  if (status.textContent !== strings['profile.photoUnsupported'] || commands.length) {
    throw new Error('a wrong file type was not named or reached the core');
  }
  refusal = 'photoRefused';
  await addProfilePhoto(file, call);
  if (status.textContent !== strings['profile.photoRefused'] || grid.children.length) {
    throw new Error('a refused photo was not named or changed the grid');
  }
  refusal = null;

  for (let added = 0; added < 6; added += 1) await addProfilePhoto(file, call);
  await Promise.all([...grid.children].map((tile) => tile.ready));
  if (grid.children.length !== 6 || !add.disabled || byId('profile-photo-limit').hidden
    || !byId('profile-photo-hint').hidden) {
    throw new Error('six photos did not disable Add photo with the reason');
  }
  if (status.textContent !== strings['profile.draft']) throw new Error('a photo was not a draft change');
  if (grid.children[0].querySelector('canvas').hidden) throw new Error('a stored photo was not drawn');
  if (grid.children[0].querySelector('.left') || grid.children[5].querySelector('.right')) {
    throw new Error('a photo offered a move past the end');
  }
  await shot('profile-photos');

  grid.children[0].querySelector('.right').click();
  await window.profilePhotoChanged;
  if (String(hashes.slice(0, 2)) !== '2,1') throw new Error('Move right did not reorder the draft');
  if (!grid.children[1].contains(document.activeElement)) throw new Error('the move lost the focus');

  const dialog = byId('photo-remove-dialog');
  grid.children[0].querySelector('.remove').click();
  if (!dialog.open || document.activeElement !== byId('photo-remove-cancel')) {
    throw new Error('removal did not ask first with Cancel focused');
  }
  await shot('photo-remove-dialog');
  byId('photo-remove-cancel').click();
  if (dialog.open || hashes.length !== 6) throw new Error('Cancel removed a photo');
  grid.children[0].querySelector('.remove').click();
  byId('photo-remove-confirm').click();
  await window.profilePhotoChanged;
  if (hashes.length !== 5 || add.disabled || document.activeElement !== add) {
    throw new Error('removal did not free a place and move the focus to Add photo');
  }

  setProfileBusy(true);
  if (!add.disabled) throw new Error('Add photo stayed enabled while busy');
  const before = commands.length;
  await addProfilePhoto(file, call);
  if (commands.length !== before) throw new Error('a photo changed while the form was busy');
  const avatar = document.querySelector('#sidebar-identity .avatar');
  await window.ownAvatarsShown;
  if (!avatar.querySelector('canvas') || !avatar.textContent) {
    throw new Error('the identity avatar did not draw the first photo over its letters');
  }
  if (commands.some((name) => name.includes('peer') || name.includes('fetch'))) {
    throw new Error('the own photo touched a peer or node command');
  }
  window.showProfilePhotos([], call);
  await window.ownAvatarsShown;
  if (avatar.querySelector('canvas')) throw new Error('an identity without photos kept one');
  setProfileBusy(false);
  if (add.disabled) throw new Error('Add photo stayed disabled after the form was free');

  if (!publicFields({ name: '', photos: [[1], [2]], birthYear: null, birthMonth: null,
    birthDay: null }).includes(`${strings['profile.photos']}: 2`)) {
    throw new Error('the publish dialog did not count the photos');
  }
  profileFailure('full');
  if (status.textContent !== strings['profile.full']) throw new Error('a full node was not named');
  await showProfile();
};
// Other people's photos: lists ask only for thumbnails, the view for full images under the download
// setting, and a refused blob keeps the letters and is asked for again on the next view.
const checkPeerPhotos = async () => {
  const source = document.createElement('canvas');
  source.width = 4;
  source.height = 4;
  source.getContext('2d').fillRect(0, 0, 4, 4);
  const jpeg = await new Promise((resolve) => { source.toBlob(resolve, 'image/jpeg'); });
  const bytes = await jpeg.arrayBuffer();
  const fetches = [];
  const held = new Set();
  const refused = new Set();
  const chatRows = ['ab', 'cd'].map((byte) => ({ peerId: byte.repeat(32), state: 'open',
    lastMessage: null, unreadCount: 0, activeRecently: false }));
  let auto = false;
  const call = (command, args) => {
    if (command === 'fetch_peer_photos') {
      fetches.push(args);
      for (const hash of args.hashes) if (!refused.has(hash[0])) held.add(hash[0]);
      return Promise.resolve();
    }
    if (command === 'peer_photo') {
      return held.has(args.hash[0]) ? Promise.resolve(bytes) : Promise.reject('notFound');
    }
    if (command === 'peer_photo_auto') return Promise.resolve(auto);
    if (command === 'chat_rows') return Promise.resolve(chatRows);
    if (command === 'contact_names') {
      return Promise.resolve(chatRows.map(({ peerId }, index) => (
        { peerId, name: '', verified: null, photo: index ? [] : [9] })));
    }
    return Promise.reject('unsupported');
  };
  const person = (byte, photos, fullPhotos) => ({ peerId: byte.repeat(32), version: 1, place: '',
    fullPhotos, draft: { photos, gender: 0, birthYear: null, birthMonth: null, birthDay: null } });
  const { strings } = window;
  const found = person('ab', [[1], [2]], [[3], [4]]);
  const card = resultCard(found, 'peer-photo-card');
  const avatar = card.querySelector('.avatar');
  const letters = avatar.textContent;

  refused.add(1);
  await window.showCardPhotos([found], [card], call);
  if (avatar.querySelector('canvas') || avatar.textContent !== letters) {
    throw new Error('a refused thumbnail did not leave the letters');
  }
  refused.clear();
  await window.showCardPhotos([found], [card], call);
  if (!avatar.querySelector('canvas')) throw new Error('the next view did not retry the thumbnail');
  if (fetches.some(({ hashes, full }) => full || String(hashes) !== '1')) {
    throw new Error('a list asked for more than the first thumbnail');
  }

  await openArea('chats-area');
  await window.chatsDone;
  await window.showChats(call);
  await window.chatPhotosShown;
  const [shownRow, plainRow] = chatRows.map(({ peerId }) => liveRow('chat-list', peerId));
  if (!shownRow.querySelector('.avatar canvas') || plainRow.querySelector('canvas')
    || !plainRow.querySelector('.avatar').textContent) {
    throw new Error('a chat row did not show its photo, or letters without one');
  }

  await openArea('search-area');
  byId('search-results').append(card);
  const main = document.querySelector('#view-photos .main');
  const thumbs = document.querySelectorAll.bind(document, '#view-photos .thumbs button');
  const load = byId('view-photo-load');
  fetches.length = 0;
  held.clear();
  refused.add(3);
  openFound(found, card, call);
  await window.peerPhotosShown;
  if (fetches.some(({ full }) => full) || !main.querySelector('canvas') || load.hidden) {
    throw new Error('on a metered network the view did not wait for a tap with the thumbnail shown');
  }
  if (thumbs().length !== 2 || !thumbs()[0].hasAttribute('aria-current')
    || main.getAttribute('aria-label') !== strings['profile.photoLabel'].replace('{n}', '1').replace('{count}', '2')) {
    throw new Error('the gallery did not name the photo or mark it current');
  }
  load.click();
  await window.peerPhotoLoaded;
  if (byId('view-status').textContent !== strings['view.photoFailed'] || load.hidden || load.disabled
    || document.activeElement !== load) {
    throw new Error('a failed full photo did not say so and offer the button again');
  }
  refused.clear();
  load.click();
  await window.peerPhotoLoaded;
  if (!load.hidden || document.activeElement !== main || !fetches.at(-1).full
    || String(fetches.at(-1).hashes) !== '3') {
    throw new Error('a tap did not load the full photo and move the focus to it');
  }
  await shot('peer-photos');
  thumbs()[1].click();
  await window.peerPhotoChosen;
  if (load.hidden || thumbs()[0].hasAttribute('aria-current') || !thumbs()[1].hasAttribute('aria-current')) {
    throw new Error('the second photo was not chosen or waited for no tap');
  }

  auto = true;
  held.clear();
  refused.add(5);
  const missing = person('cd', [[5]], [[6]]);
  openFound(missing, card, call);
  if (main.querySelector('canvas')) throw new Error('the last profile stayed under the new name');
  await window.peerPhotosShown;
  if (!load.hidden || !held.has(6)) throw new Error('a free network did not load the full photo');
  refused.clear();
  held.delete(6);
  refused.add(6);
  await window.showFoundPhotos(missing, call);
  if (!main.querySelector('canvas') || !load.hidden) {
    throw new Error('the next view did not retry, or a free network offered a tap');
  }
  refused.clear();
  await window.showFoundPhotos(found, call);
  if (fetches.at(-1).hashes[0][0] !== 3 || !fetches.at(-1).full) {
    throw new Error('a free network did not fetch the full photo without a tap');
  }

  held.clear();
  refused.add(1);
  await window.showFoundPhotos(person('ef', [[1]], [[]]), call);
  if (main.querySelector('canvas') || main.getAttribute('aria-hidden') !== 'true' || !main.textContent) {
    throw new Error('a missing photo did not show the letters');
  }
  byId('view-back').click();
  card.remove();
};
const addRealPhoto = async () => {
  const source = document.createElement('canvas');
  source.width = 640;
  source.height = 640;
  source.getContext('2d').fillRect(0, 0, 640, 640);
  const jpeg = await new Promise((resolve) => { source.toBlob(resolve, 'image/jpeg'); });
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
    await livePoll(() => document.querySelector(`#search-results [data-peer-id="${peer}"] .avatar canvas`),
      'the thumbnail on the card');
  },
};
globalThis.uiSmokePhotos = Object.freeze({
  addRealPhoto, checkChatPhoto, checkProfilePhotos, checkPeerPhotos, photoSteps,
});
