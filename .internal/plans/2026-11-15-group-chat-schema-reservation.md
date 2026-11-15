# Group Chat Schema Reservation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use beads-superpowers:subagent-driven-development (recommended) or beads-superpowers:executing-plans to implement this plan task-by-task. Each Task becomes a bead (`bd create -t task --parent <epic-id>`). Steps within tasks use checkbox (`- [ ]`) syntax for human readability.

**Goal:** The v0.0.1 client's initial schema creates seven empty group chat tables and three
indexes, so group chats can ship later without a client database migration.

**Architecture:** A new module `rust/client/src/store/groups.rs` holds only `SCHEMA` (same
pattern as `reads.rs`) and one scenario test. `create_schema` in `store.rs` runs it together
with the other module schemas. No code reads or writes these tables yet. Design:
`.internal/specs/2026-11-15-group-chat-schema-design.md`.

**Tech Stack:** Rust, rusqlite, SQLite STRICT tables.

## Global Constraints

- `user_version` stays 1; add no migrations.
- Do not touch the bootstrap.
- One commit, at most 400 lines; title at most 72 characters, imperative mood.
- Commit trailers: `ADR: docs/decisions/0025-client-local-sqlite-storage.md` (the commit
  changes SQL schema) and `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- English in code, comments, docs and commit messages; no bead IDs in any of them.
- No new dependencies.
- The work is done when CI is green, not when a local run passes.

## File Structure

- Create: `rust/client/src/store/groups.rs` holding `SCHEMA` and its test.
- Modify: `rust/client/src/store.rs`: declare `mod groups;` and add `groups::SCHEMA` to
  `create_schema`.
- Modify: `docs/design/client-local-storage.md`: add seven rows to the table catalogue after
  `read_marks`.

---

### Task 1: Reserve group chat tables in the client schema

**Files:**
- Create: `rust/client/src/store/groups.rs`
- Modify: `rust/client/src/store.rs` (module list near line 21; `create_schema` near line 101)
- Modify: `docs/design/client-local-storage.md` (table catalogue, after the `read_marks` row)

**Interfaces:**
- Consumes: `super::tests::temp_db_path`, `Store::open`, `Store.db` (test access, as in
  `unknown_setting_format_fails_closed`).
- Produces: `pub(super) const SCHEMA: &str` in `store::groups`. No other API.

**Acceptance Criteria:**
- A fresh `Store::open` creates the tables `chat_groups`, `chat_group_members`,
  `chat_group_links`, `chat_group_cursors`, `chat_group_mentions`, `chat_group_events` and
  `mls_group_state`, and the indexes `chat_group_owner`, `chat_group_members_peer` and
  `chat_group_events_feed`, matching the spec's SQL.
- `user_version` stays 1; the newer-version rejection test passes unchanged.
- The scenario test accepts realistic rows in all seven tables. It rejects role 4, state 7, a
  65-byte group name, a pin with one NULL half, a second current owner, a 31-byte topic, a
  negative `seq`, a 15-byte `link_id`, `via_link` with `source = 1`, `kind` 0 and a 15-byte
  mention `message_id`.
- The catalogue in `docs/design/client-local-storage.md` lists the seven tables marked
  "Reserved, not used by code yet".
- The bootstrap is unchanged; one commit of at most 400 lines with the ADR trailer.

- [ ] **Step 1: Write the failing test**

Create `rust/client/src/store/groups.rs` with an empty schema and the test:

```rust
//! Reserved group chat tables. The v0.0.1 client creates them empty and never reads or writes
//! them; they exist so group chats ship later without a schema migration.

pub(super) const SCHEMA: &str = "";

#[cfg(test)]
mod tests {
    use super::super::{tests::temp_db_path, Store};
    use dyapp_identity::Identity;

    const LINK: &str = "x'000102030405060708090a0b0c0d0e0f'";

