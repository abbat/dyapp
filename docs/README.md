# dyapp Documentation

All project documentation lives in `docs/`. The only Markdown files outside
it are entry points that tools discover by path (see
[Required entry points](#required-entry-points)).

## Index

### Security
- [Encryption & Security Status](security/encryption.md): what is and is not protected today, by layer, target design
- [Privacy & Metadata Visibility](security/privacy.md): which observer sees which field, retention, residual risk
- [Threat Model](security/threat-model.md): attackers, assets, and which defence covers each threat today
- [Security Assurance](security/assurance.md): OpenSSF Scorecard ratchet, Best Practices badge, verification checklist catalogue, vulnerability disclosure
- [Repository exception audit](security/exception-audit-2026-10-24.md): first inventory review, decisions and next review date
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
- [Platform integration](architecture/platform-integration.md)

### Product and operator design
- [Client local storage](design/client-local-storage.md): per-profile SQLite ownership, migrations and crash-safe mailbox work; [accepted ADR](decisions/0025-client-local-sqlite-storage.md)
- [Private interaction signals](design/private-interactions.md): planned wire format, ordering, undo and match state
- [Message edits and deletion](design/message-edits-and-deletion.md): private operations, ordering, placeholders and best-effort node removal
- [v0.0.1 client screen draft](design/v0.0.1-client-screens.md): entry, settings, profile and search interaction cards
- [Localized configuration diagnostics](design/localized-config-diagnostics.md): Russian guidance, technical details and secret-safe errors
- [Client update discovery](design/client-updates.md): release source, version, schedule, platform delivery and EN/RU notice UX
- [Bootstrap update policy analysis](design/bootstrap-update-policy.md): threat model, alternatives and safeguards; [accepted policy](decisions/0023-bootstrap-node-update-policy.md)
- [Bootstrap update channel analysis](design/bootstrap-update-channel.md): signing boundary, key custody and anti-rollback options; [accepted channel](decisions/0024-bootstrap-update-channel.md)
- [Linux Secret Service migration](design/linux-secret-service-migration.md): options and proposed ADR amendment for removing the zbus 5 advisory chain

### Development
- [Git conventions](development/git-conventions.md): branches, commits, Cargo.lock, MSRV
- [Pull request review](development/pr-review.md): checks and human review of untrusted PRs
- [Desktop identity storage](development/desktop-identity-storage.md): user choice, runtime capability checks and persistence
- [Android identity storage](development/android-identity-storage.md): per-identity Keystore aliases, slots and recovery
- [Build automation](development/build.md)
- [FFI bindings (UniFFI)](development/ffi-bindings.md)
- [Protobuf code generation](development/protobuf-codegen.md)
- [Linting](development/linting.md)
- [Architecture lint](development/arch-lint.md)
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
- [RSS plateau investigation](testing/memory-plateau-investigation.md): measurements and unresolved growth attribution

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

Not written yet: API reference, contributing guide, performance.

## Required entry points

These stay outside `docs/` because tools load them by fixed path. Keep them
short and point here.

| File | Loaded by |
|------|-----------|
| `README.md` | Git hosting, humans |
| `AGENTS.md` | AI agents |
| `LICENSE` | Git hosting, package metadata |
| `.agents/skills/beads/SKILL.md` | Agent skill discovery (Codex; Claude Code via the `.claude/skills/beads` symlink) |
