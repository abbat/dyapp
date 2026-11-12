// A short label helps recognize a peer: "@" and the last 6 characters on every client, with no
// lengthening on a shared tail. The full ID remains the identifier.
/** @param {string} id */
export const shortPeerId = (id) => `@${id.slice(-6)}`;
