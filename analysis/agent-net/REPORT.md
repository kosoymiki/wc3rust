# P0-8 save/load + P0-9 multiplayer lockstep — agent-net report

**Task ref:** `wc3rust/revive-net-save` (monitor ledger 379)
**Scope:** `crates/wc3-sim/src/{save.rs,net.rs}` (new), `crates/wc3-sim/tests/{save_load.rs,multiplayer.rs}` (new),
`crates/wc3-sim/examples/save_net_cli.rs` (new), `analysis/agent-net/**` (owned),
`crates/wc3-sim/src/lib.rs` (two-line additive patch only).
**Date:** 2026-11-14.

## 1. Goal and result

Goal: make P0-8 (save/load: full state round-trip, identical state hash) and P0-9 (multiplayer
lockstep: 2 clients, deterministic, no desync) pass with mechanic-id tests, plus replay and a CLI smoke.

Result: **DONE** — all three required gates rc=0, plus a headless CLI smoke rc=0.

| gate | rc | result |
|---|---:|---|
| `cargo test -p wc3-sim --test save_load` | 0 | 10 passed; 0 failed |
| `cargo test -p wc3-sim --test multiplayer` | 0 | 10 passed; 0 failed |
| `cargo test -p wc3-sim --lib` | 0 | 27 passed; 0 failed |
| `cargo run -p wc3-sim --example save_net_cli -- smoke <dir>` | 0 | save→load→60 ticks→replay verified |

Logs: `analysis/agent-net/logs/gate-save-load.log`, `gate-multiplayer.log`, `gate-lib.log`.
Hashes of every artifact: `analysis/agent-net/logs/artifact-sha256.txt`.

## 2. Mechanic → test → rc

### `save.load` (`tests/save_load.rs`)

| test | proves |
|---|---|
| `save_load_full_state_roundtrip_600_ticks` | 600 ticks of melee (players, AI, creeps, production, harvesting, combat) → save → load → `state_hash`, `save_digest`, tick/seed/gold/lumber/AI equal, re-encode byte-identical; then 600 more ticks on both with the same command script and `state_hash` equal at **every 60 ticks**, final serialized state byte-identical |
| `save_load_every_entity_class_roundtrip` | every save section non-empty (items, camps, destructables+blocked cell, shops+stock, projectile+missile art, pending area cast, research, learned abilities, data cooldowns, order queue, rally, inventory, carrying, harvest timer, patrol, per-player upgrades) → load → byte-identical re-encode + field-by-field asserts |
| `save_load_rng_stream_preserved` | 64 subsequent `random()` draws equal (the RNG is state, not just a hash field) |
| `save_load_orders_known_scene_after_load` | an in-flight move order keeps driving the unit after load (120 ticks, hash equal every tick, final position equal) |
| `save_load_truncated_file_is_typed_error` | exhaustive truncation of a small save (`0..len`), sampled truncation of a realistic save: always a typed `SaveError`, never a panic |
| `save_load_corrupt_payload_is_typed_error` | bit flip → `Checksum`, bad magic → `BadMagic`, bad length → `LengthMismatch`, reserved flags → `Flags` |
| `save_load_path_and_io_roundtrip` | `save_to_path`/`from_save_path` round-trip; missing file → `SaveError::Io` |
| `save_load_worker_and_hall_ids_stable` | unit ids/owners/def indexes and `next_id` survive, so later commands and replays address the same units |

### `save.versioning` (`tests/save_load.rs`)

| test | proves |
|---|---|
| `save_versioning_unknown_version_rejected` | versions 0/2/99/65535 → `UnknownVersion(v)` **before** any payload parse; written version == `SAVE_FORMAT_VERSION` |
| `save_versioning_short_file_rejected` | 0/1/3/4/6/8/11-byte files, header-only, and header+declared-payload-without-bytes are all typed errors |

### `net.lockstep` (`tests/multiplayer.rs`)

| test | proves |
|---|---|
| `net_lockstep_two_clients_identical_state_hash_every_tick` | two peers, 6 broadcast frames (harvest/train/move/stop/hold from both seats), 300 ticks, `state_hash` compared **every tick**, identical per-tick hash history and final hash |
| `net_lockstep_seeded_rng_identical_on_both_seats` | same setup ⇒ same start hash and same 32-draw RNG stream; 60 ticks of AI with no commands stay equal |
| `net_lockstep_frame_clock_30hz` | exact 30 Hz conversion (500 ms ⇒ 15 ticks, 1000 ms ⇒ 30, 33 ms ⇒ 0, +1 ms ⇒ 1) and a 5-tick catch-up cap on a stall |
| `net_lockstep_late_frame_rejected` | frame for a past tick → `LateFrame`; duplicate `(tick,player)` → `DuplicateFrame`; seat 4 → `PlayerOutOfRange` |
| `net_lockstep_command_frames_roundtrip_all_variants` | all 12 `Command` variants through the frame codec, trailing bytes rejected |

