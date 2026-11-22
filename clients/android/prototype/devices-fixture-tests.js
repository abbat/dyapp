import { checkControls, isVisible } from './forward-reachability.js';

const checkRows = (screen) => {
  const rows = [...document.querySelectorAll('#devices-list tr')];
  const empty = ['devices-loading', 'devices-empty', 'devices-error'].includes(screen);
  if (empty) {
    if (rows.length) throw new Error('Unavailable device state fabricated rows');
    return;
  }
  const current = rows.filter((row) => row.hasAttribute('data-device-current'));
  if (current.length !== 1 || current[0] !== rows[0] || current[0].querySelector('button'))
    throw new Error('Current device is missing, reordered or disconnectable');
  if (rows.length !== (screen === 'devices-gone' ? 1 : 2))
    throw new Error('Unconfirmed device state removed its target');
};

const checkConfirmation = (screen, dialog) => {
  const busy = screen === 'devices-revoking';
  const retry = document.getElementById('disconnect-retry');
  const action = document.getElementById('disconnect-confirm');
  const cancel = document.getElementById('disconnect-cancel');
  const rowKey = document.querySelector('#device-disconnect').closest('tr').querySelector('code');
  if (document.getElementById('disconnect-device').textContent !== rowKey.textContent)
    throw new Error('Disconnect confirmation changed its target');
  if (cancel.disabled !== busy || action.disabled !== busy)
    throw new Error('Admitted disconnect can be cancelled or repeated');
  if (isVisible(retry) !== (screen === 'devices-uncertain'))
    throw new Error('Retry is not an explicit uncertain-result action');
  return [...dialog.querySelectorAll('button')].filter((node) => isVisible(node) && !node.disabled);
};

/** Actual static controls and geometry; this is not native authorization evidence. */
export const checkDevicesFixture = async () => {
  const screen = document.documentElement.dataset.screen;
  checkRows(screen);
  const status = document.getElementById('device-read-status');
  if (!status.textContent.trim() || status.textContent.startsWith('?'))
    throw new Error('Device read outcome is missing');
  const dialog = document.querySelector('[aria-labelledby="disconnect-title"]');
  const confirming = ['devices-confirm', 'devices-revoking', 'devices-uncertain'].includes(screen);
  if (isVisible(dialog) !== confirming) throw new Error('Wrong device confirmation state');
  const controls = confirming
    ? checkConfirmation(screen, dialog)
    : [...document.querySelectorAll('#settings-devices button, #settings-devices input')].filter(
        (node) => isVisible(node) && !node.disabled,
      );
  if (screen === 'devices-loading' && !document.getElementById('settings-devices-refresh').disabled)
    throw new Error('Pending initial read admitted another refresh');
  await checkControls(controls);
};