    #[test]
    fn reserved_group_tables_accept_realistic_rows_and_reject_bad_ones() {
        let store = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
        let run = |sql: &str| store.db.execute_batch(sql);
        for sql in [
            "INSERT INTO `chat_groups` (`group_id`, `state`, `epoch`) VALUES (x'01', 2, 0)".to_owned(),
            "INSERT INTO `chat_group_members` (`group_id`, `peer_id`, `role`, `state`, `source`, `since_epoch`) \
             VALUES (x'01', 'alice', 1, 2, 1, 0)".to_owned(),
            format!(
                "INSERT INTO `chat_group_members` (`group_id`, `peer_id`, `role`, `state`, `source`, `via_link`, `since_epoch`) \
                 VALUES (x'01', 'bob', 3, 5, 2, {LINK}, 1)"
            ),
            format!(
                "INSERT INTO `chat_group_links` (`group_id`, `link_id`, `sealed_secret`, `created_by`, `created_epoch`) \
                 VALUES (x'01', {LINK}, x'aa', 'alice', 0)"
            ),
            "INSERT INTO `chat_group_cursors` VALUES (x'01', zeroblob(32), 'node', 0, 0)".to_owned(),
            "INSERT INTO `chat_group_mentions` VALUES (x'01', 'bob', zeroblob(16), 1)".to_owned(),
            "INSERT INTO `chat_group_events` (`group_id`, `epoch`, `kind`, `actor`, `target`, `after_arrival`, `at_ms`) \
             VALUES (x'01', 1, 2, 'alice', 'bob', 0, 1)".to_owned(),
            "INSERT INTO `mls_group_state` VALUES (x'01', x'aa', 0)".to_owned(),
        ] {
            run(&sql).unwrap_or_else(|error| panic!("{sql}: {error}"));
        }
        for sql in [
            "INSERT INTO `chat_group_members` (`group_id`, `peer_id`, `role`, `state`, `source`, `since_epoch`) \
             VALUES (x'01', 'carol', 4, 2, 1, 0)".to_owned(),
            "INSERT INTO `chat_group_members` (`group_id`, `peer_id`, `role`, `state`, `source`, `since_epoch`) \
             VALUES (x'01', 'carol', 3, 7, 1, 0)".to_owned(),
            "UPDATE `chat_groups` SET `name` = substr(hex(zeroblob(33)), 1, 65)".to_owned(),
            "UPDATE `chat_groups` SET `pinned_author` = 'alice'".to_owned(),
            "INSERT INTO `chat_group_members` (`group_id`, `peer_id`, `role`, `state`, `source`, `since_epoch`) \
             VALUES (x'01', 'carol', 1, 2, 1, 0)".to_owned(),
            "INSERT INTO `chat_group_cursors` VALUES (x'01', zeroblob(31), 'node', 0, 0)".to_owned(),
            "INSERT INTO `chat_group_cursors` VALUES (x'01', zeroblob(32), 'other', 0, -1)".to_owned(),
            "INSERT INTO `chat_group_links` (`group_id`, `link_id`, `sealed_secret`, `created_by`, `created_epoch`) \
             VALUES (x'01', zeroblob(15), x'aa', 'alice', 0)".to_owned(),
            format!(
                "INSERT INTO `chat_group_members` (`group_id`, `peer_id`, `role`, `state`, `source`, `via_link`, `since_epoch`) \
                 VALUES (x'01', 'dan', 3, 2, 1, {LINK}, 1)"
            ),
            "INSERT INTO `chat_group_events` (`group_id`, `epoch`, `kind`, `actor`, `after_arrival`, `at_ms`) \
             VALUES (x'01', 1, 0, 'alice', 0, 1)".to_owned(),
            "INSERT INTO `chat_group_mentions` VALUES (x'01', 'bob', zeroblob(15), 1)".to_owned(),
        ] {
            assert!(run(&sql).is_err(), "{sql}");
        }
    }
}
```

Register the module in `rust/client/src/store.rs` with the others (alphabetical, after
`mod error;`):

```rust
mod groups;
```

and add to `create_schema` after `conversations::SCHEMA,`:

```rust
        groups::SCHEMA,
