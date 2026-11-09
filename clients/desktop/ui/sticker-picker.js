import { identityCurrent, identityState } from './identity-state.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
import { stickerPixels } from './sticker-pixels.js';
import { controller as stickersController } from './stickers.js';
import { progress } from './ui-progress.js';
export const controller = {};

// Every opening and page read owns its callbacks; committed sends belong to the common outbox.
(() => {
  const el = (id) => document.getElementById(`sticker-picker-${id}`);
  const dialog = document.getElementById('sticker-picker');
  const text = (key) => strings[`stickerPicker.${key}`];
  let view = null;
  let returnTo = null;
  const active = (target) =>
    target && view === target && dialog.open && identityCurrent(target.generation);
  const notice = (key, error = false) => {
    el('status').textContent = key ? text(key) : '';
    el('status').hidden = !key;
    el('status').classList.toggle('error', error);
  };
  const restorePickerFocus = (target) => {
    if ([document.body, dialog, target.focus].includes(document.activeElement)) {
      const focus =
        target.focus?.isConnected && !target.focus.disabled ? target.focus : el('cancel');
      focus.focus();
    }
  };
  const busy = (target, value) => {
    if (value && !target.busy) target.focus = document.activeElement;
    target.busy = value;
    for (const button of dialog.querySelectorAll('button')) {
      if (button !== el('cancel')) button.disabled = value || button.dataset.unavailable === 'true';
    }
    if (!value) restorePickerFocus(target);
  };
  const close = (restore = true) => {
    const target = view;
    view = null;
    if (dialog.open) dialog.close();
    dialog.hidden = true;
    el('choices').replaceChildren();
    el('grid').replaceChildren();
    if (restore && target?.button.isConnected && !target.button.disabled) target.button.focus();
  };
  const entryButton = (target, entry) => {
    const packId = target.pack?.id || entry.packId;
    const name = target.pack?.name || entry.packName;
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'btn';
    const label = stickersController.stickerMessageText({ emoji: entry.emoji, packName: name });
    button.setAttribute('aria-label', label);
    const canvas = stickerPixels(entry.frame, label, 96);
    canvas.setAttribute('aria-hidden', 'true');
    button.append(canvas);
    button.addEventListener('click', () => {
      if (active(target) && !target.busy)
        controller.stickerSent = send(target, packId, entry.index);
    });
    return button;
  };
  const renderPage = (target, page, after) => {
    target.after = after;
    target.next = page.next;
    el('previous').dataset.unavailable = String(!target.pages.length);
    el('next').dataset.unavailable = String(page.next === null);
    el('grid').replaceChildren(...page.entries.map((entry) => entryButton(target, entry)));
    notice(page.entries.length ? '' : target.pack ? 'empty' : 'noRecent');
  };
  const pageCurrent = (target, read) => active(target) && target.read === read;
  const load = async (target, after = 0) => {
    const read = ++target.read;
    target.phase = 'page';
    target.after = after;
    target.next = null;
    el('next').dataset.unavailable = 'true';
    busy(target, true);
    notice('loading');
    el('grid').replaceChildren();
    el('retry').hidden = true;
    try {
      const page = await target.call(target.pack ? 'saved_sticker_page' : 'recent_sticker_page', {
        identityId: target.identityId,
        ...(target.pack ? { packId: target.pack.id } : {}),
        after,
      });
      if (!pageCurrent(target, read)) return;
      renderPage(target, page, after);
    } catch {
      if (pageCurrent(target, read)) {
        notice('error', true);
        el('retry').hidden = false;
      }
    } finally {
      if (pageCurrent(target, read)) busy(target, false);
    }
  };
  const choose = (target, pack) => {
    target.pack = pack;
    target.pages = [];
    for (const button of el('choices').children)
      button.setAttribute(
        'aria-pressed',
        String(button.dataset.pack === (pack ? JSON.stringify(pack.id) : 'recent')),
      );
    return load(target);
  };
  const packs = async (target, after = null) => {
    target.phase = 'packs';
    target.packAfter = after;
    busy(target, true);
    notice('loading');
    try {
      const page = await target.call('saved_sticker_packs', {
        identityId: target.identityId,
        after,
      });
      if (!active(target)) return;
      target.packAfter = after;
      target.packNext = page.next;
      const choices = [
        { name: text('recent'), id: null },
        ...page.entries.filter((entry) => !entry.error),
      ];
      el('choices').replaceChildren(
        ...choices.map((pack) => {
          const button = document.createElement('button');
          button.type = 'button';
          button.className = 'btn';
          button.textContent = pack.name;
          button.dataset.pack = pack.id ? JSON.stringify(pack.id) : 'recent';
          button.addEventListener('click', () => {
            if (active(target) && !target.busy)
              progress.stickersShown = choose(target, pack.id ? pack : null);
          });
          return button;
        }),
      );
      el('packs-previous').dataset.unavailable = String(!target.packPages.length);
      el('packs-next').dataset.unavailable = String(!page.next);
      await choose(target, null);
    } catch {
      if (active(target)) {
        notice('error', true);
        el('retry').hidden = false;
        busy(target, false);
      }
    }
  };
  const send = async (target, packId, index) => {
    busy(target, true);
    notice('sending');
    try {
      await target.call('send_saved_sticker', {
        identityId: target.identityId,
        selection: { peerId: target.peerId, packId, index },
      });
      if (!active(target)) return;
      close();
      await target.onSent?.();
    } catch {
      if (active(target)) {
        notice('sendError', true);
        busy(target, false);
      }
    }
  };
  const open = ({ peerId, call = invoke, onSent }) => {
    close(false);
    const target = {
      peerId,
      identityId: identityState.current.peerId,
      generation: identityState.generation,
      call,
      onSent,
      button: document.getElementById('chat-stickers'),
      pages: [],
      packPages: [],
      read: 0,
      busy: false,
    };
    view = target;
    dialog.hidden = false;
    dialog.showModal();
    el('cancel').focus();
    return packs(target);
  };
  el('cancel').addEventListener('click', () => close());
  dialog.addEventListener('cancel', (event) => {
    event.preventDefault();
    close();
  });
  dialog.addEventListener('close', () => {
    if (!dialog.open) close();
  });
  el('retry').addEventListener('click', () => {
    if (active(view) && !view.busy)
      progress.stickersShown =
        view.phase === 'page' ? load(view, view.after) : packs(view, view.packAfter);
  });
  for (const direction of ['previous', 'next'])
    el(direction).addEventListener('click', () => {
      const target = view;
      if (!active(target) || target.busy) return;
      let after;
      if (direction === 'next') {
        target.pages.push(target.after);
        after = target.next;
      } else after = target.pages.pop();
      progress.stickersShown = load(target, after);
    });
  for (const direction of ['previous', 'next'])
    el(`packs-${direction}`).addEventListener('click', () => {
      const target = view;
      if (!active(target) || target.busy) return;
      let after;
      if (direction === 'next') {
        target.packPages.push(target.packAfter);
        after = target.packNext;
      } else after = target.packPages.pop();
      progress.stickersShown = packs(target, after);
    });
  el('manage').addEventListener('click', () => {
    if (!active(view) || view.busy) return;
    returnTo = view;
    close(false);
    progress.stickersShown = stickersController.stickerCollection.open(returnTo.call);
  });
  document.addEventListener('dyapp:sticker-manager-close', () => {
    const target = returnTo;
    returnTo = null;
    if (target && identityCurrent(target.generation)) progress.stickersShown = open(target);
  });
  const invalidate = () => {
    returnTo = null;
    close(false);
  };
  document.addEventListener('dyapp:identity-change', invalidate);
  document.addEventListener('dyapp:area-change', invalidate);
  controller.stickerPicker = {
    open,
    close: invalidate,
    leave: (peerId) => {
      if ((view && view.peerId !== peerId) || (returnTo && returnTo.peerId !== peerId))
        invalidate();
    },
  };
})();
