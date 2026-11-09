import { all as domAll, byId as domById } from './dom.js';
import { showArea, wizardNetwork, show, identity } from './app.js';
import { copyFrom } from './clipboard.js';
import { identityState, identityCurrent } from './identity-state.js';
import { invoke } from './ipc.js';
import { startLink } from './link.js';
import { strings } from './locale.js';
import { progress } from './ui-progress.js';
export const controller = {};
/** @type {Promise<void> | undefined} */
controller.connectionChecked = undefined;

// The Network area and the shell's connection status use explicitly imported helpers.
// Bootstrap settings of the open identity: draft rows and one bounded Check and connect.
const network = domById('network', HTMLFormElement);
const seeds = domById('seeds', HTMLTextAreaElement);
const networkName = domById('network-name', HTMLInputElement);
const checkButton = domById('check-connection', HTMLButtonElement);
const cancelButton = domById('cancel-check', HTMLButtonElement);
const seedRows = domById('seed-rows', HTMLOListElement);
const networkError = domById('network-error', HTMLDivElement);
const connectionStatus = domById('connection-status', HTMLButtonElement);
// Only the latest launch connect or check may set the shell status.
let connectionRun = 0;
let checkGeneration = identityState.generation;
/** @param {undefined | string} [name] */
const showConnection = (state, name) => {
  connectionStatus.dataset.state = state;
  for (const dot of domAll(document, '.connection-dot', HTMLElement)) {
    dot.classList.toggle('busy', state === 'connecting');
    dot.classList.toggle('off', state === 'offline');
  }
  for (const label of domAll(document, '.connection-label', HTMLElement)) {
    label.textContent = strings[`connection.${state}`];
  }
  // The rail hides the label, so the title names the button there.
  connectionStatus.title = strings[`connection.${state}`];
  if (name !== undefined) {
    domById('connection-network', HTMLSpanElement).textContent =
      name || strings['network.name.public'];
  }
};
connectionStatus.addEventListener('click', () => showArea('network-area'));
// Dials the saved active network as the identity opens; nothing active means Offline.
const connectAtLaunch = async (call = invoke) => {
  const generation = identityState.generation;
  const run = ++connectionRun;
  const active = await call('load_network', { file: 'active' }).catch(() => null);
  if (!identityCurrent(generation) || run !== connectionRun) return;
  if (!active) {
    showConnection('offline');
    return;
  }
  showConnection('connecting', active.name);
  const answered = await call('connect_network', { waitMs: 15000 }).catch(() => false);
  if (identityCurrent(generation) && run === connectionRun) {
    showConnection(answered ? 'connected' : 'offline');
  }
};
/** @param {string} key @param {undefined | string} [detail] */
const networkStatus = (key, detail) => {
  const text = strings[key];
  domById('network-status', HTMLParagraphElement).textContent = detail ? `${text} ${detail}` : text;
};
const draft = () => ({
  seeds: seeds.value
    .split('\n')
    .map((row) => row.trim())
    .filter(Boolean),
  name: networkName.value,
});

// Fills the form from the draft, else the active settings; a damaged file is shown, not reset.
const showNetwork = async (call = invoke) => {
  const generation = identityState.generation;
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
    const saved = (await call('load_network', { file: 'draft' })) || active;
    if (!identityCurrent(generation)) return;
    seeds.value = saved ? saved.seeds.join('\n') : '';
    networkName.value = saved ? saved.name : '';
    networkStatus(active ? 'network.active' : 'network.idle');
  } catch (error) {
    if (identityCurrent(generation)) networkStatus('network.failed', String(error));
  }
};
controller.showNetwork = showNetwork;