```

- [ ] **Step 2: Run the test and confirm it fails**

Run:
```bash
python3 tools/scripts/docker-local.py -f tools/docker/compose.dev.yml -f tests/docker/compose.dev.yml --profile test run --no-deps --pull never dev python3 -B tools/scripts/run-check.py --log-dir build/check-output rust-test -- cargo test --package dyapp-client --lib store::groups --locked --offline
```
Expected: FAIL on the first insert with `no such table: chat_groups`.

- [ ] **Step 3: Write the schema**

Replace `pub(super) const SCHEMA: &str = "";` in `groups.rs` with the following. It is the
spec's SQL with the comments translated to English:

```rust
pub(super) const SCHEMA: &str = r"
CREATE TABLE `chat_groups` (
    `group_id` BLOB PRIMARY KEY NOT NULL CHECK (length(`group_id`) > 0),
    -- own membership: 1 invited, 2 member, 3 left, 4 removed, 5 awaiting approval, 6 banned
    `state` INTEGER NOT NULL CHECK (`state` IN (1, 2, 3, 4, 5, 6)),
    `epoch` INTEGER NOT NULL CHECK (`epoch` >= 0),
    `name` TEXT NOT NULL DEFAULT '' CHECK (length(CAST(`name` AS BLOB)) <= 64),
    `description` TEXT NOT NULL DEFAULT '' CHECK (length(CAST(`description` AS BLOB)) <= 1024),
    -- attachment descriptor, as in chat_attachments
    `avatar` BLOB CHECK (`avatar` IS NULL OR length(`avatar`) BETWEEN 1 AND 2048),
    `pinned_author` TEXT,
    `pinned_message` BLOB CHECK (`pinned_message` IS NULL OR length(`pinned_message`) = 16),
    -- permission bit mask; code rejects a change with an unknown bit
    `permissions` INTEGER NOT NULL DEFAULT 0 CHECK (`permissions` BETWEEN 0 AND 4294967295),
    `muted` INTEGER NOT NULL DEFAULT 0 CHECK (`muted` IN (0, 1)),
    `muted_hlc` INTEGER NOT NULL DEFAULT 0,
    `muted_device` BLOB NOT NULL DEFAULT x'',
    `favorite` INTEGER NOT NULL DEFAULT 0 CHECK (`favorite` IN (0, 1)),
    `favorite_hlc` INTEGER NOT NULL DEFAULT 0,
    `favorite_device` BLOB NOT NULL DEFAULT x'',
    CHECK ((`pinned_author` IS NULL) = (`pinned_message` IS NULL))
) STRICT;

CREATE TABLE `chat_group_members` (
    `group_id` BLOB NOT NULL CHECK (length(`group_id`) > 0),
    `peer_id` TEXT NOT NULL CHECK (length(`peer_id`) > 0),
    -- 1 owner, 2 admin, 3 member
    `role` INTEGER NOT NULL CHECK (`role` IN (1, 2, 3)),
    -- same codes as chat_groups.state
    `state` INTEGER NOT NULL CHECK (`state` IN (1, 2, 3, 4, 5, 6)),
    -- 1 added by a member, 2 joined through a link
    `source` INTEGER NOT NULL CHECK (`source` IN (1, 2)),
    `added_by` TEXT,
    -- the link used to join; link uses are counted from this column
    `via_link` BLOB CHECK (`via_link` IS NULL OR (length(`via_link`) = 16 AND `source` = 2)),
    `since_epoch` INTEGER NOT NULL CHECK (`since_epoch` >= 0),
    -- member card; NULL until the member publishes one
    `display_name` TEXT CHECK (`display_name` IS NULL OR length(CAST(`display_name` AS BLOB)) <= 64),
    `photo` BLOB CHECK (`photo` IS NULL OR length(`photo`) BETWEEN 1 AND 2048),
    `card_hlc` INTEGER NOT NULL DEFAULT 0,
    `card_device` BLOB NOT NULL DEFAULT x'',
    `card_operation` BLOB CHECK (`card_operation` IS NULL OR length(`card_operation`) = 16),
    PRIMARY KEY (`group_id`, `peer_id`)
) STRICT;

