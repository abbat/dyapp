export let updatePermissionFixture = null;

/** Controlled platform presentation only; never a feed request, OS prompt or preference write. */
export const installUpdatePermissionFixture = (strings, screen) => {
  if (!screen.startsWith('update-notice-permission-')) return;
  const root = document.getElementById('update-permission');
  const status = document.getElementById('update-permission-status');
  const error = document.getElementById('update-permission-error');
  const action = document.getElementById('update-permission-action');
  const state = {
    active: true,
    pending: screen.endsWith('-pending'),
    allowed: screen.endsWith('-unknown') ? null : screen.endsWith('-allowed'),
    failed: screen.endsWith('-failed'),
    attempts: 0,
    reads: 0,
  };
  let readback = state.allowed;
  let failLaunch = state.failed;
  const render = () => {
    root.hidden = false;
    status.textContent =
      strings[
        state.allowed == null
          ? 'update.permissionFailed'
          : state.allowed
            ? 'update.permissionAllowed'
            : 'update.permissionBlocked'
      ];
    error.textContent = strings['update.permissionFailed'];
    error.hidden = !state.failed || state.allowed == null;
    action.textContent =
      strings[state.allowed == null ? 'update.permissionRetry' : 'update.permissionManage'];
    action.disabled = state.pending;
  };
  const read = () => {
    state.reads++;
    state.allowed = readback;
    state.failed = readback == null;
  };
  const launch = () => {
    if (!state.active || state.pending) return;
    if (state.allowed == null) {
      read();
    } else {
      state.attempts++;
      state.failed = failLaunch;
      state.pending = !failLaunch;
    }
    render();
  };
  const complete = () => {
    if (!state.active || !state.pending) return;
    state.pending = false;
    read();
    render();
  };
  const retire = () => {
    state.active = false;
  };
  action.addEventListener('click', launch);
  document.getElementById('update-state').textContent = strings['update.package'];
  const checked = new Intl.DateTimeFormat(document.documentElement.lang, {
    dateStyle: 'short',
    timeStyle: 'short',
  }).format(new Date(1000));
  document.querySelector('[data-t="update.lastNever"]').textContent = strings[
    'update.checkedSample'
  ].replace('{date}', checked);
  updatePermissionFixture = {
    state,
    launch,
    complete,
    retire,
    setReadback: (value) => {
      readback = value;
    },
    allowLaunch: () => {
      failLaunch = false;
    },
  };
  render();
};
