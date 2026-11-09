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

// Byte budgets survive toggling off; uncertain writes require fresh readback.
(() => {
  const enabled = domById('media-cache-enabled', HTMLInputElement),
    limit = domById('media-cache-limit', HTMLInputElement),
    clear = domById('media-cache-clear', HTMLButtonElement),
    refresh = domById('media-cache-refresh', HTMLButtonElement);
  const dialog = domById('photo-keep-dialog', HTMLDialogElement);
  const cancel = domById('photo-keep-cancel', HTMLButtonElement);
  const confirm = domById('photo-keep-confirm', HTMLButtonElement);
  const mib = 1048576;
  let view = null,
    asked = null;
  const active = (target) => view === target && identityCurrent(target.generation);
  const render = () => {
    const known = view?.config;
    enabled.disabled = !known || view.busy;
    limit.disabled = !known || view.busy || !known.enabled;
    clear.disabled = !known || view.busy || !known.usedBytes;
    refresh.disabled = Boolean(view?.busy);
    enabled.checked = Boolean(known?.enabled);
    enabled.setAttribute('aria-checked', String(enabled.checked));
    limit.value = known ? String(known.limitBytes / mib) : '';
    domById('media-cache-used', HTMLSpanElement).textContent = known
      ? sizeText(known.usedBytes)
      : strings['usage.unavailable'];
  };
  /** @param {string} key */
  const status = (key) => {
    domById('media-cache-status', HTMLParagraphElement).textContent = key
      ? strings[`photos.cache.${key}`]
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
    const target = {
      generation: identityState.generation,
      identityId: identityState.current?.peerId,
      call,
      config: null,
      busy: true,
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
  const apply = async (target, command, values) => {
    if (!active(target) || target.busy || !target.config) return;
    target.busy = true;
    render();
    status('saving');
    try {
      const config = await target.call(command, { identityId: target.identityId, ...values });
      if (!active(target)) return;
      target.config = config;
      status('');
    } catch (_) {
      if (!active(target)) return;
      // The native write may have committed before a later sweep/usage failure.
      target.config = null;
      status('failed');
    } finally {
      if (active(target)) {
        target.busy = false;
        render();
      }
    }
  };
  /** @param {'set_media_cache' | 'clear_media_cache'} command */
  const change = (command, values = {}) => {
    const target = view;
    if (!target?.config || target.busy) return;
    const destructive =
      command === 'clear_media_cache' ||
      (target.config.enabled &&
        (!values.enabled || values.limitBytes < target.config.limitBytes)) ||
      (target.config.usedBytes > 0 &&
        (!values.enabled || values.limitBytes < target.config.usedBytes));
    render();
    if (destructive) {
      domQuery(document, '#photo-keep-dialog p', HTMLParagraphElement).textContent =
        strings[
          command === 'clear_media_cache' ? 'photos.cache.clearWarning' : 'photos.keepEraseWarning'
        ];
      asked = { target, command, values, opener: document.activeElement };
      dialog.hidden = false;
      dialog.showModal();
      cancel.focus();
    } else controller.mediaCacheSaved = apply(target, command, values);
  };
  enabled.addEventListener('change', () => {
    if (!view?.config || view.busy) return;
    change('set_media_cache', { enabled: enabled.checked, limitBytes: view.config.limitBytes });
  });
  limit.addEventListener('change', () => {
    if (!view?.config || view.busy) return;
    const bytes = Number(limit.value) * mib;
    if (!limit.value.trim() || !Number.isSafeInteger(bytes) || bytes < mib || bytes > 10240 * mib) {
      status('invalid');
      return;
    }
    change('set_media_cache', { enabled: view.config.enabled, limitBytes: bytes });
  });
  clear.addEventListener('click', () => change('clear_media_cache'));
  refresh.addEventListener('click', () => {
    controller.mediaCacheShown = show();
  });
  const dismiss = () => {
    const button = asked?.opener;
    close();
    render();
    if (button?.isConnected && !button.disabled) button.focus();
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
    if (pending)
      controller.mediaCacheSaved = apply(pending.target, pending.command, pending.values);
  });
  document.addEventListener('dyapp:identity-change', invalidate);
  document.addEventListener('dyapp:area-change', invalidate);
  document.addEventListener('dyapp:settings-pane', (event) => {
    if (eventDetail(event) === 'photos') controller.mediaCacheShown = show();
    else invalidate();
  });
  controller.showMediaCache = show;
})();
