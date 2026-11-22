/**
 * @param {(query: string, cursor: import('./ipc.js').CollectionCursor | null) =>
 *   Promise<import('./ipc.js').ReceivedViewPage>} read
 * @param {() => boolean} current
 * @param {() => void} changed
 */
export const receivedViewState = (read, current, changed) => {
  /** @type {{ page: import('./ipc.js').ReceivedViewPage | null, query: string,
   * cursor: import('./ipc.js').CollectionCursor | null, busy: boolean, failed: boolean }} */
  const state = { page: null, query: '', cursor: null, busy: false, failed: false };
  let sequence = 0;
  /** @param {number} attempt */
  const live = (attempt) => attempt === sequence && current();
  /** @param {import('./ipc.js').ReceivedViewPage | null} page */
  const admits = (page) => current() && !state.busy && page !== null && page === state.page;
  const cancel = () => {
    ++sequence;
    state.busy = false;
    changed();
  };
  /** @param {string} [query] @param {import('./ipc.js').CollectionCursor | null} [cursor] */
  const load = async (query = state.query, cursor = state.cursor) => {
    if (!current()) return;
    const attempt = ++sequence;
    if (query !== state.query) state.page = null;
    state.query = query;
    state.cursor = cursor;
    state.busy = true;
    state.failed = false;
    changed();
    try {
      const page = await read(query, cursor);
      if (!live(attempt)) return;
      state.page = page;
    } catch {
      if (live(attempt)) state.failed = true;
    } finally {
      if (live(attempt)) {
        state.busy = false;
        changed();
      }
    }
  };
  // These are UI request/lifetime guards. Core pages and opaque cursors pass through unchanged.
  return { state, load, cancel, admits };
};
