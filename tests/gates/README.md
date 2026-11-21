# Gate scripts (lane 510)

Two executable gates back the workspace quality job. Both are runnable locally,
print a single machine-readable verdict line, and exit non-zero on any violated
assertion.

| Script | What it proves | Run |
| --- | --- | --- |
| `orc01-smoke.sh` | `Maps\Campaign\Orc01.w3m` boots headless through the real runtime (JASS, roster, quests, cutscene director) and survives 3000 ticks. Prints `ORC01 units=N quests=N ticks=3000 cam=K fades=M subs=S`. | `tests/gates/orc01-smoke.sh` |
| `apk-release-gate.sh` | The signed release APK has `debuggable=false` in the compiled manifest and in badging, a verified v2/v3 signature, the `arm64-v8a` ABI, a deterministic security summary, and a report sha256 equal to the file's. | `tests/gates/apk-release-gate.sh` |
| `menu-layout-gate.sh` | A headless `Game` publishes `GAME_MENU_LAYOUT screen=Main vp=2414x1080 entries=N` with `N >= 5` (the real main menu), one `GAME_MENU_ENTRY` per declared entry, non-degenerate rects and a tap point inside each rect. | `tests/gates/menu-layout-gate.sh` |
| `hud_pack_check.py` (re-pointed) | Re-derives the pack, touch-floor, 4x3 card-grid, safe-area and magenta/placeholder rules from `hud-layout-matrix.json` + the rendered screenshot; the retired draw-quad/section rules are gone. | `python3 tools/hud_pack_check.py` |

`scripts/test-all.sh` runs the menu gate as `gate:menu` (`--no-menu` disables
it) and the Orc01 smoke as `gate:orc01` (`--no-orc01`); both publish their
numbers into `analysis/test-all.jsonl`. The APK gate is run by the packager and
by hand against `build/android-p0/release/`.

## Failure self-test

`scripts/test-all.sh --self-test` proves the gate goes red for all three:

1. a failing test injected into `wc3-bink` — a member the old hand-written
   subset never ran;
2. a zero-byte `Orc01.w3m` in a throwaway install root — the gate must exit
   non-zero and the log must carry `ORC01_SMOKE_FAIL`;
3. the menu gate on a valid and on a 2-entry fixture — the valid block must be
   accepted and the short menu rejected with `MENU_GATE_FAIL`.

## Why there is no GitHub Actions workflow here

The gate is device-bound by construction: the host target is
`aarch64-linux-android`, the native build needs the Termux `$PREFIX` libraries
and `third_party/sdl3-android-build`, and the Orc01 chapter is read from the
operator's install (`analysis/war3_full3`, intentionally untracked). A hosted
runner cannot satisfy any of that, and a workflow that runs a reduced subset
would be a decorative gate. The gate script is the CI entry point; it is invoked
on the machine that holds the install, with `--self-test` as the proof that it
can fail.


## Exit codes shared by the gates

- `0` every rule holds.
- `1` a rule is violated (the exact reason is printed as `*_FAIL reason=...` and
  the tail of the job log is pasted).
- `2` bad usage / missing input (no manifest, no matrix, no replay log).
- `3` the gate could not run: `h2-cargo` refused to bypass the FIFO slot
  (`rc 75`) or the build was SIGKILLed (`rc 137`, e.g. OOM). `scripts/test-all.sh`
  turns a job `rc 3` into `TEST_ALL_ABORT` and exits 3, so congestion is never
  reported as a member failure.