### `net.desync` (`tests/multiplayer.rs`)

| test | proves |
|---|---|
| `net_desync_detected_on_diverging_client` | one resource point of local drift → `NetError::Desync { tick: 31, local, remote }` on the first diverging tick |
| `net_desync_detector_flags_remote_mismatch` | `DesyncDetector` flags a mismatching remote report for a known tick and ignores unknown ticks |

### `net.replay` (`tests/multiplayer.rs`)

| test | proves |
|---|---|
| `net_replay_record_and_replay_identical_hash` | 240-tick match recorded (setup+seed+frames+start/end hash), encode→decode equality, `verify()` reproduces `end_hash`, and the same replay through a file on disk reproduces it again |
| `net_replay_tampered_stream_detected` | altered frame → `HashMismatch`; dropped frame → error; bit flip → `Checksum`; 9 truncation cuts → typed errors |
| `net_replay_short_file_is_typed_error` | empty/short/wrong-magic/wrong-version replays are typed errors, never panics |

## 3. Formats (implemented, not sketched)

### Save `W3SV` v1 (`src/save.rs`)

```
magic "W3SV" | u16 version | u16 flags(0) | u32 payload_len | payload | u64 FNV-1a(payload)
```

Payload sections, each `u32` tag-prefixed (hexdump-readable, mis-order fails with `BadTag`):
header (tick, seed, **rng state**, ruleset, passive income) · grid (w, h, cell size, **bit-packed
walkable bitmap**) · id counters (unit/item/destructable/camp/projectile) · gold+lumber per player ·
fog (visibility + explored bitmaps per player) · full `GameplayConfig` · gold mines · destructables ·
camps · items · shops+stock slots · units (all 52 fields: orders, order queue, path, production,
research, inventory, cooldowns, construction, blocked cells, rally, carry state, hero stats) ·
projectiles · pending casts · AI controllers (config, mode, timers, expansion, target) ·
per-player upgrade maps · end marker.

**Forward-compatibility rule (documented in the module header, enforced by the reader):**
(1) only the exact `SAVE_FORMAT_VERSION` is accepted; any other value returns `UnknownVersion`
before parsing; (2) an append to an existing version is impossible because readers reject bytes after
the end marker (`TrailingBytes`) — a layout change must bump the version and add a `decode_vN` arm;
(3) old saves are read through explicit migration arms only, never by mutating an old tag's meaning.
Every `&'static str` field (ability ids, upgrade ids, item ids, missile art) is interned back to the
canonical data-table static on load; an unknown id is `UnknownName`, not a leak or a lossy copy.

On load, derived accelerators are rebuilt (`id_to_index`, pathfinder scratch, spatial hash,
path/flow scratch, fog buffers, event log) so the restored sim is `state_hash`-equal **immediately**,
before the next tick.

`Sim::save_digest()` = checksum over the encoded payload — a deep equality check that covers the
sections `state_hash` does not (destructables, items, projectiles, camps, shops, upgrades, fog).

### Replay `W3RP` v1 (`src/net.rs`)

Same envelope discipline: magic/version/flags/length/payload/checksum; payload = match setup
(seed, size, ruleset, passive income, layout + races) + start hash + end tick + end hash + command
frames. `Replay::verify()` rebuilds the match from the setup, applies frames by tick, and fails with
`HashMismatch` if either the start or the end hash differs — a replay is a proof, not a recording.

### Lockstep (`src/net.rs`)

`CommandFrame { tick, player, commands }`; every peer executes frames for tick *N* in
`(tick, player, arrival-seq)` order before stepping *N*, so all peers apply the same total order.
`LockstepPair` steps two peers and compares `state_hash` every tick (returns `Desync` on the first
divergence). `FrameClock` converts wall-clock ms to ticks at exactly 30 Hz (`elapsed_us * 30 /
1_000_000`, capped catch-up) — the simulation itself never reads a clock, so replay and lockstep
share one deterministic path.

## 4. CLI

`crates/wc3-sim/examples/save_net_cli.rs` (an example target, so no `Cargo.toml` edit and no conflict
with the sim owner):

