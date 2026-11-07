// Keep category navigation local to Settings; backend availability owns inner cards.
(() => {
  const root = document.getElementById('settings-root');
  const back = document.getElementById('settings-back');
  const panes = [...document.querySelectorAll('[data-settings-pane]')];
  const parent = { identities: 'account', devices: 'account', blocked: 'privacy', passed: 'privacy' };
  let selected = null;
  const open = (name = null, focus = true) => {
    if (name && !panes.some((pane) => pane.dataset.settingsPane === name)) return;
    selected = name;
    root.hidden = Boolean(name);
    back.hidden = !name;
    for (const pane of panes) pane.hidden = pane.dataset.settingsPane !== name;
    document.dispatchEvent(new CustomEvent('dyapp:settings-pane', { detail: name }));
    if (focus) {
      window.scrollTo(0, 0);
      (name ? back : root.querySelector('button')).focus();
    }
  };
  for (const button of document.querySelectorAll('[data-settings-open]')) {
    button.addEventListener('click', () => open(button.dataset.settingsOpen));
  }
  back.addEventListener('click', () => {
    const previous = selected;
    const destination = parent[previous] || null;
    open(destination, false);
    const container = destination
      ? panes.find((pane) => pane.dataset.settingsPane === destination) : root;
    container.querySelector(`[data-settings-open="${previous}"]`)?.focus();
    window.scrollTo(0, 0);
  });
  document.addEventListener('dyapp:identity-change', () => open(null, false));
  document.addEventListener('dyapp:area-change', () => open(null, false));
  // Native checkbox checked state supplies the switch's accessibility state.
  for (const id of ['send-views', 'send-read-receipts', 'send-typing',
    'message-notifications', 'tray-show', 'tray-close']) {
    const input = document.getElementById(id);
    input.setAttribute('role', 'switch');
    input.classList.add('switch');
  }
  window.openSettingsPane = open;
})();
