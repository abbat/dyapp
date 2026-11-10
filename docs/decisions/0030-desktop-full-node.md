# 0030. Desktop app runs node roles in process

- **Status:** Accepted
- **Date:** 2026-10-27

## Context

The network's capacity comes from `dyappd` nodes ([bootstrap](../architecture/bootstrap.md),
[search capacity](../architecture/search-capacity.md)). Desktop users often have spare disk and
bandwidth while the app is open, but the Linux app (`clients/linux`) is a light client: it depends
on `dyapp-client` and not on `dyapp-bootstrap`. Mobile devices have neither the battery nor the
background time for node work. A desktop behind NAT is useful to others only when AutoNAT confirms
it is reachable; hole punching and relays are [ADR 0031](0031-circuit-relay-and-hole-punching.md).

## Decision

The desktop app links `dyapp-bootstrap` as a library and, while it is open, runs a node with the
`store` and `search` roles on the active network, in its own task and under its own node key and
data directory at the installation's app-config root, outside every profile folder. `node.json`
holds its settings; `node/` holds its key and stores; `node-config/network` holds its persisted
active-network settings. Bootstrap's distinct `node/network` marker binds the stores to a network
name and must never be overwritten with the identity seed-list format.
The node is off by default. A settings toggle turns it on
and sets its limits: disk (default 1 GiB, split between profiles, mailboxes and the search index)
and connections; everything else uses the `dyappd` defaults. Turning it off or closing the app
stops the node; its data stays until the user deletes it. The node is the same code as `dyappd`,
so the start sequence moves from the `dyappd` binary into the library and both call it. Media and
TURN roles stay on dedicated nodes. Mobile apps stay light clients.

The existing Linux network choice is retained: explicitly saving enabled node settings or
activating a network copies the selected profile's active network into `node-config/network`. No new
network-selection UI is introduced. Without an open profile, settings reuse the saved node
network; an enabled node without a saved network reports `noNetwork`. App startup restores this
configuration before an identity opens. Creating, selecting, linking or removing profiles never
stops or reconfigures the node. Only explicit node/network changes restart it, and application
exit waits for its stores to close. Node status and settings commands need no open identity;
reading the selected network takes only a short identity lease, released before node work.
No pre-release profile-node migration is provided.

## Consequences

- The network gains nodes without new hosts, and the node code has a second caller that keeps the
  start sequence a library function.
- The desktop binary grows by the node code; the size budget needs a `Size-Increase` reason once.
- The node is reachable only where AutoNAT says so; behind NAT it serves only the connections it
  opens until NAT traversal lands.
- A desktop node is up only while the app runs, so replication must keep tolerating churn.
- Linux, Windows and macOS share the embedded-node implementation. Unix keeps its 0600 key-file
  checks, root refusal and SIGHUP reload; Windows inherits key access control from the user's
  app-config directory, queries free space with `GetDiskFreeSpaceExW`, and refreshes deny lists
  on the common timer. The macOS Tauri client
  ([ADR 0002](0002-rust-core-native-apps-tauri-desktop.md)) uses the same Unix node/storage path
  as Linux; no separate daemon or Swift bridge is required.
