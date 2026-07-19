# Agent-history search

`agent-history search` is the user-facing ranked-history query CLI. It accepts a plain-text query, delegates retrieval to `claude-memory`, and emits either human-readable results or backend NDJSON. This document defines the contract; implementation details belong in [`docs/wiki/systems/agent-history-search.md`](../wiki/systems/agent-history-search.md).

## What it must do

### Query and retrieval

- [x] Accept one plain-text positional query and forward its text unchanged to `claude-memory search`.
- [x] Invoke `claude-memory` once per search with JSON mode and the selected backend limit.
- [x] Use `5` as the default maximum result count.
- [x] Treat `-m N` and `--max-count N` as aliases that forward `N` as the backend limit.

### Output and color

- [x] With `--json`, write backend NDJSON records unchanged and preserve their order.
- [x] Without `--json`, render each result as score, record type, session ID, path, then text.
- [x] Disable ANSI color when `--no-color` is supplied.
- [x] Do not emit ANSI color when human output is captured on a non-terminal stdout.

### Removed surface and data scope

- [x] Reject removed options: `--source`, `--role`, `--project`, `--session`, `--since`, `--until`, `-i`/`--ignore-case`, `-l`/`--files-with-matches`, `--live-only`, `--archive-only`, and `--all`.
- [x] Restrict searchable data to `claude-memory` results; do not scan local Claude, Codex, or Pi session files.
- [x] Provide no fallback retrieval path when `claude-memory` is unavailable, fails, or returns invalid data.

## How it works

- [`docs/wiki/systems/agent-history-search.md`](../wiki/systems/agent-history-search.md)

## Implementation inventory

- `src/main.rs` — parses the CLI and runs the search command.
- `src/cli.rs` — defines the retained command and options.
- `src/backend.rs` — invokes `claude-memory` and parses search records.
- `src/output.rs` — renders human output or passes through JSON output.

## Tests asserting this spec

- `tests/search.rs` — asserts query forwarding, default and explicit limits, JSON passthrough, human field order, color behavior, rejection of removed options, ignored local transcripts, and explicit backend failure handling without fallback.

## Known gaps (current cycle)

- None.

## Out of scope

- Local session-file scanning and its source, role, project, session, date, regex, file-listing, live/archive, and hidden-payload filters.
- Changes to the `claude-memory` backend, indexing lifecycle, or stored transcript coverage.