-- at most one current owner
CREATE UNIQUE INDEX `chat_group_owner` ON `chat_group_members` (`group_id`)
    WHERE `role` = 1 AND `state` = 2;

-- blocking, shared groups and profile removal look members up by peer
CREATE INDEX `chat_group_members_peer` ON `chat_group_members` (`peer_id`);

-- invite links; the secret is sealed and there is no use counter (see via_link)
CREATE TABLE `chat_group_links` (
    `group_id` BLOB NOT NULL CHECK (length(`group_id`) > 0),
    `link_id` BLOB NOT NULL CHECK (length(`link_id`) = 16),
    `sealed_secret` BLOB NOT NULL CHECK (length(`sealed_secret`) BETWEEN 1 AND 256),
    `created_by` TEXT NOT NULL CHECK (length(`created_by`) > 0),
    `created_epoch` INTEGER NOT NULL CHECK (`created_epoch` >= 0),
    `revoked_epoch` INTEGER CHECK (`revoked_epoch` IS NULL OR `revoked_epoch` >= `created_epoch`),
    `expires_at_ms` INTEGER CHECK (`expires_at_ms` IS NULL OR `expires_at_ms` > 0),
    `max_uses` INTEGER CHECK (`max_uses` IS NULL OR `max_uses` >= 1),
    -- 1: a member joining through the link waits for admin approval (state 5)
    `approval` INTEGER NOT NULL DEFAULT 0 CHECK (`approval` IN (0, 1)),
    PRIMARY KEY (`group_id`, `link_id`)
) STRICT;

-- read cursor of the epoch topic on each node
CREATE TABLE `chat_group_cursors` (
    `group_id` BLOB NOT NULL CHECK (length(`group_id`) > 0),
    `topic` BLOB NOT NULL CHECK (length(`topic`) = 32),
    `node` TEXT NOT NULL CHECK (length(`node`) > 0),
    `epoch` INTEGER NOT NULL CHECK (`epoch` >= 0),
    `seq` INTEGER NOT NULL CHECK (`seq` >= 0),
    PRIMARY KEY (`group_id`, `topic`, `node`)
) STRICT;

-- messages that mention or answer the owner; keyed like chat_messages so the flag needs no
-- new column in the message table
CREATE TABLE `chat_group_mentions` (
    `group_id` BLOB NOT NULL CHECK (length(`group_id`) > 0),
    `author` TEXT NOT NULL CHECK (length(`author`) > 0),
    `message_id` BLOB NOT NULL CHECK (length(`message_id`) = 16),
    `unread` INTEGER NOT NULL DEFAULT 1 CHECK (`unread` IN (0, 1)),
    PRIMARY KEY (`group_id`, `author`, `message_id`)
) STRICT;

-- system feed lines such as a member added
CREATE TABLE `chat_group_events` (
    `event_order` INTEGER PRIMARY KEY AUTOINCREMENT,
    `group_id` BLOB NOT NULL CHECK (length(`group_id`) > 0),
    `epoch` INTEGER NOT NULL CHECK (`epoch` >= 0),
    -- event kind; code owns the list, the range leaves room for new kinds without a migration
    `kind` INTEGER NOT NULL CHECK (`kind` BETWEEN 1 AND 255),
    `actor` TEXT NOT NULL CHECK (length(`actor`) > 0),
    `target` TEXT CHECK (`target` IS NULL OR length(`target`) > 0),
    -- arrival_order of the group's last message when applied; 0 if there are no messages
    `after_arrival` INTEGER NOT NULL CHECK (`after_arrival` >= 0),
    `at_ms` INTEGER NOT NULL CHECK (`at_ms` > 0),
    -- new value (name, role and so on) when the event needs one
    `detail` BLOB CHECK (`detail` IS NULL OR length(`detail`) BETWEEN 1 AND 1024)
) STRICT;

CREATE INDEX `chat_group_events_feed` ON `chat_group_events` (`group_id`, `after_arrival`);

