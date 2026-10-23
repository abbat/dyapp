# Abuse reports and legal requests

For operators of a `dyappd` node: what the node holds, how to take something down, and how to
answer a complaint or a lawful-access request. This is not legal advice; the law where the node
runs decides what you must do. The project has no abuse desk, no legal procedure and no appeal;
each operator answers for their own node ([threat model](../security/threat-model.md#illegal-content-and-legal-pressure)).

## What the node holds

All of it lives in `storage.dir` (`/var/lib/dyappd` in the package,
[backups](../manuals/bootstrap.md#backups) lists the files). Retention is set by the node's
`[limits]` ([privacy](../security/privacy.md#retention)):

| Data | Content | Kept |
|------|---------|------|
| Profiles (`profiles.db`) | Public, owner-signed profiles: peer ID, the fields the owner filled in, photo hashes | 30 days (`limits.profile_ttl_days`) after the owner's last publish or signed heartbeat, tombstone included |
| Search index (`profiles-idx.db`) | A copy of the network's profiles, with the search role | Rebuilt from profiles; disposable |
| Mailbox envelopes and signals (`messages.db`) | Mailbox address, sender key, put time, size and an opaque body (MLS ciphertext when the client uses `rust/client`) | Until the device acks or 24 hours pass (`limits.message_ttl_hours`); mailbox repair can give a copy a new TTL |
| Media (`media.db`, `data/`) | Avatars and chat attachments as whole blobs or Reed-Solomon shards, named by SHA-256 | Avatars while the owner's keep list holds them and the owner stays live; attachments until released or 168 hours (`limits.attachment_retention_hours`); after a release, its 32-byte release hash until that expiry |
| Deny list (`deny.db`) | Your entries with their notes and times | Until you remove them |
| Peer cache (`peers`, `anchors`) | Addresses of other nodes | Rebuilt from the network |

What it does not have:

- No request log: `dyappd` logs startup, storage errors, cleanup and maintenance, not individual
  requests, and no peer identifiers ([status and metrics](../manuals/bootstrap.md#status-and-metrics)).
  Client IP addresses are not stored by `dyappd`; a reverse proxy, firewall or the hosting
  provider may log them.
- No readable messages, once clients send through `rust/client`: the node cannot decrypt
  envelopes and cannot tell what is in them ([privacy](../security/privacy.md#field-visibility)).
- No account, phone number or e-mail: a user is a key.

A public profile shows what its owner published, to anyone: the node serves it to every client.

## Taking something down

Use the [deny list](../manuals/bootstrap.md#deny-list). It acts on this node only.

```bash
sudo -u dyappd dyappd deny add <sha256-of-blob> "report 2026-10-23"
sudo -u dyappd dyappd deny add <peer-id-or-key-hash> "report 2026-10-23"
sudo systemctl reload dyappd
```

- A **blob hash** removes the local copy, its manifest and its shards on reload, and refuses any
  later upload, download or repair of it here. Check it is gone: `dyappd deny list` shows the
  entry, the journal logs the purged hash, and `purged_blobs` in `dyappd status` went up.
- A **peer ID or key hash** refuses that user's profile, mailbox and media requests here
  (`REFUSED`). Data already stored stays until its retention runs out; the profile is no longer
  served, but its blobs are, by hash, until you list each hash too.
- Removing an entry restores nothing; the owner may upload again.

Other nodes keep their copies. One node's list removes no one from the network: the client
moves to another replica.

## Checking a report before acting

Abuse reports can be forged to make you cut off a user. Before you add an entry:

- Check it technically: the report names a blob hash, key or peer ID, and the claim fits what
  the node can hold. Do not download reported media yourself to check it; holding it may be
  illegal too.
- A report that names no hash, key or peer ID cannot be acted on: the node cannot find content
  by description, and nothing scans content.
- Never apply a deny list someone sends you unread. Deny-list exchange is off in the first
  release, and received lists are only stored for `dyappd deny received`; add entries one by one.
- The maintainers never ask for your node key, backups or logs. Anyone who does is not one of
  them.

## Answering a request

- Name what the node holds for the given identifier, by the table above, and nothing more.
  The node cannot search envelopes by sender name or content, and you should not build tools
  that scan.
- Envelopes are short-lived and unreadable to you; say so rather than handing over ciphertext you
  cannot interpret, unless the law requires the bytes.
- Deny-listing on your node is the only takedown you can perform; it does not reach other nodes.
- Keep a record of what you were asked and what you did, outside the node.
