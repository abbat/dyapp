// The new computer's side of a link: the code, the wait for the record and the identity joined.
// The backend holds the keys of the wait in memory, so leaving this page only has to cancel it.
const linkWait = document.getElementById('link-wait');
const linkShown = document.getElementById('link-shown');
const linkActions = document.getElementById('link-actions');
const linkNew = document.getElementById('link-new');
const linkDone = document.getElementById('link-done');
const linkOpen = document.getElementById('link-open');
const linkMs = 3000;
let linkRun = 0;
let linkTimer = 0;
let linkJoined = null;

const linkState = (status) => {
  linkShown.hidden = status === 'expired' || status === 'finishing';
  linkActions.hidden = status === 'finishing';
  linkNew.hidden = status !== 'expired';
  document.getElementById('link-status').textContent = window.strings[`link.${status}`];
};
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
const finishLink = async (state, call) => {
  linkRun += 1;
  clearTimeout(linkTimer);
  linkJoined = state;
  linkPage('finishing');
  await call('receive_mail').catch(() => {});
  const pending = await call('link_pending').catch(() => true);
  const peer = document.getElementById('link-peer');
  peer.replaceChildren(...who(peer, state.peerId, '', modeText(state)));
  document.getElementById('link-pending').hidden = !pending;
  linkWait.hidden = true;
  linkDone.hidden = false;
  linkOpen.focus();
};

// A record ends the wait even after Cancel: by then the backend has already opened its identity.
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
    document.getElementById('link-code').textContent = code;
    drawContactQr(qr, document.getElementById('link-qr'));
    await pollLink(run, call);
  } catch (_) {
    if (run === linkRun) showError();
  }
};

document.addEventListener('dyapp:identity-change', () => { stopLink(); });
document.getElementById('device-join').addEventListener('click', () => { startLink(); });
linkNew.addEventListener('click', () => { startLink(); });
document.getElementById('link-cancel').addEventListener('click', () => {
  stopLink();
  show(identity, 'status.ready');
});
linkOpen.addEventListener('click', () => act(linkJoined));
copyFrom(document.getElementById('copy-link-code'), document.getElementById('link-code'));
