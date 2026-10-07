# Architecture lint (prototype)

`tools/arch-lint` is a standalone crate (not a workspace member, no dependencies) with three
substring-matching rules. Nothing runs it: no Makefile target, CI step or hook.

```bash
cd tools/arch-lint
cargo run -- --root ../../rust/messaging/src   # text report, exit 1 on any violation
cargo run -- --root <dir> --json               # JSON report
```

It scans only the `*.rs` files directly in `--root` (default `.`), **not subdirectories**.
`--strict` is the default and cannot be turned off, so any violation exits 1.

| Rule | Files checked | Flags |
|------|---------------|-------|
| `check_no_centralized_api` | path contains `p2p_net` or `bootstrap` | `std::http`, `reqwest`, `hyper::`, `axum::`, `tokio::net::TcpListener` outside `//` comments |
| `check_encrypted_data` | all | lines containing `password`, `private_key`, `secret` or `token` with no `encrypt`/`crypto::`/`[encrypted]` nearby |

Limits:

- Matching is by substring, so false positives and negatives are common. The crate directory is
  `rust/p2p-net`, so the first rule never matches it; `rust/bootstrap` uses `axum` by design and
  would fail.
- Not wired into `make quality` or CI; treat the output as hints, not a gate.
