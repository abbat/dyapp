export let updateFixture = null;

/** Presentation-only controlled outcomes; no feed, browser, storage or installer is invoked. */
export const installUpdateFixture = (strings, screen) => {
  if (!screen.startsWith('update-notice-')) return;
  const scrim = document.getElementById('update-notice-scrim');
  const dialog = document.getElementById('update-notice');
  const open = document.getElementById('update-notice-open');
  const later = document.getElementById('update-notice-later');
  const status = document.getElementById('update-notice-status');
  const badge = document.getElementById('update-available-badge');
  const source = screen === 'update-notice-source';
  const state = {
    present: true,
    pending: screen === 'update-notice-saving',
    writeFailed: screen === 'update-notice-write-error',
    openFailed: screen === 'update-notice-open-error',
    opened: [],
    dismissals: 0,
  };
  const render = () => {
    scrim.hidden = !state.present;
    open.disabled = state.pending || state.opened.length > 0;
    later.disabled = state.pending;
    const key = state.pending
      ? 'update.saving'
      : state.writeFailed
        ? 'update.writeError'
        : state.openFailed
          ? 'update.openError'
          : null;
    status.hidden = key == null;
    status.textContent = key == null ? '' : strings[key];
  };
  const dismiss = () => {
    if (!state.present || state.pending) return;
    state.dismissals++;
    state.pending = true;
    render();
  };
  const completeLater = (saved) => {
    if (!state.present || !state.pending) return;
    state.pending = false;
    state.writeFailed = !saved;
    if (saved) state.present = false;
    render();
    if (!state.present) document.getElementById('nav-settings').focus();
  };
  const launch = () => {
    if (!state.present || state.pending || state.opened.length) return;
    if (!state.openFailed) state.opened.push('https://github.com/abbat/dyapp/releases/tag/v0.0.2');
    render();
  };
  document.getElementById('update-notice-offer').textContent =
    strings[source ? 'update.source' : 'update.package'];
  open.textContent = strings[source ? 'update.build' : 'update.open'];
  badge.hidden = false;
  badge.setAttribute('aria-label', strings['update.badge']);
  open.addEventListener('click', launch);
  later.addEventListener('click', dismiss);
  dialog.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') {
      event.preventDefault();
      dismiss();
    }
    if (event.key === 'Enter' && event.target === dialog && !open.disabled) {
      event.preventDefault();
      launch();
    }
    if (event.key === 'Tab') {
      event.preventDefault();
      const controls = [later, open].filter((node) => !node.disabled);
      const index = controls.indexOf(document.activeElement);
      const next = (index + (event.shiftKey ? -1 : 1) + controls.length) % controls.length;
      (controls[next] ?? dialog).focus();
    }
  });
  updateFixture = { state, launch, dismiss, completeLater, render };
  render();
  (state.pending ? dialog : later).focus();
};