const rowItem = (seed, row) => {
  const entry = document.createElement('li');
  entry.dataset.status = row.status;
  entry.textContent = `${seed}: ${strings[`network.row.${row.status}`]}`;
  // The node ID or the transport error is for an operator, so it stays collapsed.
  const detail = row.peerId || row.reason;
  if (detail) {
    const more = document.createElement('details');
    const label = document.createElement('summary');
    const code = document.createElement('pre');
    label.textContent = strings['network.details'];
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
/** @param {string} code */
const showNetworkError = (code, lines = []) => {
  domById('network-error-details', HTMLPreElement).textContent = [code, ...lines].join('\n');
  domById('network-error-more', HTMLDetailsElement).open = false;
  networkError.hidden = false;
};

/**
 * @param {{ seeds: string[]; name: string; }} rows
 * @param {import('./ipc.js').NetworkCheckRow[]} result
 */
const renderNetworkCheck = (rows, result) => {
  seedRows.replaceChildren(...result.map((row, at) => rowItem(rows.seeds[at], row)));
  networkStatus(summary(result));
  if (result.some((row) => row.status === 'connected')) return true;
  showNetworkError(
    summary(result).slice('network.'.length),
    result.map((row, at) =>
      [`${rows.seeds[at]}:`, row.status, row.reason].filter(Boolean).join(' '),
    ),
  );
  return false;
};

/** @param {string | undefined} [before] */
const restoreNetworkCheck = (before) => {
  if (connectionStatus.dataset.state === 'connecting') {
    if (before === 'connecting') progress.connectionShown = connectAtLaunch();
    else showConnection(before);
  }
  checkButton.disabled = false;
  cancelButton.hidden = true;
};

// A list a node of the network answered becomes active at once; anything else keeps the
// previous network and its shell status.
const check = async (call = invoke) => {
  const rows = draft();
  const generation = identityState.generation;
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
    if (!renderNetworkCheck(rows, result)) return;
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
    if (live()) restoreNetworkCheck(before);
  }
};

network.addEventListener('submit', (event) => {
  event.preventDefault();
  controller.connectionChecked = check();
});
// Edits are kept even if the app closes before a check.
const saveNetworkDraft = (call = invoke) => {
  const generation = identityState.generation;
  return call('save_network_draft', { draft: draft() }).catch((error) => {
    if (identityCurrent(generation)) networkStatus('network.failed', String(error));
  });
};
network.addEventListener('change', () => {
  saveNetworkDraft();
});
cancelButton.addEventListener('click', () => invoke('cancel_connection_check'));
// First-run step 2: the shell's Peer ID and Network form move into the wizard and back.
const peerCode = domById('peer-id', HTMLElement);
const peerCard = peerCode.parentElement;
const skipNetwork = domById('skip-network', HTMLButtonElement);
const linkNetworkCancel = domById('link-network-cancel', HTMLButtonElement);
// `on` is 'link' for the step of a link: that identity is about to go, so no Peer ID and no Skip.
/** @param {boolean | 'link'} on */
const wizardStep = (on) => {
  const linking = on === 'link';
  (on ? domById('wizard-peer', HTMLParagraphElement) : peerCard).append(peerCode);
  (on ? domById('wizard-form', HTMLDivElement) : domById('network-area', HTMLElement)).append(
    network,
  );
  network.classList.toggle('card', !on);
  skipNetwork.hidden = !on || linking;
  linkNetworkCancel.hidden = !linking;
  domById('wizard-peer', HTMLParagraphElement).hidden = linking;
  domById('wizard-step', HTMLDivElement).hidden = linking;
  domById('wizard-link-step', HTMLDivElement).hidden = !linking;
};
const leaveWizard = () => {
  if (wizardNetwork.hidden) return;
  wizardStep(false);
  show(identity, 'status.ready');
};
skipNetwork.addEventListener('click', leaveWizard);
linkNetworkCancel.addEventListener('click', leaveWizard);
copyFrom(domById('copy-peer-id', HTMLButtonElement), peerCode);
copyFrom(
  domById('copy-network-details', HTMLButtonElement),
  domById('network-error-details', HTMLPreElement),
);

export {
  cancelButton,
  check,
  checkButton,
  connectAtLaunch,
  connectionStatus,
  networkName,
  saveNetworkDraft,
  seeds,
  showNetwork,
  wizardStep,
};
