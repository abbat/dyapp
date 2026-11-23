export let searchRecoveryFixture = null;

/** Named core outcomes only: no purpose inference, version arithmetic, signing or network access. */
export const installSearchRecoveryFixture = (strings, screen) => {
  if (!screen.startsWith('search-recovery-')) return;
  const card = document.getElementById('search-recovery');
  const status = document.getElementById('search-recovery-status');
  const action = document.getElementById('search-recovery-action');
  const cancel = document.getElementById('search-recovery-cancel');
  const state = {
    active: true,
    present: true,
    stage: screen.slice('search-recovery-'.length),
    confirmations: screen.endsWith('-busy') ? 1 : 0,
    retries: 0,
    reads: 0,
  };
  let attempt = state.stage === 'busy' ? { kind: 'confirm' } : null;
  document.getElementById('search-recovery-owner').textContent = document
    .getElementById('peer-id')
    .textContent.trim();
  const render = () => {
    card.hidden = !state.present;
    status.textContent = strings[`searchRecovery.${state.stage}`];
    const retry = ['pending', 'unsupported'].includes(state.stage);
    const refresh = ['stale', 'error'].includes(state.stage);
    action.textContent =
      strings[
        retry
          ? 'searchRecovery.retry'
          : refresh
            ? 'searchRecovery.refresh'
            : 'searchRecovery.confirm'
      ];
    action.hidden = state.stage === 'sent';
    action.disabled = state.stage === 'busy';
    cancel.disabled = state.stage === 'busy';
    cancel.textContent =
      strings[state.stage === 'review' ? 'action.cancel' : 'searchRecovery.close'];
  };
  const begin = () => {
    if (!state.active || !state.present || state.stage === 'busy' || state.stage === 'sent') return;
    const kind = ['pending', 'unsupported'].includes(state.stage)
      ? 'retry'
      : ['stale', 'error'].includes(state.stage)
        ? 'read'
        : 'confirm';
    if (kind === 'confirm') state.confirmations++;
    if (kind === 'retry') state.retries++;
    if (kind === 'read') state.reads++;
    attempt = { kind };
    state.stage = 'busy';
    render();
  };
  const complete = (stage) => {
    if (!state.active || !state.present || !attempt) return;
    attempt = null;
    state.stage = stage;
    render();
  };
  const dismiss = () => {
    if (!state.active || state.stage === 'busy') return;
    state.present = false;
    render();
  };
  action.addEventListener('click', begin);
  cancel.addEventListener('click', dismiss);
  searchRecoveryFixture = {
    state,
    begin,
    complete,
    dismiss,
    retire: () => {
      state.active = false;
      attempt = null;
    },
  };
  render();
};
