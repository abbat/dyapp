import { checkControls } from './forward-reachability.js';

const visible = (node) => {
  if (!(node instanceof HTMLElement) || node.getClientRects().length === 0) return false;
  for (let parent = node.parentElement; parent; parent = parent.parentElement) {
    if (
      parent.matches('details:not([open])') &&
      !parent.querySelector(':scope > summary')?.contains(node)
    )
      return false;
  }
  return true;
};
const control = (id) => {
  const node = document.getElementById(id);
  if (!(node instanceof HTMLButtonElement)) throw new Error(`Receiving control missing: ${id}`);
  return node;
};

/** Static state/affordance proof only; native wire, write admission and cleanup need JNI tests. */
export const checkReceiveFixture = async () => {
  const screen = document.documentElement.dataset.screen;
  for (const node of document.querySelectorAll('[data-t]')) {
    if (visible(node) && (!node.textContent.trim() || node.textContent.startsWith('?')))
      throw new Error(`Receiving wording missing: ${node.dataset.t}`);
  }
  if (screen.startsWith('receive-network')) {
    checkNetworkBoundaries(screen);
  } else if (screen.endsWith('intro')) {
    checkIntro(screen);
  } else {
    checkReceivingBoundaries(screen);
  }
  await checkErrorDetails();
  await checkControls(
    [...document.querySelectorAll('button')].filter((node) => visible(node) && !node.disabled),
  );
};

const checkErrorDetails = async () => {
  const details = document.getElementById('link-receive-details')?.closest('details');
  if (!details || !visible(details)) return;
  const copy = control('link-receive-copy-details');
  if (visible(copy)) throw new Error('Collapsed error details expose their copy control');
  details.open = true;
  try {
    await checkControls([copy]);
  } finally {
    details.open = false;
  }
};

const checkIntro = (screen) => {
  if (
    visible(document.getElementById('link-receive-code')) ||
    visible(document.getElementById('link-receive-source'))
  )
    throw new Error('Intro already exposes an operation before explicit creation');
  const back = screen === 'link-receive-wizard-intro' ? 'profile' : 'settings-account';
  if (control('link-receive-back').dataset.go !== back)
    throw new Error('Receiving intro returns through the wrong entry route');
};

const checkNetworkBoundaries = (screen) => {
  if (visible(control('skip-network'))) throw new Error('Receiving network can skip activation');
  if (
    screen === 'receive-network' &&
    control('check-connection').dataset.go !== 'receive-network-ready'
  )
    throw new Error('Receiving network check cannot reach its readiness fixture');
  if (
    !visible(control('network-save')) ||
    control('network-save').disabled !== (screen === 'receive-network-activating')
  )
    throw new Error('Receiving draft save does not reflect activation admission');
  if (control('cancel-receive-network').disabled !== (screen === 'receive-network-activating'))
    throw new Error('Receiving cancellation does not reflect activation admission');
  if (visible(control('network-done')) !== (screen === 'receive-network-active'))
    throw new Error('Receiving code entry bypasses active-network readiness');
  if (control('network-done').dataset.go !== 'link-receive-wizard-intro')
    throw new Error('First-run activation loses the receiving intent');
};

const checkReceivingBoundaries = (screen) => {
  const busy = [
    'link-receive-starting',
    'link-receive-receiving',
    'link-receive-installing',
    'link-setup-running',
  ].includes(screen);
  for (const id of ['link-receive-review', 'link-receive-dismiss']) {
    if (control(id).disabled !== busy)
      throw new Error(`Busy receiving can dispose its result: ${id}`);
  }
  if (
    visible(control('link-receive-cancel')) !==
    [
      'link-receive-starting',
      'link-receive-waiting',
      'link-receive-receiving',
      'link-receive-expired',
      'link-receive-start-error',
      'link-receive-poll-error',
    ].includes(screen)
  )
    throw new Error('Installation cancellation boundary is missing');
  if (
    visible(document.getElementById('link-receive-code')) !==
    ['link-receive-waiting', 'link-receive-receiving', 'link-receive-poll-error'].includes(screen)
  )
    throw new Error('Expired or completed receiver still offers its code');
  if (
    visible(control('link-setup-finish')) !==
    ['link-receive-received', 'link-receive-durability', 'link-setup-error'].includes(screen)
  )
    throw new Error('Typed recovery refusal offers ordinary setup retry');
};