-- MLS state of each group kept apart: a 1000-member tree takes about 0.5 MB and must not be
-- rewritten with the shared mls_state on every message
CREATE TABLE `mls_group_state` (
    `group_id` BLOB PRIMARY KEY NOT NULL CHECK (length(`group_id`) > 0),
    `sealed` BLOB NOT NULL,
    `revision` INTEGER NOT NULL CHECK (`revision` >= 0)
) STRICT;
";
```

- [ ] **Step 4: Run the tests and confirm they pass**

Same command as Step 2. Expected: PASS, `1 passed`.
Then run the whole store suite to confirm `Store::open` and the version test still pass:
```bash
python3 tools/scripts/docker-local.py -f tools/docker/compose.dev.yml -f tests/docker/compose.dev.yml --profile test run --no-deps --pull never dev python3 -B tools/scripts/run-check.py --log-dir build/check-output rust-test -- cargo test --package dyapp-client --lib store:: --locked --offline
```
Expected: all PASS.

- [ ] **Step 5: Update the table catalogue**

In `docs/design/client-local-storage.md`, after the `read_marks` row, add:

```markdown
| `chat_groups` | Reserved, not used by code yet: own membership state, MLS epoch, name, description, avatar descriptor, pinned message, permission bits, mute and favorite flags with HLC and device | One row per group ID; a group ID is never also in `conversation_groups`. Group properties change only by a verified MLS commit. |
| `chat_group_members` | Reserved, not used by code yet: role, membership state (invited, member, left, removed, pending approval, banned), how the member joined and through which link, the member's own group card | `(group ID, Peer ID)` is unique; at most one current owner per group; indexed by Peer ID for blocking and profile removal. |
| `chat_group_links` | Reserved, not used by code yet: invite link ID, sealed secret, creator, creation and revocation epoch, expiry, use limit, approval flag | `(group ID, link ID)` is unique; uses are counted from members joined through the link. |
| `chat_group_cursors` | Reserved, not used by code yet: per node, the read position in the current epoch topic of a group | `(group ID, topic, node)` is unique; the topic address is stored nowhere else. |
| `chat_group_mentions` | Reserved, not used by code yet: messages that mention or answer the owner, with an unread flag | Keyed like `chat_messages`; removed with the message and by Clear chat. |
| `chat_group_events` | Reserved, not used by code yet: system feed lines such as a member added or a name changed | Placed in the feed after the message with `after_arrival`, then by local order; Clear chat removes them. |
| `mls_group_state` | Reserved, not used by code yet: sealed MLS state of one large group with a revision | Sealed with the `mls_state` key, bound to `(group ID, revision)`. |
```

- [ ] **Step 6: Check size and commit**

```bash
git diff --stat -- rust/client/src/store/groups.rs rust/client/src/store.rs docs/design/client-local-storage.md
git add -- rust/client/src/store/groups.rs rust/client/src/store.rs docs/design/client-local-storage.md
git commit -m "Reserve client schema for group chats

Create empty group, member, link, cursor, mention, event and MLS state
tables in the initial schema so group chats need no migration later.

ADR: docs/decisions/0025-client-local-sqlite-storage.md
Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```
Expected: `--stat` shows at most 400 lines and pre-commit is green. After the commit, run
`make quality` (it runs `check-adr` and rustfmt). Apply rustfmt fixes by hand from the log:
`make fmt` only checks.

---

## Beads

- **P0 v0.0.1 blocker** (task under the v0.0.1 release epic, labels as on the reactions
  schema reservation):
  "Reserve client schema for group chats before v0.0.1". Architect-level description in
  English; details in the `design` field (pointers to the spec and this plan); Acceptance
  Criteria from Task 1.
- **Post-release epic** (epic, P2): "Group chats in all clients". Success Criteria: a group
  of up to 1000 members works in every client on top of the reserved schema with no client
  database migration. Contents and open questions come from the spec's Beads section. Stage 0
  of the epic decomposes it into sub-beads, as in the reactions epic. The P0 reservation
  blocks the epic, and the epic links back to this brainstorming session in the tracker.
