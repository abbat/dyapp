/**
 * @typedef {<Result = unknown>(command: string, args?: Record<string, unknown>) => Promise<Result>} Invoke
 */
// Tauri injects this host API before page modules run; lib.dom does not declare it.
const bridge = /** @type {{ __TAURI__: { core: { invoke: Invoke } } }} */ (
  /** @type {unknown} */ (window)
);
export const { invoke } = bridge.__TAURI__.core;
