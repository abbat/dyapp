// The Network area and the shell's connection status; loaded after app.js, whose helpers
// (invoke, show, showArea, copyFrom) it shares through the page's global scope.
// Bootstrap settings of the open identity: draft rows and one bounded Check and connect.
const network = document.getElementById('network');
const seeds = document.getElementById('seeds');
const networkName = document.getElementById('network-name');
const checkButton = document.getElementById('check-connection');
const cancelButton = document.getElementById('cancel-check');
const seedRows = document.getElementById('seed-rows');
const networkError = document.getElementById('network-error');
const connectionStatus = document.getElementById('connection-status');
// Only the latest launch connect or check may set the shell status.
let connectionRun = 0;
let checkGeneration = identityGeneration;
const showConnection = (state, name) => {
  connectionStatus.dataset.state = state;
  for (const dot of document.querySelectorAll('.connection-dot')) {
    dot.classList.toggle('busy', state === 'connecting');
    dot.classList.toggle('off', state === 'offline');
  }
  for (const label of document.querySelectorAll('.connection-label')) {
    label.textContent = window.strings[`connection.${state}`];
  }
  // The rail hides the label, so the title names the button there.
  connectionStatus.title = window.strings[`connection.${state}`];
  if (name !== undefined) {
    document.getElementById('connection-network').textContent = name
      || window.strings['network.name.public'];
  }
};
connectionStatus.addEventListener('click', () => showArea('network-area'));
// Dials the saved active network as the identity opens; nothing active means Offline.
const connectAtLaunch = async (call = invoke) => {
  const generation = identityGeneration;
  const run = ++connectionRun;
  const active = await call('load_network', { file: 'active' }).catch(() => null);
  if (!identityCurrent(generation) || run !== connectionRun) return;
  if (!active) { showConnection('offline'); return; }
  showConnection('connecting', active.name);
  const answered = await call('connect_network', { waitMs: 15000 }).catch(() => false);
  if (identityCurrent(generation) && run === connectionRun) {
    showConnection(answered ? 'connected' : 'offline');
  }
};
const networkStatus = (key, detail) => {
  const text = window.strings[key];
  document.getElementById('network-status').textContent = detail ? `${text} ${detail}` : text;
};
const draft = () => ({
  seeds: seeds.value.split('\n').map((row) => row.trim()).filter(Boolean),
  name: networkName.value,
});

// Fills the form from the draft, else the active settings; a damaged file is shown, not reset.
const showNetwork = async (call = invoke) => {
  const generation = identityGeneration;
  if (checkGeneration !== generation) {
    checkGeneration = generation;
    checkButton.disabled = false;
    cancelButton.hidden = true;
  }
  seedRows.replaceChildren();
  networkError.hidden = true;
  try {
    const active = await call('load_network', { file: 'active' });
    if (!identityCurrent(generation)) return;
    const saved = await call('load_network', { file: 'draft' }) || active;
    if (!identityCurrent(generation)) return;
    seeds.value = saved ? saved.seeds.join('\n') : '';
    networkName.value = saved ? saved.name : '';
    networkStatus(active ? 'network.active' : 'network.idle');
  } catch (error) {
    if (identityCurrent(generation)) networkStatus('network.failed', String(error));
  }
};
window.showNetwork = showNetwork;

const rowItem = (seed, row) => {
  const entry = document.createElement('li');
  entry.dataset.status = row.status;
  entry.textContent = `${seed}: ${window.strings[`network.row.${row.status}`]}`;
  // The node ID or the transport error is for an operator, so it stays collapsed.
  const detail = row.peerId || row.reason;
  if (detail) {
    const more = document.createElement('details');
    const label = document.createElement('summary');
    const code = document.createElement('pre');
    label.textContent = window.strings['network.details'];
    code.textContent = detail;
    more.append(label, code);
    entry.append(more);
  }
  return entry;
};

// A row-level problem the user must correct outranks nodes that are merely down.
const summary = (rows) => {
  const has = (...codes) => rows.some((row) => codes.includes(row.status));
  if (has('connected')) return 'network.connected';
  if (has('invalidAddress', 'invalidPeerId', 'duplicate')) return 'network.invalidAddress';
  if (has('wrongPeer', 'wrongNetwork')) return 'network.wrong';
  return 'network.unreachable';
};

