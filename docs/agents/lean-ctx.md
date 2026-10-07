# lean-ctx for agents

[lean-ctx](https://leanctx.com/docs) is an MCP server with code navigation, reading, search and
shell tools whose output is compressed to save tokens. In this project it is **mandatory when
installed**, for any agent (Claude Code, Codex, others): use its tools instead of plain
`cat`/`grep`/`find`/shell. The `lean-ctx` skill (if your harness has it) is the short routing
reference; this page must not contradict it.

## Check what is available

Tool sets differ between installs and profiles, so do not assume a tool exists because it is named
here. Some tools are registered directly (they appear in your tool list as `ctx_read`, or
`mcp__lean-ctx__ctx_read` in Claude Code); the rest are reachable only through `ctx_call`.

```json
ctx_call {"name": "ctx_discover_tools", "arguments": {"query": "callers"}}
ctx_call {"name": "ctx_graph", "arguments": {"action": "status"}}
```

An empty `query` lists every tool. Follow the schema your client shows; arguments are JSON, not
CLI flags.

## Routing

| Need | Tool | Example arguments |
|------|------|-------------------|
| Unknown code, "where/how is X done" | `ctx_compose` | `{"task": "how does bootstrap discover peers"}` |
| Known file | `ctx_read` | `{"path": "/abs/rust/ffi/src/lib.rs", "mode": "signatures"}` |
| Text / regex search | `ctx_search` | `{"pattern": "pub fn ", "path": "/abs/rust/ffi/src", "max_results": 20}` |
| Files by name | `ctx_glob` | `{"pattern": "**/Cargo.toml"}` |
| Callers / callees | `ctx_callgraph` | `{"action": "callers", "symbol": "create_offer"}` |
| Builds, tests, git, `make` | `ctx_shell` | `{"command": "make test", "cwd": "/abs/repo"}` |
| Edit a file | `ctx_read` `mode="anchored"` → `ctx_patch` | |
| Output shown as `[Archived:ID]` | `ctx_expand` | `{"id": "<ID>", "search": "error"}` |

`/abs` stands for the absolute path of your checkout; paths must be absolute.

## Read modes and truncated output

- `ctx_read` `mode`: `auto` (default, may compress), `signatures` (API only), `lines:N-M`
  (window), `anchored` (for `ctx_patch`), `full` (verbatim), `raw` (exact bytes, bypasses cache).
  `fresh: true` re-reads a file changed outside lean-ctx.
- Instruction files (AGENTS.md, SKILL.md) are always returned in full.
- `ctx_shell` compresses command output; pass `raw: true` when you need it verbatim.
- If output was cut or archived, recover it with `ctx_expand` (shell/tool output) or
  `ctx_retrieve` (a file read earlier). Do not rerun the command just to see more.

## Limits

- lean-ctx runs with your permissions: a path or command denied to you stays denied. Do not retry
  denied calls or change lean-ctx configuration (`lean-ctx allow…` and similar) without explicit
  user approval.
- `ctx_shell` is not a sandbox: the project rules on installs and deletion in
  [project.md](project.md#rules-for-agents) apply to commands run through it.
- If lean-ctx is not installed or cannot reach a resource, fall back to the narrowest native tool.
