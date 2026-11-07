// Byte budgets survive toggling off; uncertain writes require fresh readback.
(() => {
  const el = (name) => document.getElementById(`media-cache-${name}`);
  const enabled = el('enabled'),
    limit = el('limit'),
    clear = el('clear'),
    refresh = el('refresh');
  const dialog = document.getElementById('photo-keep-dialog');
  const cancel = document.getElementById('photo-keep-cancel');
  const confirm = document.getElementById('photo-keep-confirm');
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
    el('used').textContent = known
      ? sizeText(known.usedBytes)
      : window.strings['usage.unavailable'];
  };
  const status = (key) => {
    el('status').textContent = key ? window.strings[`photos.cache.${key}`] : '';
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
      generation: identityGeneration,
      identityId: current?.peerId,
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
      document.querySelector('#photo-keep-dialog p').textContent =
        window.strings[
          command === 'clear_media_cache' ? 'photos.cache.clearWarning' : 'photos.keepEraseWarning'
        ];
      asked = { target, command, values, opener: document.activeElement };
      dialog.hidden = false;
      dialog.showModal();
      cancel.focus();
    } else window.mediaCacheSaved = apply(target, command, values);
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
    window.mediaCacheShown = show();
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
    if (pending) window.mediaCacheSaved = apply(pending.target, pending.command, pending.values);
  });
  document.addEventListener('dyapp:identity-change', invalidate);
  document.addEventListener('dyapp:area-change', invalidate);
  document.addEventListener('dyapp:settings-pane', (event) => {
    if (event.detail === 'photos') window.mediaCacheShown = show();
    else invalidate();
  });
  window.showMediaCache = show;
})();
