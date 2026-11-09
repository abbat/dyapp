import { byId as domById, all as domAll } from './dom.js';
import { identityCurrent, identityState } from './identity-state.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
import { stickerPixels } from './sticker-pixels.js';
import { controller as stickersController } from './stickers.js';
import { progress } from './ui-progress.js';
export const controller = {};
/** @type {Promise<void> | undefined} */
controller.stickerSent = undefined;

// Every opening and page read owns its callbacks; committed sends belong to the common outbox.
/** @param {string} id */
const el = (id) => domById(`sticker-picker-${id}`, HTMLElement);
const dialog = domById('sticker-picker', HTMLDialogElement);
/** @param {string} key */
const text = (key) => strings[`stickerPicker.${key}`];
/**
 * @typedef {{ peerId: string, call?: import('./ipc.js').Invoke,
 *   onSent?: () => void | Promise<void> }} PickerOptions
 * @typedef {PickerOptions & { identityId: string, generation: number,
 *   call: import('./ipc.js').Invoke, button: HTMLButtonElement, pages: number[],
 *   packPages: (number[] | null)[], read: number, busy: boolean, focus?: Element | null,
 *   pack?: Pick<import('./ipc.js').StickerPack, 'id' | 'name'> | null,
 *   phase?: 'page' | 'packs', after?: number, next?: number | null,
 *   packAfter?: number[] | null, packNext?: number[] | null }} PickerView
 */
/** @type {PickerView | null} */
let view = null;
/** @type {PickerView | null} */
let returnTo = null;
/** @param {PickerView | null} target @returns {target is PickerView} */
const active = (target) =>
  Boolean(target && view === target && dialog.open && identityCurrent(target.generation));
/** @param {string} key */
const notice = (key, error = false) => {
  domById('sticker-picker-status', HTMLParagraphElement).textContent = key ? text(key) : '';
  domById('sticker-picker-status', HTMLParagraphElement).hidden = !key;
  domById('sticker-picker-status', HTMLParagraphElement).classList.toggle('error', error);
};
/** @param {PickerView} target */
const restorePickerFocus = (target) => {
  if ([document.body, dialog, target.focus].includes(document.activeElement)) {
    const focus =
      target.focus?.isConnected && !target.focus.disabled
        ? target.focus
        : domById('sticker-picker-cancel', HTMLButtonElement);
    focus.focus();
  }
};
/** @param {boolean} value @param {PickerView} target */
const busy = (target, value) => {
  if (value && !target.busy) target.focus = document.activeElement;
  target.busy = value;
  for (const button of dialog.querySelectorAll('button')) {
    if (button !== domById('sticker-picker-cancel', HTMLButtonElement))
      button.disabled = value || button.dataset.unavailable === 'true';
  }
  if (!value) restorePickerFocus(target);
};
const close = (restore = true) => {
  const target = view;
  view = null;
  if (dialog.open) dialog.close();
  dialog.hidden = true;
  domById('sticker-picker-choices', HTMLElement).replaceChildren();
  domById('sticker-picker-grid', HTMLDivElement).replaceChildren();
  if (restore && target?.button.isConnected && !target.button.disabled) target.button.focus();
};
/**
 * @param {import('./ipc.js').PickerSticker & { packId?: number[], packName?: string }} entry
 * @param {PickerView} target
 */
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
    if (active(target) && !target.busy) controller.stickerSent = send(target, packId, entry.index);
  });
  return button;
};
/**
 * @param {number} after @param {import('./ipc.js').SavedStickerPage | import('./ipc.js').RecentStickerPage} page
 * @param {PickerView} target
 */
const renderPage = (target, page, after) => {
  target.after = after;
  target.next = page.next;
  domById('sticker-picker-previous', HTMLButtonElement).dataset.unavailable = String(
    !target.pages.length,
  );
  domById('sticker-picker-next', HTMLButtonElement).dataset.unavailable = String(
    page.next === null,
  );
  domById('sticker-picker-grid', HTMLDivElement).replaceChildren(
    ...page.entries.map((entry) => entryButton(target, entry)),
  );
  notice(page.entries.length ? '' : target.pack ? 'empty' : 'noRecent');
};
/** @param {number} read @param {PickerView} target */
const pageCurrent = (target, read) => active(target) && target.read === read;
/** @param {PickerView} target */
const load = async (target, after = 0) => {
  const read = ++target.read;
  target.phase = 'page';
  target.after = after;
  target.next = null;
  domById('sticker-picker-next', HTMLButtonElement).dataset.unavailable = 'true';
  busy(target, true);
  notice('loading');
  domById('sticker-picker-grid', HTMLDivElement).replaceChildren();
  domById('sticker-picker-retry', HTMLButtonElement).hidden = true;
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
      domById('sticker-picker-retry', HTMLButtonElement).hidden = false;
    }
  } finally {
    if (pageCurrent(target, read)) busy(target, false);
  }
};
/**
 * @param {Pick<import('./ipc.js').StickerPack, 'id' | 'name'> | null} pack
 * @param {PickerView} target
 */
const choose = (target, pack) => {
  target.pack = pack;
  target.pages = [];
  for (const button of domAll(
    domById('sticker-picker-choices', HTMLElement),
    ':scope > *',
    HTMLElement,
  ))
    button.setAttribute(
      'aria-pressed',
      String(button.dataset.pack === (pack ? JSON.stringify(pack.id) : 'recent')),
    );
  return load(target);
};
/** @param {number[] | null} [after] @param {PickerView} target */
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
    domById('sticker-picker-choices', HTMLElement).replaceChildren(
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
    domById('sticker-picker-packs-previous', HTMLButtonElement).dataset.unavailable = String(
      !target.packPages.length,
    );
    domById('sticker-picker-packs-next', HTMLButtonElement).dataset.unavailable = String(
      !page.next,
    );
    await choose(target, null);
  } catch {
    if (active(target)) {
      notice('error', true);
      domById('sticker-picker-retry', HTMLButtonElement).hidden = false;
      busy(target, false);
    }
  }
};
/** @param {number[]} packId @param {number} index @param {PickerView} target */
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
/** @param {PickerOptions} options */
const open = ({ peerId, call = invoke, onSent }) => {
  close(false);
  /** @type {PickerView} */
  const target = {
    peerId,
    identityId: identityState.current.peerId,
    generation: identityState.generation,
    call,
    onSent,
    button: domById('chat-stickers', HTMLButtonElement),
    pages: [],
    packPages: [],
    read: 0,
    busy: false,
  };
  view = target;
  dialog.hidden = false;
  dialog.showModal();
  domById('sticker-picker-cancel', HTMLButtonElement).focus();
  return packs(target);
};
domById('sticker-picker-cancel', HTMLButtonElement).addEventListener('click', () => close());
dialog.addEventListener('cancel', (event) => {
  event.preventDefault();
  close();
});
dialog.addEventListener('close', () => {
  if (!dialog.open) close();
});
domById('sticker-picker-retry', HTMLButtonElement).addEventListener('click', () => {
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
domById('sticker-picker-manage', HTMLButtonElement).addEventListener('click', () => {
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
  /** @param {string} peerId */
  leave: (peerId) => {
    if ((view && view.peerId !== peerId) || (returnTo && returnTo.peerId !== peerId)) invalidate();
  },
};
