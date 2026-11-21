import { checkControls } from './forward-reachability.js';

/** Runs after capture; this static acknowledgement has no storage or retry callback. */
export const checkIdentityNotice = async () => {
  const dialog = document.getElementById('identity-durability-dialog');
  const title = document.getElementById('identity-durability-title');
  const notice = document.getElementById('identity-durability-notice');
  const acknowledgement = document.getElementById('identity-durability-ack');
  if (!(dialog instanceof HTMLElement) || dialog.hidden || !dialog.getClientRects().length)
    throw new Error('Committed persistence notice is not visible');
  if (
    !title?.textContent.trim() ||
    !notice?.textContent.trim() ||
    notice.getAttribute('aria-live') !== 'polite'
  )
    throw new Error('Committed persistence notice lost its accessible copy');
  if (
    !(acknowledgement instanceof HTMLButtonElement) ||
    acknowledgement.disabled ||
    acknowledgement.dataset.go !== 'profile' ||
    dialog.querySelectorAll('button').length !== 1
  )
    throw new Error('Committed persistence notice must offer acknowledgement only');
  await checkControls([acknowledgement]);
};
