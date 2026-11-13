// Readiness shared by controllers participating in the same UI operation.
/**
 * @type {{
 *   trayShown?: Promise<unknown>,
 *   versionShown?: Promise<unknown>,
 *   chatsDone?: Promise<unknown>,
 *   connectionShown?: Promise<unknown>,
 *   stickersShown?: Promise<unknown>,
 *   composerShown?: Promise<unknown>,
 *   editShown?: Promise<unknown>,
 *   passedShown?: Promise<unknown>,
 *   devicesShown?: Promise<unknown>,
 *   noteShown?: Promise<unknown>
 * }}
 */
export const progress = {};