// The collapsed details name the outcome code and every row, for the network operator.
const showNetworkError = (code, lines = []) => {
  document.getElementById('network-error-details').textContent = [code, ...lines].join('\n');
  document.getElementById('network-error-more').open = false;
  networkError.hidden = false;
};

// A list a node of the network answered becomes active at once; anything else keeps the
// previous network and its shell status.
const check = async (call = invoke) => {
  const rows = draft();
  const generation = identityGeneration;
  const run = ++connectionRun;
  const live = () => identityCurrent(generation) && run === connectionRun;
  const before = connectionStatus.dataset.state;
  checkButton.disabled = true;
  cancelButton.hidden = false;
  networkError.hidden = true;
  seedRows.replaceChildren(...rows.seeds.map((seed) => rowItem(seed, { status: 'checking' })));
  networkStatus('network.checking');
  showConnection('connecting');
  try {
    const result = await call('check_connection', { draft: rows, waitMs: 15000 });
    if (!live()) return;
    seedRows.replaceChildren(...result.map((row, at) => rowItem(rows.seeds[at], row)));
    networkStatus(summary(result));
    if (!result.some((row) => row.status === 'connected')) {
      showNetworkError(summary(result).slice('network.'.length), result.map((row, at) => [
        `${rows.seeds[at]}:`, row.status, row.reason].filter(Boolean).join(' ')));
      return;
    }
    await call('activate_connection');
    if (!live()) return;
    networkStatus('network.active');
    showConnection('connected', rows.name);
    // A node that waited for a network starts on it now.
    call('apply_node').catch(() => {});
    const linking = !linkNetworkCancel.hidden;
    leaveWizard();
    if (linking) startLink();
  } catch (error) {
    if (!live()) return;
    const [code, ...reason] = String(error).split(': ');
    if (['empty', 'cancelled', 'invalidNetwork'].includes(code)) {
      networkStatus(`network.${code}`, reason.join(': '));
    } else {
      networkStatus('network.failed', String(error));
      showNetworkError(String(error));
    }
  } finally {
    // A check that interrupted the launch connect leaves nothing to restore, so that dial reruns.
    if (live()) {
      if (connectionStatus.dataset.state === 'connecting') {
        if (before === 'connecting') window.connectionShown = connectAtLaunch();
        else showConnection(before);
      }
      checkButton.disabled = false;
      cancelButton.hidden = true;
    }
  }
};

network.addEventListener('submit', (event) => {
  event.preventDefault();
  window.connectionChecked = check();
});
// Edits are kept even if the app closes before a check.
const saveNetworkDraft = (call = invoke) => {
  const generation = identityGeneration;
  return call('save_network_draft', { draft: draft() })
    .catch((error) => {
      if (identityCurrent(generation)) networkStatus('network.failed', String(error));
    });
};
network.addEventListener('change', () => { saveNetworkDraft(); });
cancelButton.addEventListener('click', () => invoke('cancel_connection_check'));
// First-run step 2: the shell's Peer ID and Network form move into the wizard and back.
const peerCode = document.getElementById('peer-id');
const peerCard = peerCode.parentElement;
const skipNetwork = document.getElementById('skip-network');
const linkNetworkCancel = document.getElementById('link-network-cancel');
// `on` is 'link' for the step of a link: that identity is about to go, so no Peer ID and no Skip.
const wizardStep = (on) => {
  const linking = on === 'link';
  (on ? document.getElementById('wizard-peer') : peerCard).append(peerCode);
  (on ? document.getElementById('wizard-form') : document.getElementById('network-area'))
    .append(network);
  network.classList.toggle('card', !on);
  skipNetwork.hidden = !on || linking;
  linkNetworkCancel.hidden = !linking;
  document.getElementById('wizard-peer').hidden = linking;
  document.getElementById('wizard-step').hidden = linking;
  document.getElementById('wizard-link-step').hidden = !linking;
};
const leaveWizard = () => {
  if (wizardNetwork.hidden) return;
  wizardStep(false);
  show(identity, 'status.ready');
};
skipNetwork.addEventListener('click', leaveWizard);
linkNetworkCancel.addEventListener('click', leaveWizard);
copyFrom(document.getElementById('copy-peer-id'), peerCode);
copyFrom(document.getElementById('copy-network-details'),
  document.getElementById('network-error-details'));
