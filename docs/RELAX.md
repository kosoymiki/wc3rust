# Relax — dev-loop JIT for Rust (H2 integration)

Upstream: `github.com/makepad/relax` @ `5dc818e`, MIT. Relax is an **experimental,
AI-generated Rust JIT**: `relax run <pkg>` compiles functions on first call, tiers
the hot ones, and patches edits into the running process. It exists to cut the
edit-run latency for agents and humans.

**Relax is not a release compiler and does not inherit rustc's safety guarantees.**
Release artifacts are built only with `cargo build --release`.

## Rules (enforced)

| Rule | Where | Gate |
|---|---|---|
| Dev-loop only; release = rustc | AGENTS.md, systemprompt.md, guardrails.md | `gate.relax.dev-loop-only` |
| Plugin fail-open, Termux-safe (no `/tmp`), guidance injected | plugin `~/.config/opencode/plugin/relax.js` | `gate.relax.termux-plugin` |
| Real game archive is the parity oracle (`matched==files`, `read_fail==0`) | AGENTS.md | `gate.h2.real-game-oracle` |
| Extract MDF via ISO9660 + stride solve; verify magic and `archive_size` | AGENTS.md | `gate.h2.mdf-magic-before-use` |

## Integration (this device)

- Source: `~/src/relax` (shallow clone @ `5dc818e`), built on Termux:
  `cargo build --release -p relax-runner` → `~/src/relax/target/release/relax`
  (69.9 MB, aarch64-linux-android, rc=0, 15m15s cold).
- Plugin: `~/.config/opencode/plugin/relax.js` — tools `relax_status`, `relax_run`,
  `relax_check`, `relax_test`, `relax_inspect`. Resolves `RELAX_BIN` →
  `~/src/relax/target/release/relax`; `TMPDIR` defaults outside `/tmp`;
  `RELAX_OFFLINE=1`; diagnostics in `~/.local/share/opencode/harness2/relax.log`;
  fail-open when the binary is missing.
- Verified: `relax --help` rc=0 (31 ms); `relax run hello` rc=0, correct output
  (`fib=[0..34] sum=88`), JIT cache 56 MB; warm run **149 ms** vs `cargo run`
  **738 ms** (same warm tree) on this device.

## Notes and limits

- Linux JIT path is `mmap` → `mprotect(RX)` (W^X), which is what lets it run
  under Termux/Android; the shipped docs only claim Linux x86-64 and macOS arm64,
  so this aarch64/Android run is outside upstream's stated support — treat it as
  experimental evidence, not a guarantee.
- Code Relax runs is slower than `--release` (upstream: 1.12x x64 / 1.24x arm64
  geomean). Do not benchmark release performance with it.
- `relax check` is not yet faster than `cargo check` on large packages; the value
  is `run`/`test`/`live` latency and live patching.
- Caches live in `<pkg>/target/relax` (append-only); `relax clean` removes them.
