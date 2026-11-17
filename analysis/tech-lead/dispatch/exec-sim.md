# EXECUTION STEER — SIMULATION (merge now, no waiting)

BASE.md is published (v0/v1 + §12 execution ownership) and
`analysis/tech-lead/WC3RE_MERGE_MATRIX.md` exists. This message supersedes
manifest-only mode: MERGE YOUR SCOPE NOW.

## Your exclusive execution paths (write)
crates/wc3-sim/**, new crates/wc3-game/src/sim_bridge.rs

## Do not touch
game.rs (HUD applies shim), hud.rs, other game files

## Rules
- One writer per path. Do not edit another layer's files; send the dispatcher a
  patch + exact file:line instead.
- Merge against BASE.md §2/§12 and the matrix. Do not re-implement a reader,
  parser, VM, renderer or sim.
- Compile your scope as soon as it is self-contained: `cargo check -p wc3-sim`.
- Run per-layer tests only after your scope compiles. Full suite waits for the
  dispatcher. NO device runs, screenshots or APK installs.
- Report back: rc, files changed, remaining compile conflicts (exact errors),
  and the manifest path. Keep `analysis/tech-lead/merge-sim.md` updated.

Start merging immediately; do not wait for BASE v2 or another agent's manifest.
