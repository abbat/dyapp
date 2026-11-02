# 0040. Device linking without history

- **Status:** Accepted
- **Date:** 2026-11-02

## Context

A new device is linked from an existing one ([ADR 0004](0004-identity-keys.md)), and
[ADR 0019](0019-device-directory-and-keypackage-rendezvous.md) has another device of the owner
sign a mailbox close with the identity key. Neither said which keys a linked device holds.

The identity key signs more than the device list. Today it also signs the profile, its heartbeat
(`rust/client/src/search.rs`), `media.keep`, and the `media.attach` of every chat photo
(`Session::send_attachment`). A profile whose owner signs nothing for `limits.profile_ttl_days`
(30) expires, and with it the device directory that contacts need. So a device without the
identity key cannot send a photo, edit the profile or revoke a device, and it stops being
reachable 30 days after the device holding the key goes silent.

The owner decided that v0.0.1 links devices with a QR code, one MLS leaf per device, fanout, a
signed device list with a rollback check, and revocation, and that no history or old media keys
move.

## Decision

**Every linked device holds the identity key.** Device and MLS keys stay per device, as ADR 0019
requires, so a device can be removed from groups and its mailbox closed.

**Linking.** It starts only from Devices in Settings on both sides.

1. The new device makes its device key and a one-use X25519 key, and shows a `dyapp-link` QR code
   with the public X25519 key and its mailbox address. A device without a camera takes the same
   code as text; it holds no secret, so it may be pasted.
2. The existing device reads the code, says that the other device gets full control of this
   identity, and waits for a confirmation. A contact QR scanner refuses a `dyapp-link` code.
3. It seals a link record to the X25519 key and puts it in that mailbox, in numbered parts that
   each fit the 100 KiB envelope limit. The record holds the identity key and the owner's local
   state: likes, passes, blocks, mutes, contact names, verified marks and whether the profile is
   hidden. The X25519 private key is erased once a record opens or the code expires; a code expires
   after five minutes.
4. The new device stores the identity key under its own platform store, fetches the newest
   profile from nodes, certifies its device, signs the owner binding, publishes the profile with
   itself added at a higher version, and publishes KeyPackages.
5. The existing device sees the new certificate and adds the device to every pair group. A device
   that finds a listed own device missing from a group it holds does the same, so a link survives
   the first device going offline. Two commits on one epoch fork a group for good and no server
   orders them, so own devices take turns by device key: the first in a group adds at once, and
   each later one waits an hour longer since it first saw that profile.

**History.** A linked device reads chats from the moment it joins. It gets no earlier messages or
media keys, and says so in each chat.

**Later changes.** Blocks, unlikes, mutes and names made after linking reach the other own devices
through a private MLS group of the owner's devices, with the event carrying no message content.

**Profile.** A device fetches the newest own profile before it publishes and never publishes a
lower version. It keeps the highest version seen, so a stale copy does not roll the list back.

**Revocation.** Any own device drops a certificate, publishes, closes the mailbox and removes the
device from groups. The dropped device wipes its keys and state on learning this.

## Consequences

- No node or wire change. `mailbox.put` takes any sender key and a tokenless envelope, so nodes
  carry the record as ordinary sealed mail; a device needs no delegated signature.
- A device can be lost without losing the identity, and any remaining device can revoke it.
- Every linked device is one more place to steal a key that never rotates. Revocation stops a
  stolen device that still runs the app, but not an attacker who extracted the key: they can add
  a device again. Contacts then see the new-device notice. The only full recovery is a new
  identity, and the Devices screen must say so.
- The QR code is the trust root. A code from an attacker gets the identity key from anyone who
  confirms, so the flow starts only from Devices and names the consequence before it sends.
- Linking is limited by the node's one profile update per 10 minutes and the profile proof of work.
- Contacts see a new-device notice and a verified contact loses the mark, as for any device change.
- A contact with a stale profile holds the commit that adds the new leaf until it fetches the
  newer one. Mail from a device truly dropped waits the same way, until the node expires it.
- Turns delay a link by an hour for each earlier own device that stays offline. They do not cover
  an earlier device returning exactly as a later turn commits, or a contact committing to the
  group at that moment: such a fork stays possible.
- X25519 sealing uses the HPKE of the MLS crypto provider, so it adds no direct dependency.
- History transfer stays possible later and needs its own decision.

## Alternatives and trade-offs

- **Only the first device holds the identity key.** A stolen linked device exposes no identity.
  Rejected unless nodes learn delegated signing for heartbeats, `media.keep` and `media.attach`,
  which is a wire change on every node: without it, linked devices cannot send photos or edit the
  profile and go dark 30 days after the first device is lost, and that loss cannot be revoked.
- **A symmetric secret in the QR code.** Simpler, but a photographed code then opens the record.
- **Link record over a local network.** No relay, but it fails across networks and needs
  discovery on every platform.
