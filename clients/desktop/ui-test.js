// Runs in the actual Tauri webview after the bundled page loads.
Promise.all([window.peerIdShown, window.stringsShown]).then(() => requestAnimationFrame(() => requestAnimationFrame(() => {
  const ready = document.getElementById('app-ready');
  const rect = ready && ready.getBoundingClientRect();
  const style = ready && getComputedStyle(ready);
  const { strings } = window;
  const passed = document.title === strings['app.name']
    && document.querySelector('h1').textContent === strings['app.name']
    && ready.textContent === strings['status.ready']
    && strings['status.ready'] === (window.pseudoLocale ? '[Réádý··]' : 'Ready')
    && rect.width > 0 && rect.height > 0
    && style.visibility === 'visible' && style.opacity !== '0'
    && document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2) === ready
    && /^[0-9a-f]{64}$/.test(document.getElementById('peer-id').textContent);
  window.__TAURI__.core.invoke('ui_test_result', { passed: Boolean(passed) });
})), () => window.__TAURI__.core.invoke('ui_test_result', { passed: false }));
