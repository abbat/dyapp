import { checkControls, isVisible } from './forward-reachability.js';

const node = (id) => document.getElementById(id);
const raw = (id) => node(id).textContent.trim();

/** Static presentation/reachability only; real consent and expiry remain native/core checks. */
export const checkOwnerFixture = async () => {
  const screen = document.documentElement.dataset.screen;
  const pastPreparation = [
    'owner-confirm',
    'owner-confirm-expired',
    'owner-sending',
    'owner-send-error',
    'owner-send-expired',
    'owner-send-check-error',
    'owner-sent',
  ].includes(screen);
  const dialog = node(pastPreparation ? 'owner-confirm-title' : 'owner-code-title').closest(
    '[role=dialog]',
  );
  if (!isVisible(dialog)) throw new Error('Owner approval dialog missing');
  if (
    raw('owner-recipient') !== raw('link-receive-code') ||
    raw('owner-identity') !== raw('peer-id')
  )
    throw new Error('Owner approval changed its original public target');
  if (pastPreparation) {
    for (const key of ['devices.confirm.text', 'devices.confirm.warn']) {
      const warning = dialog.querySelector(`[data-t="${key}"]`);
      if (!isVisible(warning) || !warning.textContent.trim() || warning.textContent.startsWith('?'))
        throw new Error('Full-control or physical-possession warning missing');
    }
  }
  const busy = screen === 'owner-sending';
  if (
    node('owner-confirm-cancel').disabled !== busy ||
    node('owner-code-next').disabled !== (screen === 'owner-preparing')
  )
    throw new Error('Owner approval admitted duplicate work or blocked read-only dismissal');
  if (node('owner-code-cancel').disabled)
    throw new Error('Read-only preparation cannot be dismissed');
  const sendAllowed = ['owner-confirm', 'owner-send-error'].includes(screen);
  const submit = [node('owner-send'), node('owner-retry')].filter(
    (control) => isVisible(control) && !control.disabled,
  );
  if (Boolean(submit.length) !== sendAllowed || submit.length > 1)
    throw new Error('Send admission does not match the named core outcome');
  const uncertain = dialog.querySelector('[data-t="devices.code.offline"]');
  if (
    isVisible(uncertain) !==
    ['owner-send-error', 'owner-send-expired', 'owner-send-check-error'].includes(screen)
  )
    throw new Error('Partial delivery uncertainty was lost or fabricated');
  const expired = dialog.querySelector('[data-t="devices.code.expired"]');
  if (
    isVisible(expired) !==
    ['owner-expired', 'owner-confirm-expired', 'owner-send-expired'].includes(screen)
  )
    throw new Error('Expiry outcome is missing');
  const controls = [...dialog.querySelectorAll('button, textarea')].filter(
    (control) => isVisible(control) && !control.disabled,
  );
  checkUnavailable(dialog, screen);
  await checkControls(controls);
};

const checkUnavailable = (dialog, screen) => {
  const unavailable = dialog.querySelector('[data-t="owner.unavailable"]');
  const failed = screen === 'owner-send-check-error';
  if (isVisible(unavailable) !== failed)
    throw new Error('Unavailable code check is missing or fabricated');
  if (failed && (!node('owner-retry').disabled || !isVisible(node('owner-retry'))))
    throw new Error('Unavailable code check must retain a disabled Retry');
};
