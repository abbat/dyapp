import { byId as domById } from './dom.js';
import { show, sections, who, modeText, showError, identity, act } from './app.js';
import { copyFrom } from './clipboard.js';
import { invoke } from './ipc.js';
import { strings } from './locale.js';
import { drawContactQr } from './search.js';

// The new computer's side of a link: the code, the wait for the record and the identity joined.
// The backend holds the keys of the wait in memory, so leaving this page only has to cancel it.
const linkWait = domById('link-wait', HTMLDivElement);
const linkShown = domById('link-shown', HTMLDivElement);
const linkActions = domById('link-actions', HTMLDivElement);
const linkNew = domById('link-new', HTMLButtonElement);
const linkDone = domById('link-done', HTMLDivElement);
const linkOpen = domById('link-open', HTMLButtonElement);
const linkMs = 3000;
let linkRun = 0;
let linkTimer = 0;
/** @type {import('./identity-state.js').ReadyIdentity | null} */
let linkJoined = null;

/** @param {string} status */
const linkState = (status) => {
  linkShown.hidden = status === 'expired' || status === 'finishing';
  linkActions.hidden = status === 'finishing';
  linkNew.hidden = status !== 'expired';
  domById('link-status', HTMLParagraphElement).textContent = strings[`link.${status}`];
};
/** @param {string} status */
const linkPage = (status) => {
  linkWait.hidden = false;
  linkDone.hidden = true;
  linkState(status);
  show(sections[5], 'link.steps');
};
const stopLink = (call = invoke) => {
  linkRun += 1;
  clearTimeout(linkTimer);
  return call('cancel_link').catch(() => {});
};

// Finishing takes a catch-up pass. One that fails leaves the link pending, and the app keeps trying.
/**
 * @param {import('./identity-state.js').ReadyIdentity} state
 * @param {import('./ipc.js').Invoke} call
 */
const finishLink = async (state, call) => {
  linkRun += 1;
  clearTimeout(linkTimer);
  linkJoined = state;
  linkPage('finishing');
  await call('receive_mail').catch(() => {});
  const pending = await call('link_pending').catch(() => true);
  const peer = domById('link-peer', HTMLParagraphElement);
  peer.replaceChildren(...who(peer, state.peerId, '', modeText(state)));
  domById('link-pending', HTMLParagraphElement).hidden = !pending;
  linkWait.hidden = true;
  linkDone.hidden = false;
  linkOpen.focus();
};

// A record ends the wait even after Cancel: by then the backend has already opened its identity.
/** @param {number} run */
const pollLink = async (run, call = invoke) => {
  let state = null;
  let status = 'waiting';
  try {
    state = await call('poll_link');
  } catch (error) {
    status = String(error) === 'expired' ? 'expired' : 'offline';
  }
  if (state) {
    await finishLink(state, call);
    return;
  }
  if (run !== linkRun) return;
  linkState(status);
  if (status !== 'expired') linkTimer = setTimeout(() => pollLink(run, call), linkMs);
};

const startLink = async (call = invoke) => {
  linkRun += 1;
  const run = linkRun;
  clearTimeout(linkTimer);
  linkPage('waiting');
  try {
    const { code, qr } = await call('start_link');
    if (run !== linkRun) return;
    domById('link-code', HTMLElement).textContent = code;
    drawContactQr(qr, domById('link-qr', HTMLCanvasElement));
    await pollLink(run, call);
  } catch (_) {
    if (run === linkRun) showError();
  }
};

document.addEventListener('dyapp:identity-change', () => {
  stopLink();
});
domById('device-join', HTMLButtonElement).addEventListener('click', () => {
  startLink();
});
linkNew.addEventListener('click', () => {
  startLink();
});
domById('link-cancel', HTMLButtonElement).addEventListener('click', () => {
  stopLink();
  show(identity, 'status.ready');
});
linkOpen.addEventListener('click', () => act(linkJoined));
copyFrom(domById('copy-link-code', HTMLButtonElement), domById('link-code', HTMLElement));

export { linkJoined, linkRun, linkTimer, pollLink, startLink, stopLink };