```
cargo run -p wc3-sim --example save_net_cli -- smoke  <dir>
cargo run -p wc3-sim --example save_net_cli -- save   <file> [ticks]
cargo run -p wc3-sim --example save_net_cli -- load   <file>
cargo run -p wc3-sim --example save_net_cli -- replay <file>
```

Observed run (rc=0 each):

```
{"ok":true,"command":"smoke","dir":".../wc3cli-smoke","save_bytes":25211,"save_tick":600,
 "load_hash":14850616465994789469,"checkpoint_tick":660,"checkpoint_hash":14850616465994789469,
 "replay_frames":3,"replay_ticks":180,"replay_end_hash":6418539818547446042}
rc_smoke=0
{"ok":true,"command":"save","path":".../m.w3sv","bytes":24956,"tick":300,"hash":5408052965166554151}
rc_save=0
{"ok":true,"command":"load","path":".../m.w3sv","tick":300,"hash":5408052965166554151,"digest":4164131060478693647,"units":43}
rc_load=0
{"ok":true,"command":"replay","path":".../m.w3rp","frames":3,"ticks":180,"end_hash":6418539818547446042}
rc_replay=0
```

## 5. Patch list (existing files)

The sim owner was actively editing `crates/wc3-sim/src/lib.rs` (mtime moved at 02:08, 02:10, 02:13,
02:34, 02:43 during this work), so all changes to existing files are additive and patch-documented.

| # | file | change | status |
|---|---|---|---|
| `0009-sim-lib-add-save-net-modules.patch` | `crates/wc3-sim/src/lib.rs` | +`pub mod net;` +`pub mod save;` (2 lines) | **applied and verified**; patch reverse-applies cleanly against the current file |
| `apply-save-net-modules.sh` | — | idempotent re-apply + `cargo check` verification, for use if a wholesale lib.rs rewrite drops the two lines | available |
| `0010-gapmap-runner-save-net.patch` | `analysis/tech-lead/gen_gap_map.py`, `analysis/tech-lead/run_mechanic_tests.sh` | flips `persistence.save`/`net.multiplayer` to PORTED with implementation+test evidence, adds rows `save.load`, `save.versioning`, `net.lockstep`, `net.desync`, `net.replay`, adds the 5 matching runner entries | **not applied** — tech-lead owned; forward dry-run rc=0, coverage self-check PASS |

After applying 0010: `python3 analysis/tech-lead/gen_gap_map.py && bash analysis/tech-lead/run_mechanic_tests.sh --check`.

## 6. Defects found and fixed in-session

1. **Disk full (blocker for every agent).** `target/debug/incremental` had grown to 11.5 GB and the
   home filesystem hit 100% ("No space left on device" from rustc/mold). Removed the regenerable
   incremental cache (builds re-run with `CARGO_INCREMENTAL=0`), freeing 11.5 GB. No source or
   dependency artifacts were touched.
2. **Save codec asymmetry (8 bytes).** `R::len()` bounds element counts against the remaining payload,
   but was also used for plain scalars (`train_cycle`, `path_index`, `AiConfig.workers`,
   `Command::CancelTrain.index`), so a legal value (28) above the remaining byte count produced a
   false `Truncated`. Split into `R::len()` (sequence counts) and `R::count()` (plain scalars).
3. **`def_index` validation was too narrow.** Units legitimately reference indices beyond `ROSTER`
   (creeps/shops/extras are appended tables: `data::def_at` spans 133 defs). Validation now uses
   `data::def_at(...)` and the id interner covers all four tables, so creep/shop/extra ids and missile
   art intern correctly.

## 7. Open item outside this owner (recorded, not silently dropped)

`cargo test -p wc3-sim` (whole crate) currently shows **3 failures in `crates/wc3-sim/tests/ai.rs`**
(`ai_harvests_builds_tech_and_an_army`, `ai_sends_waves_that_reach_the_player_base`,
`ai_retreats_when_the_wave_is_broken`, e.g. "AI fielded an army: 0"). Evidence that this is the sim
owner's in-flight change, not save/net:

* `crates/wc3-sim/src/ai.rs` mtime **2026-11-14 02:37:32** (after the save/net change, before the run);
* `crates/wc3-sim/tests/ai.rs` mtime **2026-11-12 16:03:05** (unchanged expectations);
* save/net are not referenced by `ai.rs`, `tests/ai.rs`, or `Sim::step`; the `--lib` suite (27 tests,
  including the two save unit tests) and both new suites are green.

Owner: sim agent (active on `wc3-sim`). Next step: re-run `cargo test -p wc3-sim --test ai` once the
melee/AI edit settles; if it is a genuine regression of the new worker/camp logic it belongs to that
change, not to the save/net layer.

