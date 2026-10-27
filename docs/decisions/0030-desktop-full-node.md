# 0030. Desktop app runs node roles in process

- **Status:** Accepted
- **Date:** 2026-10-27

## Context

The network's capacity comes from `dyappd` nodes ([bootstrap](../architecture/bootstrap.md),
[search capacity](../architecture/search-capacity.md)). Desktop users often have spare disk and
bandwidth while the app is open, but the Linux app (`clients/linux`) is a light client: it depends
on `dyapp-client` and not on `dyapp-bootstrap`. Mobile devices have neither the battery nor the
background time for node work. A desktop behind NAT is useful to others only when AutoNAT confirms
it is reachable; hole punching and relays are separate work.

## Decision

The desktop app links `dyapp-bootstrap` as a library and, while it is open, runs a node with the
`store` and `search` roles on the active network, in its own task and under its own node key and
data directory next to the profile data. The node is off by default. A settings toggle turns it on
and sets its limits: disk (default 1 GiB, split between profiles, mailboxes and the search index)
and connections; everything else uses the `dyappd` defaults. Turning it off or closing the app
stops the node; its data stays until the user deletes it. The node is the same code as `dyappd`,
so the start sequence moves from the `dyappd` binary into the library and both call it. Media and
TURN roles stay on dedicated nodes. Mobile apps stay light clients.

## Consequences

- The network gains nodes without new hosts, and the node code has a second caller that keeps the
  start sequence a library function.
- The desktop binary grows by the node code; the size budget needs a `Size-Increase` reason once.
- The node is reachable only where AutoNAT says so; behind NAT it serves only the connections it
  opens until NAT traversal lands.
- A desktop node is up only while the app runs, so replication must keep tolerating churn.
- Windows and macOS desktops follow the same decision when they gain the client.
