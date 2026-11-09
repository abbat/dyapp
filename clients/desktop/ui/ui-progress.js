// Readiness shared by controllers participating in the same UI operation.
/**
 * @type {{
 *   trayShown?: Promise<unknown>,
 *   chatsDone?: Promise<unknown>,
 *   connectionShown?: Promise<unknown>,
 *   stickersShown?: Promise<unknown>,
 *   passedShown?: Promise<unknown>,
 *   devicesShown?: Promise<unknown>,
 *   noteShown?: Promise<unknown>
 * }}
 */
export const progress = {};
