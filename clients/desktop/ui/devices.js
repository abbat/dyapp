import { byId as domById } from './dom.js';
import { identityState, identityCurrent } from './identity-state.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
import { controller as settingsCollectionsController } from './settings-collections.js';
import { progress } from './ui-progress.js';
export const controller = {};
/** @type {Promise<void> | undefined} */
controller.codeChecked = undefined;
/** @type {Promise<void> | undefined} */
controller.linkSent = undefined;
/** @type {Promise<void> | undefined} */
controller.disconnectDone = undefined;

// Settings > Devices: the devices of the open identity, the disconnect dialog and sending the
// identity to a new device.
const devicesCard = domById('devices', HTMLDivElement);
const devicesStatus = domById('devices-status', HTMLParagraphElement);
const disconnectDialog = domById('disconnect', HTMLDialogElement);
const disconnectProblem = domById('disconnect-problem', HTMLParagraphElement);
const disconnectAction = domById('disconnect-action', HTMLButtonElement);
const disconnectRetry = domById('disconnect-retry', HTMLButtonElement);
const disconnectCancel = domById('disconnect-cancel', HTMLButtonElement);
const disconnectButtons = [disconnectCancel, disconnectAction, disconnectRetry];
const codeDialog = domById('device-code-dialog', HTMLDialogElement);
const codeField = domById('device-code', HTMLTextAreaElement);
const codeProblem = domById('device-code-problem', HTMLParagraphElement);
const codeInvalid = domById('device-code-invalid', HTMLParagraphElement);
const confirmDialog = domById('device-confirm-dialog', HTMLDialogElement);
const confirmCancel = domById('device-confirm-cancel', HTMLButtonElement);
const confirmAction = domById('device-confirm-action', HTMLButtonElement);
/** @type {import('./ipc.js').Device | null} */
let disconnecting = null;
let disconnected = false;
let devicesRead = 0;
let shownDevices = '';
document.addEventListener('dyapp:identity-change', () => {
  shownDevices = '';
  devicesRead += 1;
});

