import { controller as chatsUiController } from './chats-ui.js';
import { identityCurrent, identityState } from './identity-state.js';
import { strings } from './locale.js';
import { controller as stickersController } from './stickers.js';
export const controller = {};

// Read status only for visible received stickers; actions carry the observed local generation.
(() => {
  let epoch = 0;
  const targets = new Map();
  const text = (key) => strings[`stickerRequest.${key}`];
  const active = (target) =>
    target.epoch === epoch && target.root.isConnected && identityCurrent(target.generation);
  const args = (target) => ({ identityId: target.identityId, selection: target.selection });
  const checked = (value) => {
    if (!['new', 'saved', 'waiting', 'downloading', 'failed'].includes(value?.state))
      throw Error('Invalid pack status');
    const token = value.generation;
    if (
      token !== null &&
      (!Array.isArray(token) ||
        token.length !== 16 ||
        !token.every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255))
    )
      throw Error('Invalid pack generation');
    if (['waiting', 'downloading'].includes(value.state) && token === null)
      throw Error('Missing pack generation');
    if (
      value.state === 'failed' &&
      !['fetch', 'invalid', 'quota', 'storage', 'stickerMissing'].includes(value.failure)
    )
      throw Error('Invalid pack failure');
    return { state: value.state, generation: token, failure: value.failure };
  };
  const focusHere = (target) =>
    target.root.contains(document.activeElement) ? document.activeElement : null;
  /** @param {boolean} value */
  const busy = (target, value) => {
    target.busy = value;
    target.root.setAttribute('aria-busy', String(value));
    for (const button of target.actions.children) button.disabled = value;
  };
  /** @param {Element | null} focus */
  const finish = (target, focus) => {
    if (!active(target)) return;
    busy(target, false);
    if (focus && [document.body, focus].includes(document.activeElement)) {
      const next =
        focus.isConnected && !focus.disabled ? focus : target.actions.querySelector('button');
      next?.focus();
    }
  };
  const add = (target, key, action) => {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'btn';
    button.textContent = text(key);
    button.dataset.action = key;
    button.addEventListener('click', () => {
      if (active(target) && !target.busy) controller.stickerRequestChanged = action();
    });
    target.actions.append(button);
  };
  const manage = (target) => stickersController.stickerCollection.open(target.call);
  const failedActions = (target, value, action) => {
    if (value.generation && value.failure !== 'stickerMissing') action('retry', 'retry');
    else if (value.failure !== 'stickerMissing') add(target, 'refresh', () => read(target));
    if (value.generation && value.failure === 'invalid') action('fresh', 'fresh');
  };
  const render = (target, value) => {
    const signature = JSON.stringify(value);
    target.value = value;
    if (target.signature === signature) return;
    target.signature = signature;
    target.actions.replaceChildren();
    target.error.textContent = '';
    target.caption.className = value.state === 'failed' ? 'notice error' : 'caption';
    target.caption.textContent = text(
      value.state === 'failed' ? value.failure : value.state,
    ).replace('{name}', () => target.name);
    /** @param {string} key @param {string} kind */
    const action = (key, kind) => add(target, key, () => change(target, kind));
    if (value.state === 'new') action('add', 'request');
    if (value.state === 'failed') failedActions(target, value, action);
    if (value.generation) action('stop', 'stopWaiting');
    if (value.state === 'saved' || ['quota', 'storage', 'stickerMissing'].includes(value.failure))
      add(target, 'manage', () => manage(target));
  };
  const failedRead = (target) => {
    target.value = null;
    target.signature = null;
    target.caption.className = 'notice error';
    target.caption.textContent = text('readFailed');
    target.actions.replaceChildren();
    add(target, 'refresh', () => read(target));
    add(target, 'manage', () => manage(target));
  };
  const read = async (target) => {
    if (!active(target) || target.busy) return;
    const focus = focusHere(target);
    busy(target, true);
    try {
      const value = checked(await target.call('chat_sticker_pack_status', args(target)));
      if (active(target)) render(target, value);
    } catch {
      if (active(target)) failedRead(target);
    } finally {
      finish(target, focus);
    }
  };
  const receiveAfterChange = (target, kind) => {
    if (kind !== 'stopWaiting') {
      controller.stickerRequestDelivered = target
        .call('receive_mail')
        .then((count) => {
          if (active(target)) return chatsUiController.mailUpdated(count, target.call);
        })
        .catch(() => {});
    }
  };
  const actionFailure = (error) => {
    if (String(error) === 'unpublished') return 'unpublished';
    if (String(error) === 'too many pending sticker pack requests') return 'limit';
    return 'actionFailed';
  };
  const requestGeneration = (target) =>
    target.value.generation ? [...target.value.generation] : null;
  const change = async (target, kind) => {
    if (!active(target) || target.busy || !target.value) return;
    const focus = focusHere(target);
    const generation = requestGeneration(target);
    busy(target, true);
    target.error.textContent = '';
    try {
      const value = checked(
        await target.call('change_sticker_pack_request', {
          ...args(target),
          action: { kind, generation },
        }),
      );
      if (!active(target)) return;
      render(target, value);
      receiveAfterChange(target, kind);
    } catch (error) {
      if (!active(target)) return;
      // A failure can follow a committed action. Read back before enabling another action.
      target.busy = false;
      await read(target);
      if (active(target)) target.error.textContent = text(actionFailure(error));
    } finally {
      finish(target, focus);
    }
  };
  const observer = new IntersectionObserver((entries) => {
    for (const entry of entries) {
      const target = targets.get(entry.target);
      if (!target) continue;
      target.visible = entry.isIntersecting;
      if (target.visible) controller.stickerRequestsUpdated = read(target);
    }
  });
  const clear = () => {
    epoch += 1;
    observer.disconnect();
    targets.clear();
  };
  const controls = (message, peerId, call) => {
    const root = document.createElement('div');
    root.className = 'sticker-request';
    if (message.author !== peerId) return root;
    const caption = document.createElement('p');
    caption.className = 'caption';
    caption.setAttribute('role', 'status');
    caption.textContent = text('checking');
    const error = document.createElement('p');
    error.className = 'notice error';
    error.setAttribute('role', 'alert');
    const actions = document.createElement('div');
    actions.className = 'actions';
    const target = {
      root,
      caption,
      error,
      actions,
      call,
      epoch,
      visible: false,
      busy: false,
      value: null,
      identityId: identityState.current.peerId,
      generation: identityState.generation,
      name: message.sticker.packName,
      selection: {
        peerId,
        groupId: [...message.groupId],
        author: message.author,
        id: [...message.id],
      },
    };
    root.append(caption, error, actions);
    targets.set(root, target);
    observer.observe(root);
    return root;
  };
  const refresh = (changed = 0, force = false) => {
    controller.stickerRequestsUpdated = Promise.all(
      [...targets.values()]
        .filter(
          (target) =>
            target.visible && (force || changed || !['new', 'saved'].includes(target.value?.state)),
        )
        .map(read),
    );
    return controller.stickerRequestsUpdated;
  };
  document.addEventListener('dyapp:identity-change', clear);
  document.addEventListener('dyapp:area-change', clear);
  document.addEventListener('dyapp:sticker-manager-close', () => refresh(0, true));
  controller.stickerRequests = { controls, clear, refresh };
})();
