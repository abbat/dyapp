// The complete photo key must survive IPC, concurrent fetches and a warm browser cache.
const checkPhotoIdentityRequests = (requests, messages, peer) => {
  for (const [index, request] of requests.entries()) {
    const message = messages[index];
    if (
      request.peerId !== peer ||
      request.author !== message.author ||
      JSON.stringify(request.id) !== JSON.stringify(message.id) ||
      JSON.stringify(request.groupId) !== JSON.stringify(message.groupId)
    ) {
      throw Error('Photo original request lost group identity');
    }
  }
};
smoke.uiSmokeChatPhotoIdentity = async (fixture) => {
  const { peer, photo, original, entry } = fixture;
  const photos = appApi.chatPhotos;
  const firstMessage = entry(peer, 6, { ...photo, preview: [], hoursLeft: null });
  const messages = [firstMessage, { ...firstMessage, groupId: [...firstMessage.groupId, 0] }];
  const pending = smoke.pendingSmokeCall();
  const requests = [];
  const call = (command, args) => {
    if (command !== 'chat_photo_original') throw Error('Unexpected photo identity command');
    requests.push(args);
    return requests.length === 1 ? pending.call() : Promise.resolve(original);
  };
  const mount = document.createElement('div');
  document.body.append(mount);
  let firstDone;
  const steps = smoke.timedChecks({
    secondGroupOriginal: (box) => box.fetchPhoto(),
    firstGroupOriginal: () => firstDone,
    cachedGroupsReady: (boxes) => Promise.all(boxes.map((box) => box.ready)),
    photoIdentityCleanup: () => firstDone?.catch(() => {}),
  });
  photos.resetPeer(peer, true);
  try {
    const boxes = messages.map((message) => photos.photoBubble(message, peer, false, call));
    mount.append(...boxes);
    firstDone = boxes[0].fetchPhoto();
    if (requests.length !== 1 || !pending.resolve) {
      throw Error('Photo first request was not admitted');
    }
    await steps.secondGroupOriginal(boxes[1]);
    if (requests.length !== 2) throw Error('Photo groups shared a running fetch');
    checkPhotoIdentityRequests(requests, messages, peer);
    if (!boxes[0].canvas.hidden || boxes[1].canvas.hidden) {
      throw Error('Photo original targeted another group');
    }
    pending.resolve(original);
    await steps.firstGroupOriginal();
    if (boxes.some((box) => box.canvas.hidden || box.querySelector('.photo-download'))) {
      throw Error('Photo original did not complete for its own group');
    }
    const cached = messages.map((message) => photos.photoBubble(message, peer, false, call));
    mount.replaceChildren(...cached);
    await steps.cachedGroupsReady(cached);
    if (
      requests.length !== 2 ||
      cached.some((box) => box.canvas.hidden || box.querySelector('.photo-download'))
    ) {
      throw Error('Photo warm cache crossed group identity');
    }
  } finally {
    photos.resetPeer(null, true);
    pending.resolve?.(original);
    await steps.photoIdentityCleanup();
    mount.remove();
  }
};
