// Runs in the actual Tauri webview after the bundled page loads.
const invokeTest = window.__TAURI__.core.invoke;
const fail = (error) => invokeTest('ui_test_result', {
  passed: false, peerId: '', reason: String(error),
});
const waitFor = (id) => new Promise((resolve, reject) => {
  const section = document.getElementById(id);
  if (!section.hidden) { resolve(); return; }
  const observer = new MutationObserver(() => {
    if (!section.hidden) { observer.disconnect(); clearTimeout(timeout); resolve(); }
  });
  observer.observe(section, { attributes: true, attributeFilter: ['hidden'] });
  const timeout = setTimeout(() => { observer.disconnect(); reject(new Error(`${id} not shown`)); }, 10000);
});
const chooseStorage = async (mode) => {
  const choice = document.getElementById(`${mode}-choice`);
  if (!choice || choice.disabled || choice.hidden) throw new Error('storage choice unavailable');
  choice.click();
  await waitFor('identity');
};
Promise.all([window.identityShown, invokeTest('ui_test_mode'), invokeTest('ui_test_existing'),
  invokeTest('ui_test_step')])
  .then(async ([state, mode, existing, step]) => {
    if (step === 'recovery') {
      const passed = state.status === 'recovery' && state.reason === 'missing'
        && !document.getElementById('recovery').hidden
        && document.getElementById('recovery-reason').textContent
          === window.strings['recovery.missing'];
      return invokeTest('ui_test_result', { passed, peerId: '', reason: 'recovery not shown' });
    }
    if (step === 'add') {
      if (state.status !== 'ready') throw new Error('identity not reopened');
      document.getElementById('switch-identity').click();
      await waitFor('picker');
      document.getElementById('add-identity').click();
      await waitFor('setup');
      await chooseStorage(mode);
    } else if (step === 'pick') {
      if (state.status !== 'picker') throw new Error('identity picker not shown');
      document.querySelector('#identities button').click();
      await waitFor('identity');
    } else if (existing && state.status !== 'ready') {
      throw new Error('identity not reopened');
    } else if (!existing) {
      if (state.status !== 'setup') throw new Error('storage picker not shown');
      await chooseStorage(mode);
    }
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const ready = document.getElementById('app-ready');
    const rect = ready.getBoundingClientRect();
    const style = getComputedStyle(ready);
    const { strings } = window;
    const peerId = document.getElementById('peer-id').textContent;
    const peer = document.getElementById('peer-id');
    const originalBodyWidth = document.body.style.width;
    const peerLinesAt = (width) => {
      document.body.style.width = `${width}px`;
      const range = document.createRange();
      range.selectNodeContents(peer);
      return range.getClientRects().length;
    };
    const narrowLines = peerLinesAt(350);
    const wideLines = peerLinesAt(960);
    document.body.style.width = originalBodyWidth;
    const backup = document.getElementById('file-backup');
    const keyPath = document.getElementById('key-path').textContent;
    const backupMatches = mode === 'file'
      ? !backup.hidden && /[\\/]identity\.key$/.test(keyPath)
        && backup.querySelector('p').textContent === strings['storage.file.backup']
      : backup.hidden && keyPath === '';
    const checks = {
      backupMatches,
      title: document.title === strings['app.name'],
      heading: document.querySelector('h1').textContent === strings['app.name'],
      readyText: ready.textContent === strings['status.ready'],
      locale: strings['status.ready'] === (window.pseudoLocale ? '[Réádý··]'
        : window.locale === 'ru' ? 'Готово' : 'Ready'),
      dimensions: rect.width > 0 && rect.height > 0,
      visibility: style.visibility === 'visible' && style.opacity !== '0',
      onTop: document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2) === ready,
      peerId: /^[0-9a-f]{64}$/.test(peerId),
      peerIdReflows: narrowLines > 1 && wideLines === 1,
    };
    const failed = Object.entries(checks).filter(([, ok]) => !ok).map(([name]) => name);
    return invokeTest('ui_test_result', {
      passed: failed.length === 0, peerId, reason: failed.join(','),
    });
  })
  .catch(fail);
