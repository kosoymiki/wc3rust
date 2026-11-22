# FULL GOAL — literal 1:1 Warcraft III port (2026-11-22)

**Not a technical demo.** The deliverable is the operator's own Warcraft III install
played through our Rust port, behaving like the retail game from first click to score
screen, verified against the real game and on the device.

## Definition of done (all must hold)

1. **Mechanics**: every row of `MECHANICS_GAP_MAP.md` is `ported` with a named mechanic
   test green and device evidence for the same behaviour. No `partial`, no `missing`.
2. **Acceptance**: `FULL_PORT_CRITERIA.md` P0-1…P0-14 all PASS in
   `accept_full_port.sh` (`FULL_PORT_ACCEPTED`, 14/14, rc=0).
3. **Reference parity**: the operator's WC3 runs under a reference runtime
   (FEX-Emu / Wine / Proton) on this device or a reachable host; recorded traces
   (menus, economy, combat, campaign) are diffed against our build; every diff is a
   board item.
4. **HUD/UI**: every screen (menu, campaign, HUD, dialogs, score) is placed correctly
   on 2414x1080 and 1080x2414, with device screenshots; no overflow, no fallback art,
   no baked numerals, no cursor art leaking into the console.
5. **Input**: one physical touch = one gesture; minimap, selection, orders, box select,
   buildings, hotkeys evidenced on device.
6. **Rendering**: terrain, water, cliffs, doodads, units and shadows match the reference
   frames; no black patches, no untextured polygons.
7. **Audio**: per-def voices and music resolve from the shipped tables; underruns=0.
8. **Campaign**: 44 shipped chapters boot and are completable; save/load works.
9. **Living board**: `analysis/tech-lead/PORT_BOARD.{md,json}` is synced every
   generation (`scripts/port_board.py sync`), append-only, and every `done` carries
   evidence (`--evidence` is mandatory).
10. **H2 binding**: each lane has a ledger row, THICK packet, exclusive OWNER_PATHS,
    checkpoint ≤90 s, capture + experience, and closes with numeric ids.

## Non-negotiables

- No stubs, no placeholders, no "later" without owner + ledger entry.
- Evidence only: device screenshot, rc, trace, or byte-level artifact. "Looks fine" is not done.
- The board is the single TODO surface; new defects are added the moment they are met.
- Release artifacts are built by rustc (`cargo build --release`); Relax is dev-loop only.
- The shipped game is the oracle: real War3.mpq byte-exact (`10523/10523`), real
  behaviour traces for semantics.

## Lanes (exclusive OWNER_PATHS)

| Lane | Scope | OWNER_PATHS | Board sections |
|---|---|---|---|
| sim-mechanics | mechanics partials + economy.repair, JASS semantics | `crates/wc3-sim/**`, `crates/wc3-jass/**`, `crates/wc3-game/src/campaign.rs` | mechanics, acceptance |
| render | black terrain, shadows, water/cliffs, reference frames | `crates/wc3-render/**`, `crates/wc3-game/src/terrain_render.rs`, `crates/wc3-render/tests/**` | rendering |
| hud-input | HUD pack art/mapping, minimap hit, touch dedup, selection/orders | `crates/wc3-game/src/{game,hud,mobile_hud,assets}.rs`, `crates/wc3-input/**`, `apps/android-native/src/lib.rs`, `assets/hud/**`, `crates/wc3-game/src/ui/**`, `crates/wc3-game/src/hudskin/**` | hud, input, operator-bugs |
| reference-runtime | FEX/Proton/Wine, run the operator's WC3, capture traces | `analysis/agent-ref/**`, `~/src/ref-runtime/**` | reference-runtime |
| parity-data | shipped tables, original_parity checks, data coverage | `crates/wc3-data/**`, `crates/wc3-formats/**`, `crates/wc3-assetdb/**`, `crates/wc3-mpq/**`, `scripts/original_parity.py`, `scripts/*_compare.py`, `analysis/port-parity/**` | mechanics(data), parity |
| infra-board | board sync automation, gates, test-all, release/APK | `scripts/port_board.py`, `scripts/test-all.sh`, `scripts/build-apk.sh`, `scripts/build-native.sh`, `docs/**`, `analysis/tech-lead/PORT_BOARD.*` | h2-binding, infra, tooling |

## Execution order

1. infra-board: board sync + gate parity (so every lane updates the same surface).
2. sim-mechanics: close the 11 partials + economy.repair one by one, each with a test and device evidence.
3. hud-input: fix the console/minimap/touch defects from the 2026-11-22 screenshots.
4. render: kill the black terrain and untextured polygons.
5. reference-runtime: bring up FEX or Proton, record traces, feed diffs to the board.
6. parity-data: keep the byte-level oracle green and grow coverage.

## Reporting

Every lane replies `Goal | Actions | Evidence (rc/screenshot/trace) | Result | Risks |
H2_PERSISTED | SESSION_CLOSED` with numeric ids, and runs `python3 scripts/port_board.py sync`
before it closes. The tech lead closes `analysis/tech-lead/ALL_LAYERS_FINAL.md` only when
the ten DoD items hold.
