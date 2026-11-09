// A short label helps recognize a peer; the full ID remains the identifier.
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
let knownPeers = [];
export const knowPeers = (items) => {
  knownPeers = items.filter((item) => item.peerId).map((item) => item.peerId);
  return items;
};
export const addKnownPeers = (items) => {
  knownPeers = [...new Set([...knownPeers, ...items.map((item) => item.peerId)])];
};
export const shortPeer = (id) => shortPeerId(id, knownPeers);
