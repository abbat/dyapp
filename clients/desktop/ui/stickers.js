import { byId as domById } from './dom.js';
import { sizeText } from './formatting.js';
import { identityCurrent, identityState } from './identity-state.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
import { progress } from './ui-progress.js';
export const controller = {};
/** @type {Promise<void> | undefined} */
controller.stickerRemoved = undefined;
/** @type {Promise<void> | undefined} */
controller.stickerImported = undefined;

// Each disclosure opening owns its reads, picker and import completion.
const panel = domById('sticker-packs', HTMLDetailsElement);
const manager = domById('sticker-manager', HTMLDialogElement);
let openedFrom = null;
const text = (key) => strings[`stickers.${key}`];
let view = null;
let removing = null;
const dialog = domById('sticker-remove-dialog', HTMLDialogElement);
const hexId = (id) => id.map((byte) => byte.toString(16).padStart(2, '0')).join('');
const active = (target) =>
  target && view === target && panel.open && identityCurrent(target.generation) && manager.open;
/** @param {string} key */
const message = (key, error = false) => {
  domById('sticker-status', HTMLParagraphElement).textContent = key ? text(key) : '';
  domById('sticker-status', HTMLParagraphElement).hidden = !key;
  domById('sticker-status', HTMLParagraphElement).classList.toggle('error', error);
};
/** @param {boolean} value */
const busy = (target, value) => {
  target.busy = value;
  for (const id of ['sticker-import', 'sticker-refresh', 'sticker-next'])
    domById(id, HTMLButtonElement).disabled = value;
  for (const button of domById('sticker-list', HTMLUListElement).querySelectorAll('button'))
    button.disabled = value;
};
/** @param {string} key */
const failure = (key, error) => {
  message(key, true);
  domById('sticker-details', HTMLDetailsElement).hidden = false;
  domById('sticker-reason', HTMLParagraphElement).textContent = String(error);
};
const row = (entry, target) => {
  const item = document.createElement('li');
  const name = document.createElement('strong');
  name.textContent = entry.error ? text('unreadable') : entry.name;
  const info = document.createElement('p');
  info.className = 'caption';
  info.textContent = entry.error
    ? text('unreadableHint')
    : text('item').replace('{count}', entry.stickers).replace('{size}', sizeText(entry.bytes));
  const id = document.createElement('code');
  id.textContent = ` @${hexId(entry.id).slice(0, 8)}`;
  const remove = document.createElement('button');
  remove.type = 'button';
  remove.className = 'btn quiet';
  remove.textContent = text('remove');
  remove.setAttribute(
    'aria-label',
    `${text('remove')} ${name.textContent} ${hexId(entry.id).slice(0, 8)}`,
  );
  remove.addEventListener('click', () => {
    if (!active(target) || target.busy || !remove.isConnected) return;
    closeDialog();
    removing = { target, id: [...entry.id], button: remove };
    domById('sticker-remove-name', HTMLParagraphElement).textContent = name.textContent;
    const fullId = hexId(entry.id);
    domById('sticker-remove-id', HTMLElement).replaceChildren(
      fullId.slice(0, 32),
      document.createElement('br'),
      fullId.slice(32),
    );
    dialog.hidden = false;
    dialog.showModal();
    domById('sticker-remove-cancel', HTMLButtonElement).focus();
  });
  item.append(name, id, info, remove);
  return item;
};
/** @param {string} notice */
const showCollectionStatus = (page, notice) => {
  const usage = text('usage').replace('{used}', sizeText(page.usedBytes));
  message('');
  domById('sticker-status', HTMLParagraphElement).textContent =
    `${notice ? `${text(notice)} ` : ''}${page.entries.length ? usage : `${text('empty')} ${usage}`}`;
  domById('sticker-status', HTMLParagraphElement).hidden = false;
};
const load = async (target, after = null, notice = '') => {
  const paging = document.activeElement === domById('sticker-next', HTMLButtonElement);
  busy(target, true);
  message('loading');
  domById('sticker-details', HTMLDetailsElement).hidden = true;
  let lastPage = false;
  try {
    const page = await target.call('saved_sticker_packs', {
      identityId: target.identityId,
      after,
    });
    if (!active(target)) return;
    target.next = page.next;
    lastPage = !page.next;
    domById('sticker-list', HTMLUListElement).replaceChildren(
      ...page.entries.map((entry) => row(entry, target)),
    );
    domById('sticker-next', HTMLButtonElement).hidden = !page.next;
    domById('sticker-unknown', HTMLParagraphElement).hidden = !page.unrecognizedFiles;
    showCollectionStatus(page, notice);
  } catch (error) {
    if (active(target)) failure('error', error);
  } finally {
    if (active(target)) {
      busy(target, false);
      if (paging && lastPage) domById('sticker-refresh', HTMLButtonElement).focus();
    }
  }
};
const closeDialog = () => {
  removing = null;
  if (dialog.open) dialog.close();
  dialog.hidden = true;
  domById('sticker-remove-name', HTMLParagraphElement).textContent = '';
  domById('sticker-remove-id', HTMLElement).replaceChildren();
};
dialog.addEventListener('close', () => {
  if (!dialog.open) closeDialog();
});
const cancelRemoval = () => {
  const selection = removing;
  closeDialog();
  if (selection && active(selection.target) && selection.button.isConnected)
    selection.button.focus();
};
domById('sticker-remove-cancel', HTMLButtonElement).addEventListener('click', cancelRemoval);
dialog.addEventListener('cancel', (event) => {
  event.preventDefault();
  cancelRemoval();
});
const removeSelected = async (target, packId) => {
  try {
    await target.call('remove_saved_sticker_pack', {
      identityId: target.identityId,
      packId,
    });
    if (active(target)) await load(target, null, 'removed');
  } catch (error) {
    if (active(target)) {
      await load(target);
      if (active(target)) failure('removeError', error);
    }
  } finally {
    if (active(target)) {
      busy(target, false);
      domById('sticker-refresh', HTMLButtonElement).focus();
    }
  }
};
domById('sticker-remove-confirm', HTMLButtonElement).addEventListener('click', () => {
  const selection = removing;
  if (!dialog.open || !selection || !active(selection.target) || selection.target.busy) return;
  closeDialog();
  const target = selection.target;
  busy(target, true);
  message('removing');
  controller.stickerRemoved = removeSelected(target, selection.id);
});
const close = () => {
  view = null;
  closeDialog();
  if (manager.open) manager.close();
  manager.hidden = true;
  openedFrom = null;
  panel.open = false;
  domById('sticker-file', HTMLInputElement).value = '';
  domById('sticker-list', HTMLUListElement).replaceChildren();
  domById('sticker-next', HTMLButtonElement).hidden = true;
  domById('sticker-unknown', HTMLParagraphElement).hidden = true;
  domById('sticker-details', HTMLDetailsElement).hidden = true;
  domById('sticker-details', HTMLDetailsElement).open = false;
  domById('sticker-reason', HTMLParagraphElement).textContent = '';
  message('');
};
const open = (call = invoke) => {
  closeDialog();
  if (!manager.open) {
    openedFrom = document.activeElement;
    manager.hidden = false;
    manager.showModal();
  }
  panel.open = true;
  const target = {
    generation: identityState.generation,
    identityId: identityState.current?.peerId,
    call,
    busy: false,
    next: null,
  };
  view = target;
  domById('sticker-list', HTMLUListElement).replaceChildren();
  domById('sticker-next', HTMLButtonElement).hidden = true;
  domById('sticker-unknown', HTMLParagraphElement).hidden = true;
  return load(target);
};
/** @param {File} file */
const prepareImport = (file, target) => {
  if (!file || !active(target) || target.busy) return false;
  domById('sticker-details', HTMLDetailsElement).hidden = true;
  if (file.size > 8 * 1024 * 1024) {
    message('tooLarge', true);
    return false;
  }
  busy(target, true);
  message('importing');
  domById('sticker-details', HTMLDetailsElement).hidden = true;
  return true;
};
const showImportFailure = (target, error) => {
  if (active(target))
    failure(
      String(error) === 'saved sticker pack quota exceeded' ? 'quota' : 'importFailed',
      error,
    );
};
/** @param {File} file */
const importFile = async (file, target = view) => {
  if (!prepareImport(file, target)) return;
  try {
    const bytes = await file.arrayBuffer();
    if (!active(target)) return;
    if (bytes.byteLength > 8 * 1024 * 1024) {
      message('tooLarge', true);
      return;
    }
    await target.call('import_sticker_pack', {
      identityId: target.identityId,
      bytes: Array.from(new Uint8Array(bytes)),
    });
    if (active(target)) await load(target, null, 'saved');
  } catch (error) {
    showImportFailure(target, error);
  } finally {
    if (active(target)) busy(target, false);
  }
};
panel.addEventListener('toggle', () => {
  if (panel.open) {
    if (!view) progress.stickersShown = open();
  } else close();
});
domById('sticker-refresh', HTMLButtonElement).addEventListener('click', () => {
  if (active(view) && !view.busy) progress.stickersShown = load(view);
});
domById('sticker-next', HTMLButtonElement).addEventListener('click', () => {
  if (active(view) && !view.busy && view.next) progress.stickersShown = load(view, view.next);
});
domById('sticker-import', HTMLButtonElement).addEventListener('click', () => {
  if (!active(view) || view.busy) return;
  const target = view;
  // The chooser owns this element; its late change must never inherit another opening's owner.
  const input = /** @type {HTMLInputElement} */ (
    domById('sticker-file', HTMLInputElement).cloneNode()
  );
  input.value = '';
  domById('sticker-file', HTMLInputElement).replaceWith(input);
  input.addEventListener('change', () => {
    const file = input.files[0];
    input.value = '';
    controller.stickerImported = importFile(file, target);
  });
  input.click();
});
const dismiss = () => {
  const button = openedFrom;
  close();
  if (button?.isConnected && !button.disabled) button.focus();
  document.dispatchEvent(new Event('dyapp:sticker-manager-close'));
};
domById('sticker-manager-cancel', HTMLButtonElement).addEventListener('click', dismiss);
manager.addEventListener('cancel', (event) => {
  event.preventDefault();
  dismiss();
});
// A queued close event from an earlier opening must not close a newly reopened manager.
manager.addEventListener('close', () => {
  if (!manager.open) close();
});
document.addEventListener('dyapp:identity-change', close);
document.addEventListener('dyapp:area-change', close);
controller.stickerMessageText = (sticker) =>
  text('message')
    .replace('{emoji}', () => sticker.emoji)
    .replace('{name}', () => sticker.packName);
controller.stickerCollection = { open, close, importFile };
