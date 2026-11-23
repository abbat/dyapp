import { byId as domById, query as domQuery, eventDetail } from './dom.js';
import { sizeText } from './formatting.js';
import { identityCurrent, identityState } from './identity-state.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
export const controller = {};
/** @type {Promise<void> | undefined} */
controller.mediaCacheSaved = undefined;
/** @type {Promise<void> | undefined} */
controller.mediaCacheShown = undefined;

const enabled = domById('media-cache-enabled', HTMLInputElement),
  limit = domById('media-cache-limit', HTMLInputElement),
  clear = domById('media-cache-clear', HTMLButtonElement),
  refresh = domById('media-cache-refresh', HTMLButtonElement);
const dialog = domById('photo-keep-dialog', HTMLDialogElement);
const cancel = domById('photo-keep-cancel', HTMLButtonElement);
const confirm = domById('photo-keep-confirm', HTMLButtonElement);
/**
 * @typedef {{ generation: number, identityId: string, call: import('./ipc.js').Invoke,
 *   config: import('./ipc.js').MediaCacheDecision | null, busy: boolean,
 *   input: string | null }} CacheView
 * @typedef {{ target: CacheView, change: import('./ipc.js').MediaCacheEdit,
 *   opener: Element | null }} CacheChange
 */
/** @type {CacheView | null} */
let view = null;
/** @type {CacheChange | null} */
let asked = null;
/** @param {CacheView} target */
const active = (target) => view === target && identityCurrent(target.generation);
const render = () => {
  const known = view?.config;
  const busy = Boolean(view?.busy);
  enabled.disabled = !known || busy;
  limit.disabled = !known || busy || !known.enabled;
  clear.disabled = !known || busy || !known.usedBytes;
  refresh.disabled = busy;
  enabled.checked = Boolean(known?.enabled);
  enabled.setAttribute('aria-checked', String(enabled.checked));
  limit.value = view?.input ?? known?.limitMib ?? '';
  limit.min = known?.minMib ?? '';
  limit.max = known?.maxMib ?? '';
  domById('media-cache-used', HTMLSpanElement).textContent = known
    ? sizeText(known.usedBytes)
    : strings['usage.unavailable'];
};
/** @param {string} key */
const status = (key) => {
  domById('media-cache-status', HTMLParagraphElement).textContent = key
    ? strings[`photos.cache.${key}`]
        .replace('{min}', view?.config?.minMib ?? '')
        .replace('{max}', view?.config?.maxMib ?? '')
    : '';
};
const close = () => {
  asked = null;
  if (dialog.open) dialog.close();
  dialog.hidden = true;
};
const invalidate = () => {
  view = null;
  close();
  render();
  status('');
};
const show = async (call = invoke) => {
  close();
  const identity = identityState.current;
  if (identity?.status !== 'ready') {
    invalidate();
    return;
  }
  /** @type {CacheView} */
  const target = {
    generation: identityState.generation,
    identityId: identity.peerId,
    call,
    config: null,
    busy: true,
    input: null,
  };
  view = target;
  render();
  status('loading');
  try {
    const config = await call('media_cache', { identityId: target.identityId });
    if (!active(target)) return;
    target.config = config;
    status('');
  } catch (_) {
    if (active(target)) status('failed');
  } finally {
    if (active(target)) {
      target.busy = false;
      render();
    }
  }
};
/** @param {CacheChange} pending */
const ask = (pending) => {
  domQuery(document, '#photo-keep-dialog p', HTMLParagraphElement).textContent =
    strings[
      pending.change.kind === 'clear' ? 'photos.cache.clearWarning' : 'photos.keepEraseWarning'
    ];
  asked = pending;
  dialog.hidden = false;
  dialog.showModal();
  cancel.focus();
};
/** @param {CacheChange} pending @param {boolean} confirmed */
const apply = async (pending, confirmed) => {
  const { target, change } = pending;
  if (!active(target) || target.busy || !target.config) return;
  target.busy = true;
  if (change.kind === 'budget') target.input = change.mib;
  render();
  status('saving');
  try {
    const config = await target.call('change_media_cache', {
      identityId: target.identityId,
      change,
      confirmed,
    });
    if (!active(target)) return;
    target.config = config;
    status('');
    if (config.confirmationRequired) ask(pending);
    else target.input = null;
  } catch (error) {
    if (!active(target)) return;
    if (String(error) === 'invalidLimit') status('invalid');
    else {
      // A durable change may precede a sweep/readback failure; no further mutation until refresh.
      target.config = null;
      status('failed');
    }
  } finally {
    if (active(target)) {
      target.busy = false;
      render();
    }
  }
};
/** @param {import('./ipc.js').MediaCacheEdit} edit */
const change = (edit) => {
  const target = view;
  if (!target?.config || target.busy || asked) return;
  controller.mediaCacheSaved = apply(
    { target, change: edit, opener: document.activeElement },
    false,
  );
};
enabled.addEventListener('change', () => change({ kind: 'retention', enabled: enabled.checked }));
limit.addEventListener('change', () => change({ kind: 'budget', mib: limit.value }));
clear.addEventListener('click', () => change({ kind: 'clear' }));
refresh.addEventListener('click', () => {
  controller.mediaCacheShown = show();
});
const dismiss = () => {
  const button = asked?.opener;
  if (asked && active(asked.target)) asked.target.input = null;
  close();
  render();
  if (button instanceof HTMLElement && button.isConnected && !button.matches(':disabled'))
    button.focus();
};
cancel.addEventListener('click', dismiss);
dialog.addEventListener('cancel', (event) => {
  event.preventDefault();
  dismiss();
});
dialog.addEventListener('close', () => {
  if (!dialog.open) {
    asked = null;
    dialog.hidden = true;
  }
});
confirm.addEventListener('click', () => {
  const pending = asked;
  close();
  if (pending) controller.mediaCacheSaved = apply(pending, true);
});
document.addEventListener('dyapp:identity-change', invalidate);
document.addEventListener('dyapp:area-change', invalidate);
document.addEventListener('dyapp:settings-pane', (event) => {
  if (eventDetail(event) === 'photos') controller.mediaCacheShown = show();
  else invalidate();
});
controller.showMediaCache = show;
