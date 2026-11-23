import { searchRecoveryState } from './search-recovery-state.js';

/** @typedef {ReturnType<typeof searchRecoveryState>} RecoveryModel */
/** @param {HTMLElement} root @param {string} peer @param {Record<string, string>} words */
const cardElements = (root, peer, words) => {
  const heading = document.createElement('h2');
  heading.id = `${root.id}-title`;
  heading.textContent = words['searchRecovery.title'];
  root.setAttribute('aria-labelledby', heading.id);
  root.classList.add('card');
  const identity = document.createElement('dl');
  identity.className = 'confirm-id';
  const label = document.createElement('dt');
  label.id = `${root.id}-owner-label`;
  label.textContent = words['searchRecovery.owner'];
  const owner = document.createElement('code');
  owner.id = `${root.id}-owner`;
  owner.className = 'full-id';
  owner.textContent = peer;
  const value = document.createElement('dd');
  value.append(owner);
  identity.append(label, value);
  const copy = document.createElement('button');
  copy.id = `${root.id}-copy`;
  copy.type = 'button';
  copy.className = 'btn';
  copy.textContent = words['action.copy'];
  const status = document.createElement('p');
  status.id = `${root.id}-status`;
  status.setAttribute('role', 'status');
  status.tabIndex = -1;
  const actions = document.createElement('div');
  actions.className = 'actions';
  const action = document.createElement('button');
  action.id = `${root.id}-action`;
  action.type = 'button';
  action.className = 'btn';
  const close = document.createElement('button');
  close.id = `${root.id}-close`;
  close.type = 'button';
  close.className = 'btn';
  actions.append(close, action);
  const ownerActions = document.createElement('div');
  ownerActions.className = 'actions field';
  ownerActions.append(copy);
  root.replaceChildren(heading, identity, ownerActions, status, actions);
  return { owner, copy, status, action, close };
};
/** @param {RecoveryModel['state']} state */
const messageKey = (state) => {
  if (state.busy) {
    const operation = {
      prepare_search_recovery: 'reading',
      confirm_search_recovery: 'saving',
      retry_search_recovery: 'sending',
    };
    return operation[/** @type {keyof typeof operation} */ (state.operation)];
  }
  if (state.absent) return 'none';
  if (state.failure) return `failure.${state.failure}`;
  if (state.saved) return state.saved.searchPending ? 'pending' : 'sent';
  return 'review';
};
/** @param {RecoveryModel['state']} state */
const nextAction = (state) => {
  if (state.absent) return null;
  if (state.saved?.searchPending && (state.failure === null || state.failure === 'failed'))
    return 'retry';
  if (state.failure) return 'refresh';
  return state.saved === null ? 'confirm' : null;
};
/**
 * @param {RecoveryModel} model @param {ReturnType<typeof cardElements>} nodes
 * @param {Record<string, string>} words
 */
const renderCard = (model, nodes, words) => {
  const { state } = model;
  const next = nextAction(state);
  nodes.status.textContent = words[`searchRecovery.${messageKey(state)}`];
  nodes.action.hidden = next === null;
  nodes.action.disabled = state.busy;
  nodes.action.textContent = next === null ? '' : words[`searchRecovery.${next}`];
  nodes.close.disabled = state.busy && state.operation !== 'prepare_search_recovery';
  nodes.close.textContent =
    state.absent || state.saved || state.failure
      ? words['searchRecovery.close']
      : words['action.cancel'];
  return next;
};
/**
 * @param {RecoveryModel} model @param {ReturnType<typeof cardElements>} nodes
 * @param {ReturnType<typeof nextAction>} next
 * @param {(opening: object | null) => boolean} shown @param {() => Promise<void>} retire
 */
const bindRecoveryActions = (model, nodes, next, shown, retire) => {
  const opening = model.state.opening;
  nodes.action.onclick = () => {
    if (!shown(opening) || model.state.busy) return;
    if (next === 'confirm') void model.confirm(opening);
    else if (next === 'retry') void model.retry(opening);
    else if (next === 'refresh') void model.prepare(opening);
  };
  nodes.close.onclick = () => {
    if (shown(opening) && !nodes.close.disabled) void retire();
  };
};
/** @param {HTMLElement} root @param {RecoveryModel} model */
const closeCard = (root, model) => {
  const focused = root.contains(document.activeElement);
  root.hidden = true;
  const heading = root.parentElement?.querySelector('h1');
  if (focused && heading) {
    heading.tabIndex = -1;
    heading.focus();
  }
  return model.retire();
};
/**
 * @param {RecoveryModel['state']} state
 * @param {import('./ipc.js').SearchRecoverySaved | null | undefined} reported
 */
const hasNewOutcome = (state, reported) =>
  !state.busy && (state.saved !== null || state.absent) && reported !== state.saved;
/**
 * Shared native/prototype presentation. It never reloads or writes the profile form.
 * @param {HTMLElement} root @param {string} peer @param {import('./ipc.js').Invoke} call
 * @param {() => boolean} current @param {Record<string, string>} words
 * @param {(saved: import('./ipc.js').SearchRecoverySaved | null) => void} [updated]
 */
export const searchRecoveryCard = (root, peer, call, current, words, updated = () => {}) => {
  const nodes = cardElements(root, peer, words);
  let retired = false;
  let shownOnce = false;
  /** @type {import('./ipc.js').SearchRecoverySaved | null | undefined} */
  let reported;
  /** @type {RecoveryModel} */
  let model;
  /** @param {object | null} opening */
  const shown = (opening) =>
    !retired &&
    current() &&
    model.state.opening === opening &&
    root.getClientRects().length > 0 &&
    !document.hidden;
  const retire = () => {
    retired = true;
    return closeCard(root, model);
  };
  const render = () => {
    if (retired || !current()) return;
    const focused = [nodes.action, nodes.close, nodes.status].some(
      (node) => node === document.activeElement,
    );
    if (!model.state.busy && !model.state.absent) shownOnce = true;
    root.hidden = !shownOnce;
    const next = renderCard(model, nodes, words);
    const opening = model.state.opening;
    bindRecoveryActions(model, nodes, next, shown, retire);
    if (hasNewOutcome(model.state, reported)) {
      reported = model.state.saved;
      updated(reported);
    }
    if (focused && shown(opening)) {
      (model.state.busy ? nodes.status : nodes.close).focus();
    }
  };
  model = searchRecoveryState(call, current, render);
  nodes.copy.onclick = async () => {
    const opening = model.state.opening;
    if (!shown(opening)) return;
    try {
      await navigator.clipboard.writeText(peer);
      if (shown(opening)) nodes.copy.textContent = words['action.copied'];
    } catch {
      if (shown(opening)) window.getSelection()?.selectAllChildren(nodes.owner);
    }
  };
  root.hidden = true;
  return { model, prepare: () => model.prepare(model.state.opening), retire };
};
