// Each disclosure opening owns its reads, picker and import completion.
(() => {
  const el = (id) => document.getElementById(`sticker-${id}`);
  const panel = document.getElementById('sticker-packs');
  const text = (key) => window.strings[`stickers.${key}`];
  let view = null;
  let pickedFor = null;
  const active = (target) => target && view === target && panel.open
    && identityCurrent(target.generation) && !document.getElementById('settings').hidden;
  const message = (key, error = false) => {
    el('status').textContent = key ? text(key) : '';
    el('status').hidden = !key;
    el('status').classList.toggle('error', error);
  };
  const busy = (target, value) => {
    target.busy = value;
    for (const id of ['import', 'refresh', 'next']) el(id).disabled = value;
  };
  const failure = (key, error) => {
    message(key, true);
    el('details').hidden = false;
    el('reason').textContent = String(error);
  };
  const row = (entry) => {
    const item = document.createElement('li');
    const name = document.createElement('strong');
    name.textContent = entry.error ? text('unreadable') : entry.name;
    const info = document.createElement('p');
    info.className = 'caption';
    info.textContent = entry.error ? text('unreadableHint')
      : text('item').replace('{count}', entry.stickers).replace('{size}', sizeText(entry.bytes));
    const id = document.createElement('code');
    id.textContent = ` @${entry.id.map((byte) => byte.toString(16).padStart(2, '0')).join('').slice(0, 8)}`;
    item.append(name, id, info);
    return item;
  };
  const load = async (target, after = null, saved = false) => {
    const paging = document.activeElement === el('next');
    busy(target, true);
    message(saved ? 'saved' : 'loading');
    el('details').hidden = true;
    let lastPage = false;
    try {
      const page = await target.call('saved_sticker_packs', { identityId: target.identityId, after });
      if (!active(target)) return;
      target.next = page.next;
      lastPage = !page.next;
      el('list').replaceChildren(...page.entries.map(row));
      el('next').hidden = !page.next;
      el('unknown').hidden = !page.unrecognizedFiles;
      const usage = text('usage').replace('{used}', sizeText(page.usedBytes));
      message('');
      el('status').textContent = `${saved ? `${text('saved')} ` : ''}${page.entries.length ? usage : `${text('empty')} ${usage}`}`;
      el('status').hidden = false;
    } catch (error) {
      if (active(target)) failure('error', error);
    } finally {
      if (active(target)) {
        busy(target, false);
        if (paging && lastPage) el('refresh').focus();
      }
    }
  };
  const close = () => {
    view = null;
    pickedFor = null;
    panel.open = false;
    el('file').value = '';
    el('list').replaceChildren();
    el('next').hidden = true;
    el('unknown').hidden = true;
    el('details').hidden = true;
    el('details').open = false;
    el('reason').textContent = '';
    message('');
  };
  const open = (call = invoke) => {
    const target = { generation: identityGeneration, identityId: current?.peerId, call, busy: false, next: null };
    view = target;
    el('list').replaceChildren();
    el('next').hidden = true;
    el('unknown').hidden = true;
    return load(target);
  };
  const importFile = async (file, target = view) => {
    if (!file || !active(target) || target.busy) return;
    el('details').hidden = true;
    if (file.size > 8 * 1024 * 1024) { message('tooLarge', true); return; }
    busy(target, true);
    message('importing');
    el('details').hidden = true;
    try {
      const bytes = await file.arrayBuffer();
      if (!active(target)) return;
      if (bytes.byteLength > 8 * 1024 * 1024) { message('tooLarge', true); return; }
      await target.call('import_sticker_pack', {
        identityId: target.identityId, bytes: Array.from(new Uint8Array(bytes)),
      });
      if (active(target)) await load(target, null, true);
    } catch (error) {
      if (active(target)) failure(String(error) === 'saved sticker pack quota exceeded' ? 'quota' : 'importFailed', error);
    } finally {
      if (active(target)) busy(target, false);
    }
  };
  panel.addEventListener('toggle', () => {
    if (panel.open) { if (!view) window.stickersShown = open(); }
    else close();
  });
  el('refresh').addEventListener('click', () => {
    if (active(view) && !view.busy) window.stickersShown = load(view);
  });
  el('next').addEventListener('click', () => {
    if (active(view) && !view.busy && view.next) window.stickersShown = load(view, view.next);
  });
  el('import').addEventListener('click', () => {
    if (!active(view) || view.busy) return;
    pickedFor = view;
    el('file').value = '';
    el('file').click();
  });
  el('file').addEventListener('change', () => {
    const file = el('file').files[0];
    const target = pickedFor;
    pickedFor = null;
    el('file').value = '';
    window.stickerImported = importFile(file, target);
  });
  document.addEventListener('dyapp:identity-change', close);
  document.addEventListener('dyapp:area-change', (event) => { if (event.detail !== 'settings') close(); });
  window.stickerCollection = { open, close, importFile };
})();
