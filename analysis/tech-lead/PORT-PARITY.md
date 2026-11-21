# PORT PARITY — the rust-anything loop applied to wc3rust

Source of the method: `github.com/erkamyaman/rust-anything` @ `98adbe2` (MIT), read file to file
on 2026-11-19: `README.md`, `docs/how-it-works.md`, `skills/rust-anything/SKILL.md`,
`skills/rust-anything/whole-app.md`, `DECISIONS.md`. Skill ported to
`~/.config/opencode/skills/rust-anything/` (upstream files verbatim + `ADDENDUM-H2.md`).
H2 gates added: `gate.port.parity-loop`, `gate.port.no-fixture-edit` (principle `port.parity-loop`).

## The philosophy in one paragraph

The original stays the **source of truth**. You do not argue that a port is correct — you
*record* what the original does on generated edge cases, semantic traps and real inputs, then
replay every one of those cases against the Rust and refuse to ship until all of them match and the
gate passes. Port one unit at a time, keep the original's API in front of callers, and only
`lift` to pure Rust when replay is 100 % and the fallback rate is zero. A mismatch is a port bug,
never a reason to edit the recording or widen a tolerance.

## Why it fits wc3rust

wc3rust ports Warcraft III behaviour to Rust. We cannot run the retail client as an oracle on the
device, so the "original" is decomposed:

| rust-anything | wc3rust |
|---|---|
| original app | the retail data (`War3.mpq`, SLKs, `war3map.*`) + the documented behaviour + the donor C++ where it builds |
| `record` | headless capture: `GAME_*` lines, state hashes, quest/trigger counts, `campaign_report` output — saved as cases |
| `cases.jsonl` | `analysis/port-parity/<unit>/cases.jsonl` (`id, args_sha, command, exit, stdout_sha, stdout`) |
| `diff` | `scripts/port-parity.sh diff <unit> -- <command>` — replays and compares |
| `bench` | `GAME_TIMING`, `GAME_STATE fps=…`, PSS on device; a port that slows the frame is not accepted |
| `shadow` | both paths in one binary behind a selector, old result returned, mismatches logged |
| `gate` | `scripts/port-parity.sh gate <unit>` + `scripts/test-all.sh` + the port gate (fmt, clippy, no `unimplemented!`, every recorded error path exercised) |

## The harness in this repo

```bash
# capture the reference behaviour of a unit (one case per distinct command line)
bash scripts/port-parity.sh record campaign-load --cmd \
  "scripts/h2-cargo run -q -p wc3-game --example campaign_report -- --mission Prologue01 --frames 20"

# replay every recorded command of the unit (or one with --cmd / -- <argv...>)
bash scripts/port-parity.sh diff campaign-load
# -> replay 1/1 match, known-diffs 0, mismatches 0

# the gate: parity + recording presence + no unimplemented!/todo! + cargo check
bash scripts/port-parity.sh gate campaign-load

# units, case counts, last diff/gate, stale recordings (JSON for the plugin/tools)
bash scripts/port-parity.sh status --json
```

The engine is `scripts/port_parity.py`; `scripts/port-parity.sh` is a thin wrapper, so every
older command keeps working. Demonstrated 2026-11-19 on `campaign-load`: the original case was
captured with stderr merged (compiler warnings included) and is now `"retired"`; the current case
`campaign-load-b26f7242` records stdout only (stdout sha `c0517596…`, stderr sha kept for
diagnostics), `diff` replays `1/1` and the full `gate` passes (`cargo check` rc=0).

Cases are append-only; re-recording the same command is refused (the recording is the truth).
An intended difference goes to `known-diffs.jsonl` with `{case, actual_sha, reason}` and the gate
fails again once the difference disappears — that is how a deliberate deviation stays visible.

## Rules adopted

1. A unit is ported only when replay is **N/N** and the gate passes; a green `cargo check` is not
   a port.
2. Never edit a recorded case to make a mismatch vanish; never widen a tolerance silently.
3. Every original error path must be reached by a recorded failing case (that is what forces
   `Err` instead of `unwrap`/`panic!` in the Rust).
4. `unsafe` only with a `// SAFETY:` note; nothing `unimplemented!`/`todo!` in ported code.
5. Report per unit: files, replay counts, gate output, bench numbers, known diffs, what is left.

## Plugin and tools (2026-11-19)

`~/.config/opencode/plugin/rust-anything.js` makes the loop part of every session:

| tool | what it does |
|---|---|
| `rust_anything_status` | units, case counts, retired cases, last diff/gate, stale recordings (read-only, `status --json`) |
| `rust_anything_record` | record the reference behaviour of a unit (`record <unit> --cmd "<command>"`) |
| `rust_anything_diff` | replay every recorded command (or one) → N/N, known-diffs, mismatches, cases sha256 |
| `rust_anything_gate` | full gate: replay + recording + no `unimplemented!`/`todo!` + `cargo check -p wc3-game --lib` |

The plugin injects the rule into the model context (same mechanism as harness2): *a unit is done
only when diff is N/N and the gate passes; the recording is never edited*. The repo is resolved
from `RUST_ANYTHING_REPO`, then `~/src/wc3rust`, then the session cwd; a missing repo disables the
tools and writes a line to `~/.local/share/opencode/harness2/rust-anything.log` (fail-open).

Machine output: `--json` prints exactly one JSON object and writes `last-diff.json` /
`last-gate.json` next to the cases; `status` reads them. New recordings use
`"capture": "stdout"` (`stdout`/`stdout_sha` are the contract, `stderr`/`stderr_sha` are
diagnostics); pre-migration cases have no `capture` and replay with the merged stream; a case can
be `"retired"` (history only) when its capture format is replaced by a fresh recording of the same
command — that never hides a mismatch.

The plugin loads at the next serve start (`opencode2 service restart` or a new session); the H2
gates it enforces are `gate.port.parity-loop` and `gate.port.no-fixture-edit`.

## What this cannot catch (from the upstream doc, applied to us)

Behaviour the recording never exercised: timing/GC effects, floating-point order (our sim
determinism tests cover some of it), regex/Unicode differences, and anything the retail client
does that we never captured. For wc3rust the answer is to record **mission-level** behaviour
(Orc01/Prologue01 runs, quests, triggers, camera beats), not only unit-level calls.

## Limits of this device

The upstream CLI (npm `rust-anything@1.5.1`, prebuilt binaries; `cargo install` unproven on
android-arm64) is treated as `unavailable` here; the loop is executed by the harness above. On a
Linux host the same cases can be fed to `rust-anything record/diff/gate` for the extra checks
(bridge generation, clippy gate, shadow mode).
