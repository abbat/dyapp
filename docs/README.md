# dyapp Documentation

All project documentation lives in `docs/`. The only Markdown files outside
it are entry points that tools discover by path (see
[Required entry points](#required-entry-points)).

## Index

### Security
- [Encryption & Security Status](security/encryption.md): what is and is not protected today, by layer, target design
- [Privacy & Metadata Visibility](security/privacy.md): which observer sees which field, retention, residual risk
- [Threat Model](security/threat-model.md): attackers, assets, and which defence covers each threat today

### Architecture
- [Overview](architecture/overview.md)
- [P2P networking](architecture/p2p-networking.md)
- [Messaging](architecture/messaging.md)
- [Video](architecture/video.md)
- [Bootstrap server](architecture/bootstrap.md)
- [Replication design](architecture/replication.md): planned acceptor fan-out, media sharding, repair
- [FFI design](architecture/ffi-design.md)
- [Protobuf schema](architecture/protobuf-schema.md)
- [Platform integration](architecture/platform-integration.md)

### Development
- [Git conventions](development/git-conventions.md): branches, commits, Cargo.lock, MSRV
- [Build automation](development/build.md)
- [FFI bindings (UniFFI)](development/ffi-bindings.md)
- [Protobuf code generation](development/protobuf-codegen.md)
- [Linting](development/linting.md)
- [Architecture lint](development/arch-lint.md)
- [Code quality](development/code-quality.md)
- [CI base workflow](development/ci-base.md)
- [CI matrix](development/ci-matrix.md)
- [Documentation tooling](development/documentation.md)
- [Architecture decisions (ADR)](decisions/README.md)

### Testing
- [Testing overview](testing/README.md)
- [UI testing](testing/ui-testing.md)
- [Docker testing](testing/docker.md)

### Platforms
- [Empty app contract](platforms/empty-app-contract.md)

### Operations
- [Deployment](operations/deployment.md)

### AI agents
- [Project guide for agents](agents/project.md): layout, commands, limits, rules
- [lean-ctx](agents/lean-ctx.md): optional MCP context tools
- [Beads (`bd`)](agents/beads.md): optional local issue tracker

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
