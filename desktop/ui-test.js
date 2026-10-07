// Runs in the actual Tauri webview after the bundled page loads.
requestAnimationFrame(() => requestAnimationFrame(() => {
  const ready = document.getElementById('app-ready');
  const rect = ready && ready.getBoundingClientRect();
  const style = ready && getComputedStyle(ready);
  const passed = document.title === 'DYApp'
    && document.querySelector('h1').textContent === 'DYApp'
    && ready.textContent === 'Ready'
    && rect.width > 0 && rect.height > 0
    && style.visibility === 'visible' && style.opacity !== '0'
    && document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2) === ready;
  window.__TAURI__.tauri.invoke('ui_test_result', { passed: Boolean(passed) });
}));
