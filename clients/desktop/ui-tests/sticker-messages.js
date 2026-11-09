// Real viewport callbacks and button events; IPC is mocked, native framing is tested separately.
(() => {
  const check = (ok, why) => {
    if (!ok) throw new Error(why);
  };
  const deferred = () => {
    let resolve;
    const promise = new Promise((done) => {
      resolve = done;
    });
    return { promise, resolve };
  };
  const createImageFixture = () => {
    const bytes = new Uint8Array(12);
    const header = new DataView(bytes.buffer);
    header.setUint32(0, 1, true);
    header.setUint32(4, 1, true);
    bytes.set([255, 50, 0, 255], 8);
    const message = {
      groupId: [9],
      author: 'fixture-peer',
      id: Array(16).fill(1),
      sentAtMs: 1,
      sticker: { emoji: '😺', packName: '<Cats> $&' },
    };
    const holder = document.createElement('div');
    holder.style.cssText =
      'position:fixed;inset:0;z-index:100;background:var(--surface);overflow:auto';
    document.body.append(holder);
    const ui = appApi.stickerMessages;
    let closed = false;
    return {
      bytes,
      message,
      holder,
      ui,
      close() {
        if (closed) return;
        closed = true;
        ui.clear();
        holder.remove();
      },
    };
  };
  const checkManualImage = async (fixture) => {
    try {
      const { bytes, message, holder, ui } = fixture;
      ui.clear();
      const started = deferred();
      const calls = [];
      const bubble = ui.bubble(message, 'fixture-peer', async (command, args) => {
        calls.push({ command, args });
        started.resolve();
        if (!args.manual) throw 'onTap';
        return bytes;
      });
      holder.append(bubble);
      await started.promise;
      await appApi.stickerImagesLoaded;
      check(
        calls.length === 1 && !calls[0].args.manual,
        'Viewport read did not use automatic policy',
      );
      check(
        bubble.textContent.includes('<Cats> $&') && !bubble.querySelector('img'),
        `Fallback interpreted peer markup: ${JSON.stringify(bubble.textContent)}`,
      );
      bubble.querySelector('button').focus();
      bubble.querySelector('button').click();
      await appApi.stickerImageLoaded;
      const canvas = bubble.querySelector('canvas');
      check(
        canvas?.width === 1 && canvas.height === 1 && canvas.getAttribute('aria-label'),
        'Manual fetch lost safe pixels/name',
      );
      check(document.activeElement === canvas, 'Manual load lost keyboard focus');
      check(
        calls[1].args.manual &&
          calls[1].args.selection.author === message.author &&
          calls[1].args.identityId === appBindings.current.peerId,
        'Image read lost selection/owner binding',
      );
      check(
        canvas.getContext('2d').getImageData(0, 0, 1, 1).data[0] === 255,
        'RGBA byte layout changed',
      );
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  const checkStaleImage = async (fixture) => {
    try {
      const { bytes, message, holder, ui } = fixture;
      ui.clear();
      holder.replaceChildren();
      const stale = deferred();
      const old = ui.bubble(message, 'fixture-peer', () => stale.promise);
      holder.append(old);
      old.querySelector('button').click();
      const loading = appApi.stickerImageLoaded;
      ui.clear();
      stale.resolve(bytes);
      await loading;
      check(!old.querySelector('canvas'), 'Old image repainted a cleared view');
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  const checkBinaryFrames = (fixture) => {
    try {
      const { bytes } = fixture;
      const padded = new Uint8Array(16);
      padded.set(bytes, 2);
      for (const value of [bytes.buffer, Array.from(bytes), padded.subarray(2, 14)]) {
        check(
          appApi.stickerBinaryPixels(value, 'safe').width === 1,
          'Supported IPC bytes rejected',
        );
      }
      const zero = new Uint8Array(bytes);
      zero[0] = 0;
      for (const value of [[], [1, 2], zero, [...bytes, 0], [...bytes.slice(0, 11), -1]]) {
        let rejected = false;
        try {
          appApi.stickerBinaryPixels(value, 'bad');
        } catch {
          rejected = true;
        }
        check(rejected, 'Malformed frame accepted');
      }
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  const checkOwnSticker = (fixture) => {
    try {
      const { bytes, message } = fixture;
      const own = appApi.chatMessage(
        { ...message, author: appBindings.current.peerId },
        'fixture-peer',
        { state: 'open' },
        async () => bytes,
      );
      check(
        own.classList.contains('sticker-message') && !own.classList.contains('msg'),
        'Sticker gained a bubble',
      );
      const actions = [...own.querySelectorAll('button')].map((button) => button.textContent);
      check(
        actions.includes(appApi.strings['chats.delete']) &&
          !actions.includes(appApi.strings['chats.edit']) &&
          !actions.includes(appApi.strings['chats.forward']),
        'Sticker actions changed',
      );
    } catch (error) {
      fixture.close();
      throw error;
    }
  };
  smoke.checkStickerMessages = async () => {
    const fixture = createImageFixture();
    try {
      await checkManualImage(fixture);
      await checkStaleImage(fixture);
      checkBinaryFrames(fixture);
      checkOwnSticker(fixture);
    } finally {
      fixture.close();
    }
  };
})();
