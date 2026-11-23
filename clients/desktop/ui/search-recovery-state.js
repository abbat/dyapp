/** Presentation request lifetime only; shared Rust owns eligibility, signing and delivery. */
/**
 * @typedef {{opening: object | null, busy: boolean, absent: boolean,
 * saved: import('./ipc.js').SearchRecoverySaved | null,
 * failure: string | null, operation: string | null}} RecoveryState
 */
/** @returns {RecoveryState} */
const initialState = () => ({
  opening: null,
  busy: false,
  absent: false,
  saved: null,
  failure: null,
  operation: null,
});
/** @param {RecoveryState} state @param {object} opening */
const beginPreparation = (state, opening) =>
  Object.assign(state, initialState(), {
    opening,
    busy: true,
    operation: 'prepare_search_recovery',
  });
/**
 * @param {{saved: import('./ipc.js').SearchRecoverySaved | null, failure: string | null}} state
 * @param {(command: 'confirm_search_recovery' | 'retry_search_recovery', opening: object | null) => Promise<void>} run
 */
const recoveryActions = (state, run) => ({
  /** @param {object | null} opening */
  confirm: (opening) =>
    state.saved === null && state.failure === null
      ? run('confirm_search_recovery', opening)
      : Promise.resolve(),
  /** @param {object | null} opening */
  retry: (opening) =>
    state.saved?.searchPending && (state.failure === null || state.failure === 'failed')
      ? run('retry_search_recovery', opening)
      : Promise.resolve(),
});
/** @param {import('./ipc.js').Invoke} call @param {string | null} token */
const dismissReview = async (call, token) => {
  if (token === null) return;
  try {
    await call('dismiss_search_recovery', { token });
  } catch {
    // An obsolete token cannot authorize another review; no action is retried here.
  }
};
/** @param {unknown} error */
const failureKind = (error) => {
  const kind = error && typeof error === 'object' && 'kind' in error ? error.kind : null;
  return typeof kind === 'string' &&
    [
      'closed',
      'changed',
      'reviewRequired',
      'linkPending',
      'versionExhausted',
      'wrongOwner',
    ].includes(kind)
    ? kind
    : 'failed';
};
/**
 * @param {import('./ipc.js').Invoke} call
 * @param {() => boolean} current
 * @param {() => void} changed
 */
export const searchRecoveryState = (call, current, changed) => {
  const state = initialState();
  let retired = false;
  /** @type {string | null} */
  let token = null;
  const live = () => !retired && current();
  /** @param {object | null} opening */
  const admits = (opening) => live() && !state.busy && opening === state.opening;
  /** @param {string | null} original */
  const dismiss = (original) => dismissReview(call, original);
  /** @param {object | null} opening */
  const isCurrentOpening = (opening) => live() && state.opening === opening;
  const retire = () => {
    retired = true;
    state.opening = null;
    const original = token;
    token = null;
    return dismiss(original);
  };
  /** @param {object | null} previous */
  const prepare = async (previous) => {
    if (!admits(previous)) return;
    const opening = {};
    const original = token;
    token = null;
    beginPreparation(state, opening);
    changed();
    try {
      await dismiss(original);
      if (!isCurrentOpening(opening)) return;
      const prepared = await call('prepare_search_recovery');
      if (!isCurrentOpening(opening)) {
        await dismiss(prepared);
        return;
      }
      token = prepared;
      state.absent = prepared === null;
    } catch (error) {
      if (isCurrentOpening(opening)) state.failure = failureKind(error);
    } finally {
      if (isCurrentOpening(opening)) {
        state.busy = false;
        changed();
      }
    }
  };
  /** @param {'confirm_search_recovery' | 'retry_search_recovery'} command @param {object | null} opening */
  const run = async (command, opening) => {
    const original = token;
    if (!admits(opening) || original === null) return;
    state.busy = true;
    state.operation = command;
    state.failure = null;
    changed();
    if (!isCurrentOpening(opening)) return;
    try {
      const saved = await call(command, { token: original });
      if (isCurrentOpening(opening)) state.saved = saved;
    } catch (error) {
      if (isCurrentOpening(opening)) state.failure = failureKind(error);
    } finally {
      if (isCurrentOpening(opening)) {
        state.busy = false;
        changed();
      }
    }
  };
  return { state, prepare, retire, ...recoveryActions(state, run) };
};
