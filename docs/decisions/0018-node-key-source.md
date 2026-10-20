# 0018. Node key source independent of its protection

- **Status:** Accepted
- **Date:** 2026-10-20

## Context

`dyappd` kept its libp2p key as `node.key` (mode 0600) in `storage.dir`. On a rented VM the
provider or anyone with the cloud account can copy the volume, its snapshots and backups, and
run a node under that peer ID ([threat model](../security/threat-model.md#cloud-host-access)).
Hosts differ: Debian 12 has systemd 252 with `systemd-creds` and often a vTPM, while a
container, a test node or another init system has neither. Encrypting the key inside `dyappd`
would need a passphrase prompt or a key store of its own for every one of these cases.

## Decision

`dyappd` reads the key from a file and does not interpret how it is protected. `storage.key`
names that file; empty, the node takes `$CREDENTIALS_DIRECTORY/node.key` when its systemd unit
passes that credential, else `storage.dir/node.key`. The group/other permission check applies to
a key file, not to a credential, which systemd makes readable to the service user only through
an ACL. `dyappd keygen -` writes the key to stdout and the peer ID to stderr, so the key can go
straight into another tool without a plain copy on disk.

The package default seals the key with `systemd-creds encrypt --with-key=auto` into
`/etc/credstore.encrypted/node.key`, bound to the host key and, when present, the TPM 2.0; the
unit has `LoadCredentialEncrypted=node.key`, which systemd 252 skips when the credential does
not exist, so a plain `node.key` of an older install keeps working. Without systemd or a TPM the
administrator chooses a 0600 file, a passphrase-encrypted copy decrypted to `tmpfs` before
start, or an encrypted volume.

## Consequences

- With a vTPM a copied volume alone does not yield the key. Without a TPM the host key lies on
  the same disk, so sealing then guards only a stray copy of the key file.
- A sealed key cannot move to another host: a backup is a copy the operator encrypts for
  themselves at keygen, or the node is replaced by a new one.
- No key-protection code or secret prompt in `dyappd`; new protections (a passphrase credential
  of a later systemd, a hardware token) need no change in the node.
- The key is still in memory while the node runs; memory dumps are covered by the unit's limits
  and confidential VMs only.
