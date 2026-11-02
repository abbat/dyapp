// Settings → Passed people: Undo pass shows the peer in later searches and Liked you again.
const unpassPeer = async (peerId, button, call) => {
  const generation = identityGeneration;
  button.disabled = true;
  let failed = false;
  try {
    await call('pass_peer', { peerId, passed: false });
  } catch (_) {
    failed = true;
  }
  if (!identityCurrent(generation)) return;
  await window.showPassed(call);
  if (failed && identityCurrent(generation)) privacyError.hidden = false;
};
window.showPassed = async (call = invoke) => {
  const generation = identityGeneration;
  let peers = [];
  let failed = false;
  try {
    peers = await call('passed_peers');
  } catch (_) {
    failed = true;
  }
  if (!identityCurrent(generation)) return;
  if (failed) privacyError.hidden = false;
  document.getElementById('passed-list').replaceChildren(...peers.map((peerId, index) => {
    const row = document.createElement('tr');
    const name = document.createElement('td');
    name.className = 'who';
    name.append(...who(row, peerId, null, ''));
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'btn';
    button.id = `unpass-${index + 1}`;
    button.textContent = window.strings['settings.unpass'];
    button.addEventListener('click', () => {
      window.passedShown = unpassPeer(peerId, button, call);
    });
    const action = document.createElement('td');
    action.append(button);
    row.append(name, action);
    return row;
  }));
  document.getElementById('passed-none').hidden = peers.length > 0;
};
