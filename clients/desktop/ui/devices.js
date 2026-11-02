// Settings > Devices: the devices of the open identity and the disconnect dialog.
const devicesCard = document.getElementById('devices');
const devicesList = document.getElementById('devices-list');
const devicesStatus = document.getElementById('devices-status');
const disconnectDialog = document.getElementById('disconnect');
const disconnectProblem = document.getElementById('disconnect-problem');
const disconnectAction = document.getElementById('disconnect-action');
const disconnectRetry = document.getElementById('disconnect-retry');
const disconnectCancel = document.getElementById('disconnect-cancel');
const disconnectButtons = [disconnectCancel, disconnectAction, disconnectRetry];
let disconnecting = null;
let disconnected = false;
let devicesRead = 0;
let shownDevices = '';

const deviceShort = (key) => key.slice(0, 3).map((byte) => byte.toString(16).padStart(2, '0')).join('');
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
  document.getElementById('disconnect-device').textContent = deviceShort(device.key);
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
  kind.textContent = window.strings[device.this ? 'devices.this' : 'devices.other'];
  name.append(key, document.createElement('br'), kind);
  const action = document.createElement('td');
  if (!device.this) {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'btn';
    button.id = `device-disconnect-${index}`;
    button.textContent = window.strings['devices.disconnect'];
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
  const generation = identityGeneration;
  devicesRead += 1;
  const read = devicesRead;
  devicesStatus.textContent = disconnected ? window.strings['devices.gone'] : '';
  disconnected = false;
  for (const live of [false, true]) {
    const rows = await call('list_devices', { live }).catch(() => null);
    if (read !== devicesRead || !identityCurrent(generation)) return;
    if (!rows) {
      if (!live) devicesCard.hidden = true;
      return;
    }
    rows.sort((a, b) => b.this - a.this);
    const text = JSON.stringify(rows);
    if (text !== shownDevices) devicesList.replaceChildren(...rows.map(deviceRow));
    shownDevices = text;
    devicesCard.hidden = rows.length === 0;
  }
};
// A repeat closes the mailbox again, which is harmless, and a draft another device moved on is
// already replaced, so Retry is enough after any failure.
const disconnectNow = async (call = invoke) => {
  const generation = identityGeneration;
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
  window.devicesShown = showDevices();
});
// Escape waits for a running disconnect, like Cancel, so its result is not lost.
disconnectDialog.addEventListener('cancel', (event) => {
  if (disconnectCancel.disabled) event.preventDefault();
});
disconnectCancel.addEventListener('click', () => disconnectDialog.close());
for (const button of [disconnectAction, disconnectRetry]) {
  button.addEventListener('click', () => { window.disconnectDone = disconnectNow(); });
}