## 8. Risks / next

* `lib.rs` is under concurrent edit: if a wholesale rewrite drops `pub mod net; pub mod save;`, run
  `analysis/agent-net/patches/apply-save-net-modules.sh` (idempotent, verifies with `cargo check`).
* `GameplayConfig` is serialized field-by-field (34 scalars + 3 vectors + 2 arrays). If `wc3-re` adds a
  field, the save compiles but would carry the default until the codec is extended — the round-trip
  byte-equality tests are the detector. Worth a compile-time assertion when the struct stabilizes.
* Lobbies/matchmaking are explicitly out of scope for P0-9: the harness covers deterministic
  transport, desync detection and replay. `net.multiplayer`'s "lobbies" wording is a separate feature.
* Not run: on-device (Android) smoke of the CLI; the example is pure CPU and path-agnostic, so the
  host run is the meaningful evidence for this layer.

---

# FULL GOAL — 1v1 WC3 multiplayer parity over TCP (2026-11-14, phase 2)

**Result: DONE.** Gates `cargo test -p wc3-sim --test save_load --test multiplayer` rc=0 (10 + 16 tests)
and `--lib` rc=0 (27); two separate OS processes play a full 1v1 to victory with identical hashes.

## 1. Transport (`net.rs`, protocol `W3LS` v1)

* **Socket:** `std::net::TcpStream`, loopback `127.0.0.1:port`. `NetSession::host_with(addr, …)` binds and
  accepts exactly one peer; `connect_with(addr, …)` dials it. `host_on(listener, …)` accepts an
  already-bound listener so a caller can bind port 0 and hand over the port without a race (tests use it).
* **Seat assignment:** the host is seat 0 (`--seat 0`), the connector is seat 1 (`--seat 1`). Wrong seat
  for the role is rejected before any socket work (`NetError::SeatConflict`); equal seats in the hello
  exchange are rejected too.
* **Handshake (`MSG_HELLO`):** `u8 seat | u64 setup_digest | u64 start_hash | u32 setup_len | setup bytes`.
  Both sides compare the setup digest (whole `MatchSetup`, including the map scene) and the starting
  `state_hash`; a mismatch aborts with `NetError::Handshake`/`Desync` **before tick 0**.
* **Frame format (`MSG_BATCH`, every tick, both directions):**
  `u32 len | u8 kind=2 | u64 tick | u8 player | u32 count | [u32 cmd_len | command bytes]*` —
  each command is the same tagged encoding used by saves (`Command` codec in `save.rs`).
* **Hash report (`MSG_HASH`, every tick after stepping):** `u64 tick | u64 hash`.
* **Tick loop (strict barrier, v1: no rollback, no reconnect):**
  1. send local batch for tick T, 2. receive the remote batch for T, 3. apply frames in `(player)` order
  (lower seat first, independent of arrival order), 4. step + hash, 5. exchange hashes, compare, 6. `Desync
  { tick, local, remote }` on the first mismatch. A missing/duplicate/out-of-order tick is a `Protocol`
  error; a silent peer hits the 10 s socket timeout (`Transport("socket timeout")`).
* **30 Hz:** `FrameClock` converts cumulative wall-clock time to ticks with exact integer arithmetic
  (`elapsed_us * 30 / 1_000_000`) and caps catch-up; `NetSession` paces each tick to
  `started + tick * 33333 µs` when `pace = true` (CLI). Tests run `pace = false`; pacing never touches
  simulation state.

## 2. 1v1 scenario on the ready melee map

`MapScene::load(path)` opens the library map through `wc3_re::mapfiles::MapBundle` and extracts the
map's own data: WPM walkability at 32-unit cells, terrain-origin translation, `sloc` start markers
(Ponds: **2**), `ngol`/`ugol` mines, placed units (50 creeps) and `war3map.doo` destructables.
`MatchSetup::on_map(seed, scene)` rebuilds that scene in the sim and anchors **each seat at its own
`sloc` marker** (hall + workers + fighters; nothing is placed at an invented position).

The 1v1 test scenario keeps the map's starts/pathing/mines/destructables, clears neutral creeps and
splits the opening force (seat 0 raids with 6 fighters, seat 1 holds with 1). Victory is the sim's own
rule (`Event::Victory` when an owner has no units left).

## 3. Match result and hashes (observed)

