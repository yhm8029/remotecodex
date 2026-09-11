# ADR-004: Terminal snapshots use the vendored terminal state

Status: accepted

The reconnect snapshot is serialized from Alacritty's active and inactive grids,
cursor state, tab stops, scroll region, modes, and palette. The adapter does not
parse input chunks to maintain duplicate primary, tab, or region state; this keeps
coalesced and split VT sequences equivalent.

The vendored engine stores the active charset selection on each grid cursor so
saved and current cursor character sets survive `ESC 7`/`ESC 8` and alternate
screen swaps. The supported selection profile is G0/G1 through SI/SO; G2/G3
designation is preserved while LS2/LS3 selection remains outside the profile.

Snapshot fixtures compare a live prefix plus continuation with the snapshot plus
the same continuation in `@xterm/headless`. Rust unit tests cover the engine
state transitions and Node fixtures cover rendered cells and cursor position.

## Shared Unicode scalar profile

Both terminal engines use `remotecodex-unicode17-two-cell-v1`. The complete width
table is generated from the pinned `unicode-width = 0.2.2` dependency (Unicode
17.0.0). It uses non-CJK ambiguous widths and caps nonzero scalar widths at two
terminal cells. Control characters remain the parser's responsibility.

The same cap is applied at the vendored Alacritty input handler before insert
mode shifts cells. This matters for U+17D8: the upstream width is three, while
both terminal cell models support a maximum of two. A per-character exception
is not used. The browser and headless tests register the same provider through
xterm 5.5.0's public, experimental Unicode API before any output is written.

`crates/rc-agent/examples/unicode-width-profile.rs` emits deterministic inclusive
zero/two-width ranges; omitted valid scalars have width one. Its `--raw` mode
independently emits one normalized width per codepoint for exhaustive comparison
with the generated JavaScript table. `tests/e2e/unicode-width.mjs` covers this
comparison and combining-property behavior. `tests/e2e/terminal-golden.mjs`
compares actual Rust snapshots and continuations, including normal/insert/wrap
cases at the two-cell boundary. Run these after `npm run test:client` has built
the shared provider.

The scalar combining-property logic is adapted from xterm.js 5.5.0's MIT-licensed
`src/common/input/UnicodeV6.ts` and `src/common/services/UnicodeService.ts`.
The generated table retains unicode-width's MIT/Apache-2.0 attribution. These
checks establish scalar-width agreement, not complete grapheme-cluster or
terminal-engine equivalence.

Regenerate the deterministic scalar table with `node scripts/generate-unicode-width.mjs`; CI/manual drift checks use the same command with `--check`.

## Readable current-screen projection

The authenticated projection endpoint reads the canonical server TerminalModel and sequence under one output lock. It exports visible physical rows only, keeping soft-wrap boundaries and replacing each fetched snapshot instead of appending a transcript. Hidden cells are masked, including combining marks; alternate screen or limits select terminal_raw. At most 150 rows, 400 columns, and 256 KiB of unescaped text are visited. Serialized JSON can be larger due to escaping. The UI escapes all text, discards obsolete identities, keeps one request pending, and falls back after 2.5 seconds. A timed-out request remains pending until the API timeout; raw view stays available. This is terminal_projection, not adapter_verified semantic events.