const deviceShort = (key) =>
  key
    .slice(0, 3)
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('');
/** @param {boolean} offline */
const disconnectOffline = (offline) => {
  disconnectProblem.hidden = !offline;
  disconnectAction.hidden = offline;
  disconnectRetry.hidden = !offline;
  for (const button of disconnectButtons) button.disabled = false;
};
const openDisconnect = (device) => {
  disconnecting = device;
  disconnected = false;
  devicesStatus.textContent = '';
  domById('disconnect-device', HTMLElement).textContent = deviceShort(device.key);
  disconnectOffline(false);
  disconnectDialog.hidden = false;
  disconnectDialog.showModal();
  disconnectCancel.focus();
};
const deviceRow = (device, index) => {
  const row = document.createElement('tr');
  const name = document.createElement('td');
  const key = document.createElement('code');
  key.textContent = deviceShort(device.key);
  const kind = document.createElement('span');
  kind.className = 'caption';
  kind.textContent = strings[device.this ? 'devices.this' : 'devices.other'];
  name.append(key, document.createElement('br'), kind);
  const action = document.createElement('td');
  if (!device.this) {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'btn';
    button.id = `device-disconnect-${index}`;
    button.textContent = strings['devices.disconnect'];
    button.setAttribute('aria-label', `${button.textContent} ${key.textContent}`);
    button.addEventListener('click', () => openDisconnect(device));
    action.append(button);
  }
  row.append(name, action);
  return row;
};
// The saved list appears at once and the network's newest follows, up to a timeout later; the rows
// are replaced only if they differ, so a row keeps keyboard focus. Only the latest read counts, as
// an older one may still list a disconnected device. An unpublished identity lists no device and
// has no card. A failed live read leaves the saved list up.
const showDevices = async (call = invoke) => {
  const generation = identityState.generation;
  devicesRead += 1;
  const read = devicesRead;
  devicesStatus.textContent = disconnected ? strings['devices.gone'] : '';
  disconnected = false;
  for (const live of [false, true]) {
    const rows = await call('list_devices', { live }).catch(() => null);
    if (read !== devicesRead || !identityCurrent(generation)) return;
    if (!rows) {
      if (!live) {
        devicesCard.hidden = true;
        settingsCollectionsController.renderSettingsRows(
          'devices',
          null,
          deviceRow,
          () => '',
          () => showDevices(),
        );
      }
      return;
    }
    rows.sort((a, b) => b.this - a.this);
    const text = JSON.stringify(rows);
    if (text !== shownDevices)
      settingsCollectionsController.renderSettingsRows(
        'devices',
        rows,
        deviceRow,
        (device) => device.key.map((byte) => byte.toString(16).padStart(2, '0')).join(''),
        () => showDevices(),
      );
    shownDevices = text;
    devicesCard.hidden = rows.length === 0;
  }
};
// A repeat closes the mailbox again, which is harmless, and a draft another device moved on is
// already replaced, so Retry is enough after any failure.
const disconnectNow = async (call = invoke) => {
  const generation = identityState.generation;
  for (const button of disconnectButtons) button.disabled = true;
  try {
    await call('revoke_device', { deviceKey: disconnecting.key });
  } catch (_) {
    if (!identityCurrent(generation)) return;
    disconnectOffline(true);
    disconnectCancel.focus();
    return;
  }
  if (!identityCurrent(generation)) return;
  disconnected = true;
  disconnectDialog.close();
};
disconnectDialog.addEventListener('close', () => {
  disconnectDialog.hidden = true;
  progress.devicesShown = showDevices();
});
const openCode = (problem = '') => {
  codeProblem.textContent = problem && strings[problem];
  codeInvalid.hidden = true;
  codeDialog.hidden = false;
  codeDialog.showModal();
  codeField.focus();
};
// Nothing leaves this computer before the confirmation: Continue only reads the code.
const checkCode = async (call = invoke) => {
  const generation = identityState.generation;
  codeProblem.textContent = '';
  codeInvalid.hidden = true;
  const failure = await call('check_link_code', { code: codeField.value }).then(() => '', String);
  if (!identityCurrent(generation)) return;
  if (failure === 'expired') {
    codeProblem.textContent = strings['devices.code.expired'];
  } else if (failure) {
    codeInvalid.hidden = false;
  } else {
    codeDialog.close();
    confirmDialog.hidden = false;
    confirmDialog.showModal();
    confirmCancel.focus();
  }
};
const sendLink = async (call = invoke) => {
  const generation = identityState.generation;
  confirmCancel.disabled = true;
  confirmAction.disabled = true;
  const failure = await call('send_link', { code: codeField.value }).then(() => '', String);
  confirmCancel.disabled = false;
  confirmAction.disabled = false;
  if (!identityCurrent(generation)) return;
  confirmDialog.close();
  if (failure) openCode(failure === 'expired' ? 'devices.code.expired' : 'devices.code.offline');
  else devicesStatus.textContent = strings['devices.sent'];
};
controller.closeDeviceDialogs = () => {
  for (const dialog of [disconnectDialog, codeDialog, confirmDialog]) dialog.close();
};
for (const dialog of [codeDialog, confirmDialog]) {
  dialog.addEventListener('close', () => {
    dialog.hidden = true;
  });
}
domById('device-link', HTMLButtonElement).addEventListener('click', () => {
  codeField.value = '';
  devicesStatus.textContent = '';
  openCode();
});
domById('device-code-cancel', HTMLButtonElement).addEventListener('click', () =>
  codeDialog.close(),
);
domById('device-code-next', HTMLButtonElement).addEventListener('click', () => {
  controller.codeChecked = checkCode();
});
confirmCancel.addEventListener('click', () => confirmDialog.close());
confirmAction.addEventListener('click', () => {
  controller.linkSent = sendLink();
});
// Escape waits for a running step, like Cancel, so its result is not lost.
confirmDialog.addEventListener('cancel', (event) => {
  if (confirmCancel.disabled) event.preventDefault();
});
disconnectDialog.addEventListener('cancel', (event) => {
  if (disconnectCancel.disabled) event.preventDefault();
});
disconnectCancel.addEventListener('click', () => disconnectDialog.close());
for (const button of [disconnectAction, disconnectRetry]) {
  button.addEventListener('click', () => {
    controller.disconnectDone = disconnectNow();
  });
}

export { checkCode, codeField, disconnectNow, sendLink, showDevices };