| run | ticks | start hash | end hash | victory | frames | replay |
|---|---:|---:|---:|---:|---:|---|
| in-process pair (`net_match_1v1_map_starts_deterministic_victory`) | 3357 | — | `0x546f923741cae836` | 0 | 56 | verifies |
| TCP pair (`net_match_1v1_tcp_deterministic_victory`) | 3349 | — | identical on both peers | 0 | 56 | verifies |
| **two OS processes** (`run_cli_match.sh`) | 3349 | `16012080279573507020` | `13070281514868453602` | 0 | 56 | `replay_verified=true` on both |

CLI (two processes, each one JSON line, both rc=0):

```
seat0: {"ok":true,"command":"match","seat":0,"width":384,"ticks":3349,
        "start_hash":16012080279573507020,"end_hash":13070281514868453602,
        "victory":0,"frames":56,"replay_verified":true}
seat1: {"ok":true,"command":"match","seat":1,"width":384,"ticks":3349,
        "start_hash":16012080279573507020,"end_hash":13070281514868453602,
        "victory":0,"frames":56,"replay_verified":true}
agreement: start=16012080279573507020 end=13070281514868453602 victory=0 ticks=3349
CLI_MATCH_OK
```

## 4. Save/load during a match (task 3)

* `net_match_1v1_save_load_resume_identical_hash`: 180 ticks of a map match, both peers' `to_save_bytes`
  dropped ("quit"), both reloaded and continued through `LockstepPair::from_sims`; hashes checked every
  60 ticks up to tick 480 and the final serialized states are byte-identical.
* The match-end save from the two-process run reloads to the **match hash**:
  `load seat0.w3sv → {tick:3349, hash:13070281514868453602, units:14}` — the same `end_hash` both
  processes reported.
* Bottom line: `save.rs` is unchanged in its contract; the sim owner extended it to **format v2**
  (buffs/squads/order stamps) with a `decode_v1` migration, and the versioning test now follows the
  crate's `SAVE_FORMAT_VERSION` and accepts a v1→v2 mismatch only as a typed error.

## 5. CLI (task 4)

```
cargo run -p wc3-sim --example save_net_cli -- match --port 33000 --seat 0 --fast --replay out.w3rp --save out.w3sv
cargo run -p wc3-sim --example save_net_cli -- match --port 33000 --seat 1 --fast --replay out1.w3rp
```

`--fast` disables pacing (tests/smoke); without it the session holds 30 Hz. Exit code 0 on a completed
match, 2 on any transport/handshake/replay failure. `bash analysis/agent-net/run_cli_match.sh` runs both
processes and asserts hash/victory agreement.

## 6. Tests (task 5) — `cargo test -p wc3-sim --test multiplayer` → **16 passed, rc=0**

| mechanic | tests |
|---|---|
| `net.lockstep` | in-process 300 ticks/6 frames per-tick hash; **TCP** two sessions 120 ticks with commands from both seats and per-tick hash equality; seeded RNG; 30 Hz clock; frame codec (all `Command` variants); late/duplicate frame rejection; seat conflict + setup-mismatch handshake |
| `net.desync` | in-process drift at tick 31; **TCP** drift detected on the diverging tick with both hashes; `DesyncDetector` remote mismatch |
| `net.replay` | 240-tick record→encode→decode→verify; file round trip; tampered/dropped frame → `HashMismatch`; bit flip → `Checksum`; truncations → typed errors; TCP session replay (`Replay::from_session`) verifies the transport match |
| `net.match_1v1` | map starts + deterministic victory (in-process), TCP deterministic victory with identical per-tick hashes, save/load resume with identical hashes |

## 7. Fixes made in this phase

1. **Stale derived state:** my loader rebuilt `fog_offsets` with a hardcoded 25-ring table while
   `Sim::new` had grown to 65 rings → `index out of bounds` on a real map (32-unit cells, radius 29).
   The ring table is not save state; it is now left to `Sim::new` and only `fog_stamp` is reset.
2. **Test flake:** TCP tests took a free port, dropped the listener and re-bound in another thread
   (port race under parallel load) — replaced with `host_on(listener)`, listener bound once and handed over.
3. **Retry masked real errors:** `connect_retry` retried after a *successful connect that failed the
   handshake*, reporting "connection refused" instead of the handshake error; non-transport errors are
   now final.
4. **Replay bloat:** every tick logged both seats' frames including empty ones (6698 frames per match);
   only non-empty frames are recorded (56).

## 8. Patch (existing file)

`analysis/agent-net/patches/0011-net-match-1v1-row.patch` — adds the `net.match_1v1` gap-map row
(`analysis/tech-lead/gen_gap_map.py`) and its runner target (`mechanic_targets.tsv`). Forward dry-run
rc=0; tech-lead applies it (`python3 analysis/tech-lead/gen_gap_map.py`).
