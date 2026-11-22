// Presentation owns one explicit opening; the shared Rust core owns every disclosure policy.
/**
 * @param {string} peerId
 * @param {import('./ipc.js').Invoke} call
 * @param {() => boolean} current
 */
export const profileViewOpening = (peerId, call, current) => {
  let retired = false;
  /** @type {string | null} */
  let token = null;
  const retire = async () => {
    retired = true;
    const original = token;
    token = null;
    if (original !== null) {
      try {
        await call('retire_profile_view', { token: original });
      } catch {
        // Retirement is idempotent; an obsolete token cannot authorize a new opening.
      }
    }
  };
  const done = (async () => {
    try {
      if (!current()) return;
      const prepared = await call('prepare_profile_view', { peerId });
      if (typeof prepared !== 'string' || !prepared) return;
      token = prepared;
      if (retired || !current()) {
        await retire();
        return;
      }
      await call('submit_profile_view', { token: prepared });
    } catch {
      // An optional pre-commit refusal never triggers a second submission or a guessed queue.
    }
  })();
  return { retire, done };
};
