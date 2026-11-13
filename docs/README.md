# dyapp Documentation

All project documentation lives in `docs/`. The only Markdown files outside
it are entry points that tools discover by path (see
[Required entry points](#required-entry-points)).

## Index

### Security

- [Security policy](../.github/SECURITY.md): supported versions, scope and private vulnerability reporting
- [Encryption & Security Status](security/encryption.md): what is and is not protected today, by layer, target design
- [Privacy & Metadata Visibility](security/privacy.md): which observer sees which field, retention, residual risk
- [Threat Model](security/threat-model.md): attackers, assets, and which defence covers each threat today
- [Security Assurance](security/assurance.md): OpenSSF Scorecard ratchet, Best Practices badge, verification checklist catalogue, vulnerability disclosure
- [OpenMLS curve dependency migration](security/openmls-curve-migration.md): audit failure, candidate migration and compatibility gates

### Architecture

- [Overview](architecture/overview.md)
- [P2P networking](architecture/p2p-networking.md)
- [Messaging](architecture/messaging.md)
- [Video](architecture/video.md)
- [Bootstrap server](architecture/bootstrap.md)
- [Search capacity](architecture/search-capacity.md)
- [FFI design](architecture/ffi-design.md)
- [Protobuf schema](architecture/protobuf-schema.md)
- [UI state retention](architecture/ui-state-contract.md): current Linux profile draft survival and planned chat draft contract

### Product and operator design

- [Client local storage](design/client-local-storage.md): per-profile SQLite ownership, pre-release schema, durable mailbox work and local media; [storage ADR](decisions/0025-client-local-sqlite-storage.md), [visibility ADR](decisions/0036-search-visibility.md)
- [Desktop runtime lifecycle](design/desktop-runtime-lifecycle.md): identity switch/removal gate, cancellation and cache isolation
- [Desktop video rendering](design/desktop-video-rendering.md): synthetic-frame benchmark plan for Linux WebView and native fallback
- [Desktop camera capture](design/desktop-camera-capture.md): evidence and validation plan for Linux contact QR scanning
- [Private interaction signals](design/private-interactions.md): planned wire format, ordering, undo and match state
- [Likes, matches and chats](design/chats-and-matches.md): planned per-pair state, message requests, one group per pair and own-device cases
- [Online presence](design/presence.md): planned coarse "active recently" signal for matches, audience, throttle and leaks
- [Read receipts and typing indicators](design/read-receipts.md): planned opt-in "read up to" signal; typing waits for a direct path
- [Message edits and deletion](design/message-edits-and-deletion.md): private operations, ordering, placeholders and best-effort node removal
- [Sticker packs](design/stickers.md): validated TAR/WebP, chat tags 14 to 16, durable pack exchange, shared media storage and desktop picker
- [v0.0.1 client screen draft](design/v0.0.1-client-screens.md): entry, settings, profile and search interaction cards
- [Profile photos](design/profile-photos.md): planned publication, limits, cache and failure states
- [Desktop screens](design/desktop-screens.md): control IDs and states for the desktop shell (part shipped, the rest planned), checked against the static prototype
- [Mobile screens](design/mobile-screens.md): control IDs and states for the Android client, checked against the static Android prototype
- [Desktop heuristic review](design/desktop-heuristics.md): Nielsen heuristics review of the prototype and the shipped desktop UI, its fixes and the rule to re-check screen changes
- [UI review criteria](design/review-criteria.md): sourced settings, layout, identifier and dialog criteria, and the checks that enforce them; [decision](decisions/0046-ui-review-criteria.md)
- [Localized configuration diagnostics](design/localized-config-diagnostics.md): Russian guidance, technical details and secret-safe errors
- [Client update discovery](design/client-updates.md): release source, version, schedule, platform delivery and EN/RU notice UX

### Development

- [Git conventions](development/git-conventions.md): branches, commits, Cargo.lock, MSRV
- [Pull request review](development/pr-review.md): checks and human review of untrusted PRs
- [Desktop identity storage](development/desktop-identity-storage.md): user choice, runtime capability checks and persistence
- [Android identity storage](development/android-identity-storage.md): per-identity Keystore aliases, slots and recovery
- [iOS identity storage](development/apple-identity-storage.md): per-profile Keychain slots, legacy migration and recovery
- [Build automation](development/build.md)
- [FFI bindings (UniFFI)](development/ffi-bindings.md)
- [Protobuf code generation](development/protobuf-codegen.md)
- [Linting](development/linting.md)
- [Code quality](development/code-quality.md)
- [Check output inventory](development/check-output.md): quiet checks and named data consumers
- [Weekly flow metrics](development/flow-metrics.md): local A7 report over beads, Git and GitHub
- [CI base workflow](development/ci-base.md)
- [CI matrix](development/ci-matrix.md)
- [Documentation tooling](development/documentation.md)
- [Running a quorum](development/quorum.md): multi-model cross-review, the human's steps
- [Engineering practices](development/practices.md): planned catalogue of measurable practices for agents
- [Architecture decisions (ADR)](decisions/README.md)

### Testing

- [Testing overview](testing/README.md)
- [UI testing](testing/ui-testing.md)
- [Docker testing](testing/docker.md)
- [Bootstrap acceptance tests](testing/bootstrap-acceptance.md)

### Platforms

- [Empty app contract](platforms/empty-app-contract.md)

### Operations

- [Deployment](operations/deployment.md)
- [Abuse reports and legal requests](operations/abuse-and-legal-requests.md)
- [Bootstrap node operator manual](manuals/bootstrap.md) ([Russian](manuals/bootstrap.ru.md))
- [Linux desktop client](manuals/linux.md) ([Russian](manuals/linux.ru.md))
- [Windows desktop client](manuals/windows.md) ([Russian](manuals/windows.ru.md))
- [macOS client](manuals/macos.md) ([Russian](manuals/macos.ru.md))
- [iOS client](manuals/ios.md) ([Russian](manuals/ios.ru.md))
- [Android client](manuals/android.md) ([Russian](manuals/android.ru.md))

### AI agents

- [Project guide for agents](agents/project.md): layout, commands, limits, rules
- [lean-ctx](agents/lean-ctx.md): optional MCP context tools
- [Beads (`bd`)](agents/beads.md): optional local issue tracker
- [Quorum](agents/quorum.md): multi-model cross-review of design, decisions and code
- [Answering issues](agents/issues.md): trust, reply language and tone in issues

Not written yet: API reference, performance.

## Required entry points

These stay outside `docs/` because tools load them by fixed path. Keep them
short and point here.

| File | Loaded by |
|------|-----------|
| `README.md` | Git hosting, humans |
| `AGENTS.md` | AI agents |
| `LICENSE` | Git hosting, package metadata |
| `.github/SECURITY.md` | GitHub security policy and private report guidance |
| `CONTRIBUTING.md`, `.github/PULL_REQUEST_TEMPLATE.md`, `.github/ISSUE_TEMPLATE/` | Git hosting: contributor guide, pull request and issue forms |
| `.agents/skills/*/SKILL.md` | Agent skill discovery (Codex; Claude Code via the `.claude/skills/*` symlinks) |
