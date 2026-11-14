// Chat photo rendering and download smoke uses actual policy completion and DOM readiness.
(() => {
  const photoReady = (check) =>
    new Promise((resolve, reject) => {
      const observer = new MutationObserver(update);
      const deadline = startTimer(() => finish(Error('Chat photo result deadline')), 10000);
      function finish(error) {
        observer.disconnect();
        stopTimer(deadline);
        if (error) reject(error);
        else resolve();
      }
      function update() {
        try {
          if (check()) finish();
        } catch (error) {
          finish(error);
        }
      }
      observer.observe(appBindings.chatMessages, {
        subtree: true,
        childList: true,
        attributes: true,
        characterData: true,
      });
      update();
    });
  const waitingPhoto = () => document.querySelectorAll('#chat-messages .chat-photo')[1];
  const checkCaption = (box) => {
    const { strings } = appApi;
    const caption = box.querySelector('.caption').textContent;
    for (const part of [
      strings['chats.photo'],
      'image/jpeg',
      appBindings.sizeText(2048),
      strings['chats.photoExpires'].replace('{hours}', 5),
    ]) {
      if (!caption.includes(part)) throw new Error(`the caption lost ${part}: ${caption}`);
    }
  };
  const checkPreview = async (fixture) => {
    const { peer, call } = fixture;
    const { strings } = appApi;
    await openArea('chats-area');
    await appApi.chatsDone;
    await appApi.showChats(call);
    if (!liveRow('chat-list', peer).textContent.includes(strings['chats.photo'])) {
      throw new Error('the list row did not name the photo');
    }

    await fixture.openChat();
    const box = document.querySelector('#chat-messages .chat-photo');
    await box.ready;
    const canvas = box.querySelector('canvas');
    const pixel = canvas
      .getContext('2d')
      .getImageData(Math.floor(canvas.width / 2), Math.floor(canvas.height / 2), 1, 1).data;
    if (canvas.width !== 240 || canvas.height !== 120 || pixel[3] === 0) {
      throw new Error(`the preview was not drawn: ${canvas.width}x${canvas.height}`);
    }
    checkCaption(box);
    if (!box.classList.contains('blurred')) throw new Error('a stranger preview was not blurred');
    await smoke.shot('chat-photo-blurred');
    if (!document.getElementById('chat-attach').disabled)
      throw new Error('a request enabled Attach');
    box.querySelector('.photo-show').click();
    if (
      box.classList.contains('blurred') ||
      box.querySelector('.photo-show') ||
      document.activeElement !== canvas
    ) {
      throw new Error('Show did not reveal the preview and keep the focus');
    }
    await smoke.shot('chat-photo-shown');
  };
  const checkAccepted = async (fixture) => {
    const { messages, entry, peer, photo } = fixture;
    const { strings } = appApi;
    fixture.state = 'open';
    messages.splice(
      0,
      1,
      entry(peer, 1, { ...photo, hoursLeft: 0 }),
      entry(appBindings.current.peerId, 2, { ...photo, preview: [1, 2, 3], hoursLeft: null }),
    );
    await fixture.openChat();
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

    return own;
  };
  const checkOriginalRetry = async (fixture, own) => {
    const { strings } = appApi;
    const button = own.querySelector('.photo-download');
    const finished = (expected) =>
      photoReady(() => own.querySelector('.photo-status').textContent === expected);
    button.click();
    await finished(strings['chats.photoFailed']);
    if (
      fixture.fetched.id[0] !== 2 ||
      fixture.fetched.author !== appBindings.current.peerId ||
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
    await smoke.shot('chat-photo-original');
  };
  const checkOriginalCache = async (fixture) => {
    const { messages, entry, photo } = fixture;
    const { strings } = appApi;
    await fixture.openChat();
    const kept = document.querySelectorAll('#chat-messages .chat-photo')[1];
    await kept.ready;
    if (kept.querySelector('canvas').width !== 8 || kept.querySelector('.photo-download')) {
      throw new Error('a redraw lost the original');
    }
    messages.splice(
      1,
      1,
      entry(appBindings.current.peerId, 3, { ...photo, preview: [], hoursLeft: null }),
    );
    await fixture.openChat();
    const gone = document.querySelectorAll('#chat-messages .chat-photo')[1];
    gone.querySelector('.photo-download').click();
    await photoReady(
      () => gone.querySelector('.photo-status').textContent === strings['chats.photoGone'],
    );
    if (gone.querySelector('.photo-download'))
      throw new Error('a missing original offered a retry');
  };
  const checkPolicy = async (fixture) => {
    const { downloads, original, messages, entry, photo } = fixture;
    // Policy off leaves the button; a later pass that allows it fetches once, without a tap.
    downloads.push(original);
    messages.splice(
      1,
      1,
      entry(appBindings.current.peerId, 4, { ...photo, preview: [], hoursLeft: null }),
    );
    const asked = fixture.autoAsked;
    await fixture.openChat();
    if (
      fixture.autoAsked <= asked ||
      !waitingPhoto().querySelector('.photo-download') ||
      fixture.fetched.id[0] === 4
    ) {
      throw new Error('a refused policy still fetched');
    }
    fixture.autoAllowed = true;
    await fixture.openChat();
    if (waitingPhoto().querySelector('canvas').width !== 8 || downloads.length)
      throw new Error('the allowed photo was not fetched');
  };
  const checkKeptOriginal = async (fixture) => {
    const { downloads, original, messages, entry, photo } = fixture;
    const { strings } = appApi;
    // A kept original needs no node: no expiry in its caption, and it opens under any policy.
    fixture.autoAllowed = false;
    downloads.push(original);
    messages.splice(
      1,
      1,
      entry(appBindings.current.peerId, 5, {
        ...photo,
        preview: [],
        hoursLeft: 0,
        kept: true,
      }),
    );
    await fixture.openChat();
    if (waitingPhoto().textContent.includes(strings['chats.photoExpired'])) {
      throw new Error('a kept original showed as expired');
    }
    if (waitingPhoto().querySelector('canvas').width !== 8)
      throw new Error('a kept original did not complete under the refused policy');
  };
  smoke.uiSmokeChatPhotoRender = async (fixture) => {
    const photos = appApi.chatPhotos;
    const fetchAllowed = photos.fetchAllowed;
    let completion = null;
    photos.fetchAllowed = (peer, call) => {
      const done = fetchAllowed(peer, call);
      if (call === fixture.call) completion = done;
      return done;
    };
    const steps = smoke.timedChecks({
      photoOpening: async () => {
        completion = null;
        await appBindings.openChat(fixture.peer, fixture.call);
      },
      photoPolicy: async () => {
        if (!completion) throw Error('Chat render did not invoke photo policy');
        await completion;
      },
    });
    fixture.openChat = async () => {
      await steps.photoOpening();
      await steps.photoPolicy();
    };
    try {
      await checkPreview(fixture);
      const own = await checkAccepted(fixture);
      await checkOriginalRetry(fixture, own);
      await checkOriginalCache(fixture);
      await checkPolicy(fixture);
      await checkKeptOriginal(fixture);
    } finally {
      photos.fetchAllowed = fetchAllowed;
    }
  };
})();
