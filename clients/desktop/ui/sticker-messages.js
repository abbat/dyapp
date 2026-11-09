import { identityCurrent, identityState } from './identity-state.js';
import { strings } from './locale.js';
import { stickerBinaryPixels } from './sticker-pixels.js';
import { controller as stickersController } from './stickers.js';
export const controller = {};
/** @type {Promise<void> | undefined} */
controller.stickerImagesLoaded = undefined;
/** @type {Promise<void> | undefined} */
controller.stickerImageLoaded = undefined;

// IntersectionObserver owns automatic reads; clearing the view invalidates every in-flight reply.
(() => {
  let epoch = 0;
  let batch = Promise.resolve();
  const pending = new Map();
  /** @param {string} key */
  const text = (key) => strings[`stickerImage.${key}`];
  const active = (target) =>
    target.epoch === epoch && target.root.isConnected && identityCurrent(target.generation);
  /** @param {boolean} focused */
  const renderFrame = (target, bytes, focused) => {
    const canvas = stickerBinaryPixels(bytes, target.fallback.textContent);
    target.image.replaceChildren(canvas);
    target.fallback.hidden = true;
    target.button.hidden = true;
    target.status.textContent = '';
    target.loaded = true;
    if (focused && [document.body, target.button].includes(document.activeElement)) {
      canvas.tabIndex = -1;
      canvas.focus();
    }
    observer.unobserve(target.root);
    pending.delete(target.root);
  };
  /** @param {boolean} focused */
  const finishLoad = (target, focused) => {
    if (!active(target)) return;
    target.busy = false;
    target.button.disabled = false;
    target.button.textContent = text('load');
    if (focused && [document.body, target.button].includes(document.activeElement))
      target.button.focus();
  };
  const frameFailure = (error) => {
    const code = String(error);
    if (code === 'onTap') return 'tap';
    if (['expired', 'notFound'].includes(code)) return 'unavailable';
    return 'failed';
  };
  /** @param {boolean} manual */
  const load = async (target, manual) => {
    if (!active(target) || target.busy || target.loaded) return;
    const focused = manual && document.activeElement === target.button;
    target.busy = true;
    target.button.disabled = true;
    target.button.textContent = text('loading');
    if (manual) {
      target.status.setAttribute('role', 'status');
      target.status.textContent = text('loading');
    }
    try {
      const bytes = await target.call('chat_sticker_frame', {
        identityId: target.identityId,
        selection: target.selection,
        manual,
      });
      if (!active(target)) return;
      renderFrame(target, bytes, focused);
    } catch (error) {
      if (!active(target)) return;
      target.status.textContent = text(frameFailure(error));
    } finally {
      finishLoad(target, focused);
    }
  };
  const observer = new IntersectionObserver((entries) => {
    const visible = entries
      .filter((entry) => entry.isIntersecting)
      .map((entry) => pending.get(entry.target));
    batch = batch.then(async () => {
      for (const target of visible) if (target) await load(target, false);
    });
    controller.stickerImagesLoaded = batch;
  });
  const clear = () => {
    epoch += 1;
    observer.disconnect();
    pending.clear();
    batch = Promise.resolve();
  };
  const bubble = (message, peerId, call) => {
    const root = document.createElement('div');
    root.className = 'sticker-body';
    const fallback = document.createElement('p');
    fallback.className = 'caption';
    fallback.textContent = stickersController.stickerMessageText(message.sticker);
    const image = document.createElement('span');
    image.className = 'sticker-image';
    const status = document.createElement('p');
    status.className = 'caption';
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'btn';
    button.textContent = text('load');
    button.dataset.action = 'load-sticker';
    const target = {
      root,
      fallback,
      image,
      status,
      button,
      call,
      epoch,
      generation: identityState.generation,
      identityId: identityState.current.peerId,
      busy: false,
      loaded: false,
      selection: {
        peerId,
        groupId: [...message.groupId],
        author: message.author,
        id: [...message.id],
      },
    };
    button.addEventListener('click', () => {
      controller.stickerImageLoaded = load(target, true);
    });
    root.append(image, fallback, status, button);
    pending.set(root, target);
    observer.observe(root);
    return root;
  };
  document.addEventListener('dyapp:identity-change', clear);
  document.addEventListener('dyapp:area-change', clear);
  controller.stickerMessages = { bubble, clear };
})();
