// A short label helps recognize a peer; the full ID remains the identifier.
/** @param {string} id @param {readonly string[]} known */
export const shortPeerId = (id, known) => {
  let length = Math.min(6, id.length);
  while (
    length < id.length &&
    known.some((other) => other !== id && other.endsWith(id.slice(-length)))
  ) {
    length += 1;
  }
  return `@${id.slice(-length)}`;
};

// The local identities are the only known peers until contacts exist.
/** @type {string[]} */
let knownPeers = [];
/** @template {{ peerId?: string }} Peer @param {Peer[]} items */
export const knowPeers = (items) => {
  knownPeers = items.flatMap((item) => (item.peerId ? [item.peerId] : []));
  return items;
};
/** @param {readonly { peerId: string }[]} items */
export const addKnownPeers = (items) => {
  knownPeers = [...new Set([...knownPeers, ...items.map((item) => item.peerId)])];
};
/** @param {string} id */
export const shortPeer = (id) => shortPeerId(id, knownPeers);

export { knownPeers };
