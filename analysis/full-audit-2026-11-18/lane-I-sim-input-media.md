# LANE I — simulation, input, media (audit report)

Ledger: **529** (`LEDGER-529-AUDIT-LANE-I`, monitor id 532)
Source of truth: `/storage/emulated/0/Download/WC3RUST_FULL_AUDIT_TZ_P0-P4.md` §0, §5.8, §5.10, §5.14, §6, §7, §11
Assignment: `analysis/tech-lead/AUDIT-ASSIGNMENT-2026-11-18.md` §I
Red-team partner: lane J (ledger 530)

## 0. What was audited (exact state)

| item | value |
|---|---|
| repository | `~/src/wc3rust` |
| HEAD | `d7af459daf5c507d46c8a63741385e5d1091af26` (`docs(sim): record the landed donor units …`) |
| working tree at audit time | HEAD + **182 uncommitted modified files** (product lanes landing in parallel; lane I does not touch them) |
| lane I file set | `crates/wc3-sim/**`, `crates/wc3-input/**`, `crates/wc3-audio/**`, `crates/wc3-anim/**`, `crates/wc3-assetdb/**`, `crates/wc3-replay/**` = **203 files, 159 212 lines** (`FILE_COVERAGE.csv`, reviewer `I/529`) |
| build tool | `scripts/h2-cargo` only (FIFO), never `H2_CARGO_PRIORITY` |
| operator constraint | **no new tests, nothing under `tests/`** — evidence is `file:line` + one-shot probe + exact output |

Cited files were hashed so line numbers can be re-anchored if the tree moves:

```
ff3ada65e77242b8  crates/wc3-sim/src/lib.rs
7af90b18b9a1b66f  crates/wc3-sim/src/save.rs
a73c6b506b693bae  crates/wc3-replay/src/lib.rs        (after the lane-I fix, see WC3-RPL-001)
f5185e883162aef6  crates/wc3-input/src/session.rs
4e71762f07150b58  crates/wc3-input/src/geometry.rs
6cc175c062f31415  crates/wc3-assetdb/src/lib.rs
950bef7b1e2bdb43  crates/wc3-assetdb/src/lru.rs
8704b15fde961cb4  crates/wc3-anim/src/state.rs
8db89fb2b08dda3d  crates/wc3-audio/src/engine.rs
99d5f3f4b2a2ae69  crates/wc3-audio/src/engine/sdl3.rs
48a5e0031622c66b  crates/wc3-audio/src/game_audio.rs
16a56e2716fbaf37  crates/wc3-game/src/game.rs            (read-only, lane J's file)
fbf878647d5fca1c  apps/android-native/src/lib.rs         (read-only, lane K's file)
```

Probes (sources kept, transient `crates/*/examples/` copies moved to
`probes/lane-I/build-copies/` so the crate trees stay clean):

```
analysis/full-audit-2026-11-18/probes/lane-I/audit_i_hash.rs    (state_hash coverage matrix)
analysis/full-audit-2026-11-18/probes/lane-I/audit_i_dup.rs     (duplicate command matrix)
analysis/full-audit-2026-11-18/probes/lane-I/audit_i_mirror.rs  (gesture mirror)
analysis/full-audit-2026-11-18/probes/lane-I/audit_i_replay.rs  (replay roundtrip / allocation / ticks)
```

## 1. Answers to the four priority questions

### (a) Determinism — does `state_hash`/`save_digest` cover every mutable per-seat field after the 8-seat change and save v9?

**Split answer: `save_digest` (and save v9) yes; `state_hash` no.**

Save v9 is complete and validated. It stores every seat (gold, lumber, `explored`, `upgrades`),
the seat count and the bound human seat, and rejects a mismatch:

- `crates/wc3-sim/src/save.rs:147` `SAVE_FORMAT_VERSION = 9`; `:150` `seats_for_version` → 8 for v9;
- `:4327-4334` `SEC_RESOURCES` writes gold+lumber for all 8 seats;
- `:4336-4340` `SEC_FOG` writes `visibility` + 8 `explored` grids;
- `:4429-4436` `SEC_UPGRADES` writes 8 upgrade maps;
- `:4558-4561` `SEC_SEATS` writes `MAX_PLAYERS` + `opt_u8(human_seat)`;
- `:5011-5031` decode validates `seats == MAX_PLAYERS` and `human_seat < MAX_PLAYERS`,
  `:5037` rejects trailing bytes.
- Measured: probe `audit_i_hash` → `PROBE save_v9_upgrade_roundtrip stored=Some(3) restored=Some(3) reencode_identical=true`,
  and a seat-7 bank survives (`seat7_bank state_hash_same=false gold7=4321 lumber7=123`).

`state_hash` (the lockstep divergence detector, `net.rs:1103/1124/1154-1172`) feeds tick, 8×gold,
8×lumber, rng, mines, and a **subset** of unit fields (`lib.rs:7074-7281`). Fourteen live-state
mutations leave it unchanged (probe `audit_i_hash`, exact output in §5):

```
carrying_lumber+5      state_hash_same=true   save_digest_same=false
harvest_timer+10       true                   false
stunned_until+30       true                   false
channel_until+30       true                   false
hero_xp+50             true                   false
learned+push           true                   false
research_push          true                   false
items+push             true                   false
goal+move              true                   false
order_queue_retarget   true                   false   (variant is hashed, its payload is not)
upgrades_seat7+Rhme    true                   false
destructable_hp-10     true                   false
item_charges-1         true                   false
projectile_damage+1    true                   false
projectile_target_clear true                  false
```

and a real 300-tick run stays hash-equal while the two sims differ by 5 carried lumber:
`PROBE carried_lumber_divergence first_hash_diff_tick=None tick_end=328`.

So: seats beyond 1 **are** in the hash (gold/lumber `for seat in 0..MAX_PLAYERS`), but the
per-seat **tech table** (`upgrades: [BTreeMap<…>; MAX_PLAYERS]`, `lib.rs:976`) is not, and
neither is most of the unit state the save carries. `state_hash`'s doc comment says
“Deterministic FNV-1a hash of the simulation state” (`lib.rs:7068-7072`) — the claim is broader
than the implementation. See **WC3-SIM-001**.

### (b) Orders/commands — edge cases that silently drop or duplicate

Measured matrix (probe `audit_i_dup`, exact output in §5):

| command | two copies in one tick | two copies across ticks | verdict |
|---|---|---|---|
| `Move` / `MoveAttack` | same final order/path | same | idempotent |
| `Attack` | cooldown unchanged (19/19), same target | same | idempotent |
| `Harvest` | same state; gather timer re-armed | timer reset 59→60, **0 gold lost** over 300 ticks (`lost_gold=0`) | benign |
| `Train` | 2 queued, **double charge** (160 g) | ×2 | by design (1 click = 1 unit) |
| `CancelTrain{index:0}` on a 3-deep queue | **2 units cancelled, 320 g refunded** (`queued 3→1`) | — | **defect** |
| `Build` | **2 buildings spawned, 900 g charged twice** (`buildings 2→3`, `gold 1000→100`) | — | **defect** |
| `ReviveHero` / `LearnAbility` / `Research` | rejected by existing guards | — | idempotent |

Root cause for the two defects: `Command::Build`/`CancelTrain` are applied in `Sim::apply`
(`lib.rs:1682-1886`) with no dedupe and no placement validation —
`build_instant_impl` (`lib.rs:4978`… `5978-6013`) checks builder/requirements/cost only and never
calls the `place_denial` predicate the ghost and the card use (`lib.rs:6024`), so the second copy
finds nothing to stop it. `CancelTrain` removes by **index**, so the second copy removes the next
entry and refunds it (`lib.rs:1797-1814`).

Second root cause: the documented duplicate-click guard is **API-scoped**. `Sim::issue`
(`lib.rs:1899-1931`) drops a repeated identical `Move`/`AttackMove` on the same tick via
`issue_stamps` (`lib.rs:1934-1943`), but the `Command` path used by the game for smart orders
(`crates/wc3-game/src/game.rs:6007`, `:6339`, `:6800-6830`) and by lockstep/replay
(`sim.step(&commands)`) never consults it. Probe: `issue same_tick_suppressed=true next_tick_reissued=true`.

### (c) Gesture router — can a mirrored/double event reach the world as two orders?

**Inside the crate: yes for the mouse mirror; in the shipped game: no, three downstream guards catch it.**

Probe `audit_i_mirror` (exact output §5):

- one clean finger tap → 1 `pointer_down`, 1 `pointer_up`;
- **finger tap + synthetic mouse pair → 2 `pointer_down`, 2 `pointer_up`** (the router has no
  source arbitration: `session.rs:452-464` dispatches `MouseDown` to `sink.pointer_down`
  unconditionally, `:217-231` does the same for `FingerDown`);
- duplicate `FingerDown` with the **same** id → suppressed (`session.rs:218-221` returns early);
- duplicate `FingerDown` with a **new** id → pinch → **two-finger tap**
  (`pinch_end:two_finger_tap`), which the game maps to `right_click` → an order
  (`game.rs:5963-5968`, **not** behind the mirror guard);
- stale `FingerUp` for an unknown id → ignored; `reset()` mid-tap → `pointer_cancel`, no up.

Three guards now sit downstream of the router, all outside `wc3-input`:

1. `apps/android-native/src/lib.rs:2272` `SDL_SetHint("SDL_TOUCH_MOUSE_EVENTS", "0")`;
2. `apps/android-native/src/lib.rs:1869-1876` compiles the mouse arms out on Android
   (`cfg!(target_os = "android")`) — the measured note there says one `input tap` produced two
   complete DOWN/UP gestures 4 ms apart;
3. `crates/wc3-game/src/game.rs:5919-5927` `is_mirrored_tap` drops a second `Tap` within
   60 ms / 6 px (`:5804` for menu activation, `:5908` for world taps), with
   `GAME_INPUT_MIRROR_SUPPRESSED` telemetry (`:5665`) — this is the guard the lane-510 capture
   (id 1050) records as the fix for “tap CUSTOM GAME then the mirrored tap lands on the RACE tab”.

So the current build does not double-order from a mirrored tap; the crate contract alone would, and
the two paths that are *not* behind `is_mirrored_tap` are `pinch_end → right_click`
(`game.rs:5963-5968`) and any non-`Game` consumer of `InputState`. See **WC3-INP-001** (downgraded
to P4 on this evidence).

### (d) audio / anim / assetdb — unbounded cache growth, PCM budget, animation state leaks

No unbounded growth found; three smaller defects:

- **assetdb**: byte-budgeted LRU with Arc pinning (`lru.rs:29-285`) is correct (eviction skips
  pinned entries, retries next sweep, `pinned_skipped >= len` break). Negative cache is bounded
  (`lib.rs:505-509`, clear at 8192). Budgets 64/96/48/32 MB (`lib.rs:112-124`).
  **But `AssetStats` lies**: `stats()` hardcodes `negative_hits/texture_hits/texture_misses/
  model_hits/model_misses/sound_hits/sound_misses = 0` (`lib.rs:719-733`) while only `hits/misses`
  are incremented — the module doc promises “a leak or a budget thrash is visible rather than
  silent”. Texture/model/sound thrash is therefore invisible. See **WC3-ADB-001**.
- **audio PCM**: ambience cache is byte-bounded (64 MB, `game_audio.rs:38-106`) and documented to
  keep one oversized theme alone (soft cap, not a defect). Voice pool is fixed at 64 slots with
  preemption/steal (`engine.rs:634-766`); no per-frame allocation on the render path
  (`engine.rs:6-12` design note). No leak found.
- **anim**: the crate has **no per-unit animation state** (`state.rs` is a pure function), so no
  leak is possible there. The game keeps `visuals: BTreeMap<u32, UnitVisual>` and prunes it
  (`game.rs:4570` `retain(|id,_| live.contains(id))`), death visuals age out (`:4568`). No leak.
  Real issue: the corrected selector `wc3_anim::state::select_state` has **no production caller**
  (only `crates/wc3-anim/tests/assets_anim_ingame.rs`), while `game.rs:4497-4531` re-implements
  the same priority — two sources of truth, one of them dead. See **WC3-ANM-001**.

## 2. Findings (TZ §7 template)

Index (final tally): **1×P1, 1×P2, 3×P3, 6×P4 = 11 findings, 0 NEEDS_REPRO, 0 FALSE_POSITIVE**
(the §2.1 list holds the disproved hypotheses).

| ID | P | one line | status |
|---|---|---|---|
| WC3-RPL-001 | P1 | replay crate does not compile (3 Command variants unencoded) | CONFIRMED, fixed by lane I |
| WC3-SIM-001 | P2 | `state_hash` blind to 14 live fields → lockstep detection gap | CONFIRMED |
| WC3-RPL-002 | P3 | replay read/verify trust announced lengths (226 GB VA, 2^64-tick loop) | CONFIRMED |
| WC3-SIM-002 | P3 | duplicate `CancelTrain`/`Build` double-apply; guard only in `issue()` | CONFIRMED |
| WC3-ADB-001 | P3 | `AssetStats` seven counters hardcoded to 0 | CONFIRMED |
| WC3-INP-001 | P4 | `InputState` double-dispatches a mirrored gesture (guarded downstream ×3) | CONFIRMED |
| WC3-ANM-001 | P4 | corrected animation selector has no production caller | CONFIRMED |
| WC3-SIM-003 | P4 | `Sim::buffs` doc claims a sort that does not exist | CONFIRMED |
| WC3-SIM-004 | P4 | `ugol` defined in two tables, one row shadowed | CONFIRMED |
| WC3-AUD-001 | P4 | SDL3 stream opener binds the callback slot as `*mut c_void` | CONFIRMED |
| WC3-RPL-003 | P4 | `write_ids` truncates the unit count to `u16` | CONFIRMED |

### WC3-RPL-001 — `wc3-replay` does not compile: three `Command` variants are unencoded

```
ID: WC3-RPL-001
Title: Replay crate cannot be built — encode/decode cover 14 of 17 wc3_sim::Command variants
Priority: P1
Status: CONFIRMED  (fix applied by lane I in the working tree, see below)
Confidence: HIGH
Affected commit: d7af459 (regression introduced by b9ccb59, 2026-11-18)
File(s): crates/wc3-replay/src/lib.rs, crates/wc3-sim/src/lib.rs
Exact line range(s): crates/wc3-replay/src/lib.rs:164-270 (encode_command), :272-357 (decode_command);
                     crates/wc3-sim/src/lib.rs:791-805 (Patrol, UpgradeBuilding, Research)
Affected targets/features: every workspace build (`cargo build/test --workspace`), replay recording
                     of patrol / building upgrade / research orders, apps/headless replay verify
Preconditions: none — the crate fails to type-check as committed
Trigger / minimal reproduction:
    scripts/h2-cargo check -p wc3-replay
Observed behavior:
    error[E0004]: non-exhaustive patterns: `&wc3_sim::Command::Patrol { .. }`,
    `&wc3_sim::Command::UpgradeBuilding { .. }` and `&wc3_sim::Command::Research { .. }` not covered
    --> crates/wc3-replay/src/lib.rs:166:11
    error: could not compile `wc3-replay` (lib) due to 1 previous error
    EXACT_RC=101
Expected behavior: the workspace compiles; every Command the sim can emit is encodable.
Root cause: b9ccb59 added the three variants to `wc3_sim::Command` and the `apply` arms but did not
    touch the replay crate; the exhaustive `match` in `encode_command` therefore stopped type-checking.
Impact: no workspace-wide build or test run is possible at HEAD; replay cannot record the three
    new card orders; any lane that runs `cargo test --workspace` sees a red tree for an unrelated reason.
Why not covered by excluded backlog: AUD-030 covered *missing* replay arms 10-13 (LearnAbility/
    Repair/HarvestLumber/ReviveHero). This is a different, later regression (b9ccb59) on three other
    variants, with a compile error rather than a silent drop.
Provenance: lane 510's gate run hit the same E0004 on 2026-11-18 05:xx and recorded it as
    `captured` in capture 1045/1046 (“the gate naming the failing file is the desired behaviour”),
    owner “lane that added the variants”. It was still open at HEAD when lane I audited and is fixed
    here; lane I claims the fix, not the discovery.
Fix specification: add `Command::Patrol` (tag 14: ids + a.x/a.y/b.x/b.y), `UpgradeBuilding` (15:
    u32 + u16-prefixed def id) and `Research` (16: u32 + u16-prefixed upgrade id); decode interns the
    id with `wc3_sim::save::intern_static_name` (same as LearnAbility). Applied by lane I at
    crates/wc3-replay/src/lib.rs:274-295 (encode) and :389-404 (decode) — see §5 for the round-trip proof.
Regression test(s): BLOCKED: operator test freeze. Manual: `scripts/h2-cargo check -p wc3-replay`
    must stay rc=0; the 17-variant round-trip probe must print `ok=17`.
Verification commands and exact result:
    before: `scripts/h2-cargo check -p wc3-replay` → error[E0004], EXACT_RC=101
    after:  `scripts/h2-cargo check -p wc3-replay` → `Finished dev profile`, RC=0
            `audit_i_replay roundtrip` → `PROBE roundtrip variants=17 ok=17`, RC_ROUNDTRIP=0
Related findings: WC3-RPL-002, WC3-RPL-003
```

### WC3-SIM-001 — `state_hash` is blind to most live simulation state, so lockstep divergence detection is incomplete

```
ID: WC3-SIM-001
Title: state_hash omits carrying_lumber, harvest_timer, stun/channel timers, XP/learned, research,
       item holdings/charges, goal, queued-order payloads, per-seat upgrades, destructables and
       in-flight projectiles
Priority: P2
Status: CONFIRMED
Confidence: HIGH
Affected commit: d7af459 (working tree)
File(s): crates/wc3-sim/src/lib.rs, crates/wc3-sim/src/net.rs
Exact line range(s): lib.rs:7074-7281 (state_hash), :976 (per-seat upgrades), :913-1003 (Sim fields);
                     net.rs:1103, :1124, :1154-1172 (hash used as the divergence gate)
Affected targets/features: `LockstepPair`/`LockstepSession` divergence detection, replay final-hash
                     check, every `state_hash` consumer
Preconditions: two peers (or a replay) whose live state diverges in a field the hash does not feed
Trigger / minimal reproduction:
    cp analysis/full-audit-2026-11-18/probes/lane-I/audit_i_hash.rs crates/wc3-sim/examples/
    scripts/h2-cargo run -p wc3-sim --example audit_i_hash
Observed behavior: 14 single-field mutations return an identical `state_hash` (matrix in §1a/§5);
    a 300-tick run with a 5-lumber carry difference never changes the hash:
    `PROBE carried_lumber_divergence first_hash_diff_tick=None tick_end=328`
Expected behavior: two sims that differ in any live state produce different hashes (or the hash is
    documented as a coarse checksum and the peers exchange `save_digest`).
Root cause: state_hash feeds a hand-picked field list. It covers `carrying` but not
    `carrying_lumber`; it feeds the `order_queue` *variant tag* but not its target/point payload
    (lib.rs:7164-7177: `feed(match spec { … })` only); it never feeds `upgrades`, `research`,
    `items`, `xp`, `learned`, `harvest_timer`, `stunned_until`, `channel_until`, `goal`,
    destructables or projectiles. `save_digest` (`save.rs:5688`) covers all of them.
Impact: a desync in carried resources/tech/items/projectiles is invisible for the whole walk-back
    window (measured ≥300 ticks) and only surfaces later as a combat-number mismatch, which is
    exactly the case lockstep hashes exist to catch early. This is detection quality, not a new
    gameplay bug: the two peers were already diverged.
Why not covered by excluded backlog: the old backlog has no state_hash item; the 8-seat work
    (ledger 518) only extended the *seat* part of the hash (gold/lumber), which is correct here.
Fix specification: either (a) feed the missing live fields (carrying_lumber, harvest_timer,
    stunned_until, channel_until, xp, learned ids, research (id, remaining), item ids + charges,
    goal, the full OrderSpec payload, upgrades per seat, destructable hp/alive, projectile
    fields), or (b) drop the "simulation state" claim, rename to `tick_hash`, and make lockstep
    compare `save_digest` (already complete) every N ticks. (b) costs one encode per N ticks;
    (a) costs a handful of feeds per entity per tick.
Regression test(s): BLOCKED: operator test freeze. Manual reproducer: the probe above must print
    `state_hash_same=false` for every live-state row.
Verification commands and exact result: probe output quoted in §1a and §5; existing
    tests/determinism.rs (3 tests) and tests/save_load.rs (14 tests) pass (`RC=0`) — they compare two
    identical runs / a save round-trip, so they cannot detect an omitted field.
Related findings: WC3-SIM-002
```

### WC3-RPL-002 — `Replay::read` trusts announced lengths; `verify` trusts announced tick count

```
ID: WC3-RPL-002
Title: A 44-byte replay file reserves 226 GB of address space; a 44-byte replay with ticks=u64::MAX
       makes verify loop for 2^64 ticks
Priority: P3
Status: CONFIRMED
Confidence: HIGH
Affected commit: d7af459
File(s): crates/wc3-replay/src/lib.rs
Exact line range(s): :104-112 (`Vec::with_capacity(count)`, `vec![0u8; len]`), :141 (`for tick in 0..self.ticks`)
Affected targets/features: `apps/headless` replay verification (`apps/headless/src/main.rs:144-145`),
                     any tool that opens an operator-supplied/third-party `.h2rp`
Preconditions: a replay file whose `count`/`len`/`ticks` fields are larger than the file
Trigger / minimal reproduction:
    cp analysis/full-audit-2026-11-18/probes/lane-I/audit_i_replay.rs crates/wc3-replay/examples/
    scripts/h2-cargo run -p wc3-replay --example audit_i_replay -- alloc
    scripts/h2-cargo run -p wc3-replay --example audit_i_replay -- record_len
    timeout 6 <binary> ticks
Observed behavior:
    alloc: `input_bytes=44 announced_records=4294967295 vm_before="VmSize: 2167056 kB"`
           `vm_after="VmSize: 237048080 kB"` (≈226 GiB VA) then `Err(UnexpectedEof)`
    record_len: 57-byte file announces a 256 MiB command blob; RSS unchanged (calloc pages are not
           touched because read_exact fails first) — reservation, not commit, on this kernel
    ticks: `announced=18446744073709551615 records=0`, killed by `timeout` → RC=124
Expected behavior: reader rejects `count`/`len` that cannot fit in the remaining input (or a fixed
    cap), and `verify` refuses `ticks` beyond a sane bound (or drives the loop from the record ticks).
Root cause: both fields are read from the file and used as allocation/loop bounds with no relation
    to `reader` length.
Impact: malformed replay → 226 GiB VA reservation (abort under any VA limit / 32-bit target) and a
    non-terminating verify. Local file input, no remote surface → P3, not P1.
Why not covered by excluded backlog: AUD-* covers save (`MAX_SAVE_BYTES`) and MPQ limits; the replay
    container has no equivalent cap and is a different format/parser.
Fix specification: cap `count` at `data.len().min(MAX_RECORDS)` / `MAX_RECORD_BYTES`, use
    `read_exact` into a bounded buffer, and reject `ticks > records.last().tick + 1` (or clamp the
    loop to the last recorded tick + 1).
Regression test(s): BLOCKED: operator test freeze. Manual reproducer: the three runs above.
Verification commands and exact result: quoted verbatim above (RC_ALLOC=0 with VmSize jump,
    RC_RECLEN=0, RC_TICKS=124).
Related findings: WC3-RPL-001
```

### WC3-SIM-002 — Duplicate `CancelTrain` cancels two queued units and refunds both; duplicate `Build` double-charges and spawns two buildings

```
ID: WC3-SIM-002
Title: apply() has no duplicate guard and build_instant_impl has no placement validation, so a
       repeated CancelTrain/Build command applies twice
Priority: P3
Status: CONFIRMED
Confidence: HIGH
Affected commit: d7af459
File(s): crates/wc3-sim/src/lib.rs
Exact line range(s): :1797-1814 (CancelTrain by index), :1842-1847 and :5978-6013 (Build →
                     build_instant_impl), :1899-1943 (spam guard only in issue()), :6024 (place_denial)
Affected targets/features: command stream from replay/lockstep/scripts, the game's smart-order and
                     train/cancel buttons (game.rs:6007/6339/6800-6830 push `Command`s)
Preconditions: two identical CancelTrain/Build commands reach one `Sim::step` (mirror, replay,
    duplicated input, a script that issues twice)
Trigger / minimal reproduction:
    cp analysis/full-audit-2026-11-18/probes/lane-I/audit_i_dup.rs crates/wc3-sim/examples/
    scripts/h2-cargo run -p wc3-sim --example audit_i_dup
Observed behavior:
    G cancel_train queued_before=3 queued_after_dup=1 gold=520->840 refund=320
    H build_x1 buildings=2 gold=550 | build_x2 buildings=3 gold=100
Expected behavior: the second copy is either rejected (already cancelled / site occupied) or
    treated as a no-op; at minimum the placement predicate used by the ghost must gate the spawn.
Root cause: CancelTrain removes `production[index]` with no identity check (the second copy removes
    the *next* item); `build_instant_impl` never calls `place_denial` (footprint/occupancy), so a
    second spawn on the same cell succeeds and is charged again.
Impact: double refund + two units lost from the queue; 2× building cost and two overlapping
    buildings from one duplicated click. The Android mirror is suppressed at the app layer, so the
    ordinary tap path is protected today (that is the only reason this is P3, not P2).
Why not covered by excluded backlog: AUD-* has save-ID/MPQ/HUD items; no duplicate-command item.
Fix specification: make CancelTrain carry the unit def (or verify the entry matches) and make
    `build_instant_impl` call `place_denial`-equivalent geometry/occupancy checks before the debit;
    optionally extend `issue_stamps`-style dedupe to the `Command` path.
Regression test(s): BLOCKED: operator test freeze. Manual reproducer: probe above must print
    `queued_after_dup=2` and `build_x2 buildings=2`.
Verification commands and exact result: quoted above; `A/B/D` rows show Move/Attack/Harvest are
    idempotent, `C` shows Train doubles by design, `E/F` show the guard is per-tick and only in issue().
Related findings: WC3-SIM-001, WC3-INP-001
```

### WC3-INP-001 — `InputState` dispatches a mirrored finger+mouse gesture twice; every guard lives downstream

```
ID: WC3-INP-001
Title: wc3-input has no source arbitration between FINGER_* and synthesized MOUSE_* events
Priority: P4
Status: CONFIRMED (crate-level only; the shipped Android+game path is guarded three times)
Confidence: HIGH
Affected commit: d7af459
File(s): crates/wc3-input/src/session.rs; guards in apps/android-native/src/lib.rs and
         crates/wc3-game/src/game.rs (read-only for lane I)
Exact line range(s): session.rs:437-485 (`apply`), :217-231 (`finger_down`), :452-464 (MouseDown/Up);
                     apps/android-native/src/lib.rs:2272 (SDL_TOUCH_MOUSE_EVENTS=0), :1869-1876 (cfg);
                     game.rs:5919-5927 (is_mirrored_tap 60 ms/6 px), :5804, :5908, :5665 (telemetry)
Affected targets/features: the crate contract; non-Game consumers of InputState; the
                     `pinch_end → right_click` path (game.rs:5963-5968) which has no mirror guard
Preconditions: SDL delivers a synthesized mouse pair for the same physical touch
Trigger / minimal reproduction:
    cp analysis/full-audit-2026-11-18/probes/lane-I/audit_i_mirror.rs crates/wc3-input/examples/
    scripts/h2-cargo run -p wc3-input --example audit_i_mirror
Observed behavior:
    2 mirrored_tap pointer_down=2 pointer_up=2
    3 duplicate_same_id pointer_down=1 (suppressed)
    4 duplicate_new_id pinches=1, event `pinch_end:two_finger_tap` (→ right_click order)
Expected behavior: the router treats a finger and its synthesized mouse twin as one gesture,
    independent of platform and of downstream heuristics.
Root cause: `MouseDown` unconditionally calls `sink.pointer_down`; `FingerDown` does too; there is no
    synthetic/real flag in `InputEvent` and no cross-family dedupe.
Impact: in the current build the double gesture is neutralised by three layers (SDL hint off,
    Android cfg filter, `is_mirrored_tap` 60 ms/6 px with `GAME_INPUT_MIRROR_SUPPRESSED` telemetry),
    so no double order was reproduced end-to-end. What remains: (i) `pinch_end → right_click` is not
    behind `is_mirrored_tap`; (ii) a 60 ms/6 px heuristic is time-based, so a mirrored event delayed
    past the window (scheduling stall, paused clock) is admitted; (iii) any consumer that is not
    `Game` inherits the double dispatch. P4: contract gap with a working workaround, not a live bug.
Why not covered by excluded backlog: the old backlog's touch items were HUD/lifecycle; this is the
    event-source arbitration contract.
Fix specification: add `InputEvent::Mouse{…, synthetic: bool}` (or a session-level latch cleared by
    the first FINGER_DOWN and re-armed on FINGER_UP/cancel) and drop synthetic mouse events while a
    touch pointer is active; keep real-mouse desktop behaviour. Then `is_mirrored_tap` becomes
    defence in depth rather than the only guard.
Regression test(s): BLOCKED: operator test freeze. Manual reproducer: row 2 above must print
    `pointer_down=1`.
Verification commands and exact result: quoted above (RC=0); the three downstream guards were read
    at the cited lines in the current working tree.
Related findings: WC3-SIM-002
```

### WC3-ADB-001 — `AssetStats` reports seven counters that are hardcoded to zero

```
ID: WC3-ADB-001
Title: texture/model/sound/negative hit-miss counters are never incremented, so cache thrash is invisible
Priority: P3
Status: CONFIRMED
Confidence: HIGH
Affected commit: d7af459
File(s): crates/wc3-assetdb/src/lib.rs
Exact line range(s): :719-733 (stats() literals), :456-465 and :521-545 (only shared hits/misses move)
Affected targets/features: asset-cache observability used by QA tools (`asset_catalog`,
                     `audio_bench`) and by the operator's memory-budget questions
Preconditions: any run with texture/model/sound cache pressure
Trigger / minimal reproduction: `grep -rn "texture_hits\|sound_hits\|model_hits\|negative_hits" crates/ \
    --include=*.rs` → only the struct definition and the `0` literals; `stats()` is the sole writer.
Observed behavior: `AssetStats { texture_hits: 0, texture_misses: 0, model_hits: 0, model_misses: 0,
    sound_hits: 0, sound_misses: 0, negative_hits: 0, … }` regardless of traffic; `hits`/`misses`
    aggregate every cache, so raw/texture/model/sound cannot be separated.
Expected behavior: each cache increments its own hit/miss; the doc comment at lib.rs:6-10 (“stats()
    reports … so a leak or a budget thrash is visible rather than silent”) holds.
Root cause: the counters were added to the struct but never wired into `texture_shared`,
    `model_shared`, `sound_shared` and the negative-cache path.
Impact: a texture/model cache that thrashes or a pinned-entry leak cannot be measured from the
    product; the workaround is the `evictions`/`pinned_skips` totals only.
Why not covered by excluded backlog: no asset-stats item in AUD-001…032.
Fix specification: add per-cache counters (or return an `EvictionReport` from `get`) and fill the
    seven fields; keep `hits/misses` as the aggregate.
Regression test(s): BLOCKED: operator test freeze. Manual: read stats twice around a texture load and
    compare `texture_hits`/`texture_misses`.
Verification commands and exact result: source grep above; also `tests/*` never assert these fields
    (`grep -rn "texture_hits" crates/*/tests` → no hits).
Related findings: none
```

### WC3-ANM-001 — The corrected animation-state rule has no production caller

```
ID: WC3-ANM-001
Title: wc3_anim::state::select_state (the documented corrected priority) is only used by tests;
       game.rs keeps a second implementation
Priority: P4
Status: CONFIRMED
Confidence: HIGH
File(s): crates/wc3-anim/src/state.rs, crates/wc3-game/src/game.rs (read-only)
Exact line range(s): state.rs:1-8 (doc: “game.rs should delegate to select_state”), :69-102;
                     game.rs:4384-4531 (second implementation)
Affected targets/features: unit/building animation selection, future regression of the two defects
                     the rule was written for (attack-while-walking, Birth for construct sites)
Trigger: `grep -rn "select_state" --include=*.rs .` → crates/wc3-anim/src/lib.rs:32 (re-export),
    tests/assets_anim_ingame.rs:12+; zero callers in wc3-game/wc3-render.
Observed behavior: the crate ships an unused rule; the game derives `SequenceKind` itself and today
    happens to handle both cases (moving wins at game.rs:4523, Birth at :4497).
Expected behavior: one owner of the rule, as the crate's own doc states.
Root cause: the read-only patch referenced by the doc (`analysis/agent-anim/game_state_patch.md`)
    was never applied.
Impact: no current misbehaviour (verified by reading both branches); duplication means the next
    edit to one copy silently diverges. P4 engineering debt, not a gameplay defect.
Why not covered by excluded backlog: not in AUD-001…032.
Fix specification: make `sync_visuals` build `UnitActivity` and call `select_state`, mapping
    `GameplayState` onto the existing `SequenceKind`/carry variants; delete the local cascade.
Regression test(s): BLOCKED: operator test freeze. Manual: `grep -c select_state crates/wc3-game/src`.
Verification commands and exact result: grep above (0 production callers).
Related findings: none
```

### WC3-SIM-003 — `Sim::buffs` doc claims a sort that does not exist

```
ID: WC3-SIM-003
Title: “Live buffs, sorted by (unit, id)” is false — add_buff pushes in arrival order
Priority: P4
Status: CONFIRMED
Confidence: HIGH
File(s): crates/wc3-sim/src/lib.rs, crates/wc3-sim/src/buffs.rs
Exact line range(s): lib.rs:969-970 (field + doc), :7213-7258 (state_hash feeds `self.buffs` in
                     vector order); buffs.rs:289-330 (add_buff push), grep `sort` → buffs.rs:614
                     sorts *targets*, not `Sim::buffs`
Affected targets/features: determinism review of the hash (buff order), save/load ordering
Trigger: grep `sort` over lib.rs/buffs.rs (no sort of `self.buffs`).
Observed behavior: the vector is in insertion order; the hash and the save both depend on it.
Expected behavior: either the doc is true (sorted) or it states insertion order.
Root cause: the ordering patch implied by the comment is absent.
Impact: no divergence today (command order is deterministic and save/load preserves vector order);
    the doc would mislead a future reviewer into assuming order-independence. P4.
Why not covered by excluded backlog: not in AUD-001…032.
Fix specification: sort in `add_buff` by `(unit, id)` or correct the comment to “insertion order
    (deterministic; save/load preserves it)”.
Regression test(s): BLOCKED: operator test freeze. Manual: add two buffs out of id order and print
    `sim.buffs` ids.
Verification commands and exact result: grep output above.
Related findings: WC3-SIM-001
```

### WC3-SIM-004 — `ugol` exists twice with different race/model spelling

```
ID: WC3-SIM-004
Title: ROSTER and EXTRA_DEFS both define `ugol`; def_of/def_index silently return the ROSTER row
Priority: P4
Status: CONFIRMED
Confidence: HIGH
File(s): crates/wc3-sim/src/roster_gen.rs, crates/wc3-sim/src/extras_gen.rs, crates/wc3-sim/src/data.rs
Exact line range(s): roster_gen.rs:4611-4627 (Race::Undead, model `Buildings\Undead\…`),
                     extras_gen.rs:597-613 (Race::Neutral, model `buildings\undead\…`),
                     data.rs:1021-1033 (def_of = first match across ROSTER → CREEP → SHOP → EXTRA → …)
Affected targets/features: any spawn/lookup by id `ugol`; data hygiene of the generated tables
Trigger: script in §5 comparing ids across tables → `EXTRA_DEFS collides_with_roster=1 ['ugol']`.
Observed behavior: two rows, different `race` and model-path casing; `def_of("ugol")` always returns
    the ROSTER row, so the EXTRA row is dead data.
Expected behavior: one authoritative row per rawcode (or a deliberate override mechanism).
Root cause: the generator emitted the object from two SLK sources without a dedupe step.
Impact: no demonstrated behaviour change (the ROSTER row is the Undead Haunted Gold Mine, which is
    the correct object for `ugol`); the dead row can confuse a future stat/balance change. P4.
Why not covered by excluded backlog: not in AUD-001…032.
Fix specification: dedupe by id in the generator (prefer the ROSTER/Units source), or drop the
    EXTRA row and note the provenance.
Regression test(s): BLOCKED: operator test freeze. Manual: the cross-table id script in §5.
Verification commands and exact result: `EXTRA_DEFS rows 102 collides_with_roster 1 ['ugol']`;
    ROSTER itself is clean (`ROSTER: rows=122 unique=122 dup=[]`), CREEP/SHOP clean.
Related findings: none
```

### WC3-AUD-001 — SDL3 stream opener binds the callback parameter as `*mut c_void`

```
ID: WC3-AUD-001
Title: SDL_OpenAudioDeviceStream is called through a signature whose callback parameter is a data
       pointer instead of a function pointer
Priority: P4
Status: CONFIRMED (static; ABI-compatible on all supported targets, no runtime defect observed)
Confidence: HIGH on the code fact, MEDIUM on impact (none expected on aarch64/x86-64)
File(s): crates/wc3-audio/src/engine/sdl3.rs
Exact line range(s): :21-27 (`SdlAudioSpec` + `SdlOpenAudioDeviceStream`), :169-182 (call with
                     `null_mut()` callback), :94-106 (`transmute` of `dlsym` results)
Affected targets/features: audio device open (engine/sdl3.rs backend)
Preconditions: none
Trigger: read the type alias and the call.
Observed behavior: `type SdlOpenAudioDeviceStream = unsafe extern "C" fn(u32, *const SdlAudioSpec,
    *mut c_void, *mut c_void) -> *mut c_void;` — SDL3 declares `SDL_AudioStreamCallback` (a function
    pointer) as the third argument; the call passes `null_mut()`, so the value is identical on every
    supported ABI, but the *type* is not the C prototype.
Expected behavior: prototype-faithful binding (`Option<unsafe extern "C" fn(...)>`).
Root cause: hand-rolled dlopen bindings typed for convenience.
Impact: none observed; it is a latent UB/permission-to-optimise issue only. The rest of the binding
    (SDL_AUDIO_F32 0x8120, SDL_INIT_AUDIO 0x10, default playback 0xFFFF_FFFF, `SDL_AudioSpec`
    12-byte layout) is correct, and the push model (`SDL_PutAudioStreamData`) means no Rust code
    runs on an SDL callback thread → no panic-across-FFI hazard.
Why not covered by excluded backlog: not in AUD-001…032.
Fix specification: declare the callback slot as `Option<unsafe extern "C" fn(*mut c_void, *mut c_void,
    c_int) -> c_int>` (or `*mut c_void` only if wrapped in a plain `extern "C"` shim that never
    unwinds) and pass `None`.
Regression test(s): BLOCKED: operator test freeze. Manual: `cargo test -p wc3-audio --test shutdown`
    plus a device audio smoke (`sdl_smoke`) on the target.
Verification commands and exact result: static read of sdl3.rs:21-182; device runtime BLOCKED
    (see §4).
Related findings: none
```

### WC3-RPL-003 — `write_ids` truncates the unit count to `u16`

```
ID: WC3-RPL-003
Title: encode writes `units.len() as u16` but appends every id, so >65535 units corrupt the record
Priority: P4
Status: CONFIRMED
Confidence: HIGH
File(s): crates/wc3-replay/src/lib.rs
Exact line range(s): :368-372 (`write_ids`), :436-446 (`Reader::ids`)
Affected targets/features: replay encoding of a command with an oversized unit list
Preconditions: a `Command` carrying more than 65 535 unit ids (unreachable through the current
    selection cap; reachable through a script/API caller)
Trigger: read `write_ids`.
Observed behavior: the length prefix wraps; decode reads the first 65535 ids and misparses the rest.
Expected behavior: reject (or chunk) an oversized list instead of writing an inconsistent record.
Root cause: `as u16` with no bound check.
Impact: no reachable path today (selection is bounded); a silent corruption if a caller ever passes
    a large list. P4.
Why not covered by excluded backlog: not in AUD-001…032.
Fix specification: return an error (or clamp with an explicit log) when `units.len() > u16::MAX`.
Regression test(s): BLOCKED: operator test freeze. Manual: encode a 70 000-id `Command::Stop` and
    decode it.
Verification commands and exact result: static read; probe roundtrip covers 2-id lists (`ok=17`).
Related findings: WC3-RPL-001, WC3-RPL-002
```

### 2.1 Checked and NOT a defect (negative results — do not re-file)

| hypothesis | result |
|---|---|
| duplicate `Move`/`MoveAttack` | identical final order/path (probe `A`) |
| duplicate `Attack` | cooldown not reset (19→19), same target (probe `D`) |
| duplicate `Harvest` loses gathered gold | `lost_gold=0` over 300 ticks (probe `B`) |
| duplicate `Train` | 2 units for 2 clicks — WC3 semantics (probe `C`) |
| duplicate `ReviveHero`/`LearnAbility`/`Research` | rejected by existing guards (`lib.rs:5289-5293`, `:4592-4596`, `:4611-4613`) |
| `LRUCache` pin/evict/unlink correctness | correct; `pinned_skipped >= len` break prevents spin; entry cap enforced (`lru.rs:173-215`) |
| assetdb negative cache unbounded | capped at 8192 with clear-all (`lib.rs:505-509`) |
| ambient PCM unbounded | 64 MB byte budget, one oversized theme allowed by design (`game_audio.rs:38-106`) |
| audio voice pool leak | fixed 64 slots, `free` list, steal/preempt, natural end frees (`engine.rs:634-778`) |
| animation state leak | crate is stateless; game prunes `visuals`/`death_visuals` (`game.rs:4568-4570`) |
| save v9 per-seat completeness | 8 seats + `SEC_SEATS` validation + trailing-byte check (probe) |
| `state_hash` seat banks | 8 seats hashed (`lib.rs:7081-7086`; probe `seat7_bank … state_hash_same=false`) |
| generated balance-overlay duplicate ids | keyed by `(id, field)` — legitimate (`roster_gen.rs:12819-12915`) |
| `ray_aabb_toi` / `sweep_ball_ball` degenerate cases | zero-direction and inside-start handled (`geometry.rs:191-276`) |
| `DirSource` path traversal via `..\..` names | lookup is an index key, never `root.join(name)` (`source.rs:383-390`) |
| `Sim::advance_revival` `altar.unwrap()` reachable panic | guarded by `altar_ok` (`lib.rs:4930-4945`) |

## 3. H2 / ledger evidence

- corpus pack: `ev-b97e64244f942532` (gates read: persist-cadence, no-open-tails, corpus-cadence,
  deep-reflection, no-unworked-blocker, turn-integrity; experience 4718 seat binding applied).
- monitor: ledger **532** registered → checkpoints → closed (see closure block).
- captures: **1064** (wc3-replay build break), **1069** (lane summary, written before the
  WC3-INP-001 downgrade — the report is authoritative), **1070** (stray: an unsupported
  `action:"list"` call wrote an empty capture; recorded so it is not mistaken for evidence).
- experiences: **4747** (hash coverage method), **4748** (duplicate-command audit method).
- probes: four sources under `probes/lane-I/`, transient `crates/*/examples/` copies moved to
  `probes/lane-I/build-copies/` (the crates themselves are clean: `git status` shows no
  `examples/audit_i_*`).
- `FILE_COVERAGE.csv`: lane-I rows appended, then **merged** — while lane I wrote, the shared file was
  rewritten by another lane down to 305 rows (J/K rows lost). Restored the union from
  `FILE_COVERAGE.csv` + `lane-J-coverage.csv` + `lane-K-coverage.csv` + `FILE_COVERAGE.csv.bak-laneJ`
  → **682 rows** (lane-K 228, I 203, lane-J 149, H 102), written tmp+rename, backup
  `FILE_COVERAGE.csv.bak-laneI-merge-*`. Coordination note for the tech-lead: this file needs one
  serialized writer.
- final tally: **1×P1, 1×P2, 3×P3, 6×P4** (11 findings).

## 4. Coverage, depth and gaps

Full per-file table: `analysis/full-audit-2026-11-18/FILE_COVERAGE.csv` (rows `reviewer=I/529`).
203 in-scope files, 18 `BLOCKED`, 185 `AUDITED`; the `notes` column carries a prefix that states the
depth, so the merge does not have to guess:

| status | files | note prefix | meaning here |
|---|---|---|---|
| `AUDITED` | 52 | `read:` / `read+fixed:` | file read, at least one concrete observation, finding or negative result |
| `AUDITED` | 12 | `gen:` | generated data table, row count + id uniqueness checked, no logic |
| `AUDITED` | 100 | `test:` | existing test: `#[test]`/assert counts checked against the contract it names; **not run** (freeze), no new test added |
| `AUDITED` | 10 | `example/QA driver:` | example/QA binaries read; not part of the shipped runtime |
| `AUDITED` | 11 | `structural:` | whole-file scan (lines, `unsafe`, `unwrap/expect`, `todo`, module doc); **no line-by-line claim** |
| `BLOCKED` | 18 | tool binary / device | runtime verification needs the retail install or an audio device; static review recorded in the row |

Honest depth statement for the merge: **52 files are read-depth**. The 11 `structural:` files and the
100 test files are coverage, not deep audit; a COMPLETE claim for this lane would be false for them.

Runtime verification that this lane could **not** perform (recorded, not hidden):

1. **Audio device paths** (`engine/sdl3.rs`, `engine.rs` real backend, `bin/sdl_smoke.rs`): needs an
   audio device + SDL3 on the target; static review only. The shutdown budget constants
   (`engine.rs:69-79`) are unverified at runtime here.
2. **Install-dependent tools** (`wc3-assetdb/src/bin/*`, `catalog.rs`, `blp_report.rs`,
   `wc3-audio/src/bin/audio_*`): need the operator's MPQ install; these are QA tools, not the
   shipped runtime. Code read; no runtime output claimed.
3. **Full workspace build/test**: `cargo test --workspace` was not run (182 files are being modified
   concurrently by product lanes; lane K owns the build gate). Lane I ran
   `cargo test -p wc3-sim --test determinism --test save_load` → **17 passed, RC=0**.
4. **Device/touch**: no device run (tech-lead owns device runs). The mirror evidence is the app's own
   measured note (`apps/android-native/src/lib.rs:1869-1873`) plus the in-crate probe.
5. **External claim not made**: I did not verify SDL's touch→mouse synthesis default on Windows from a
   primary source in this session; WC3-INP-001 therefore rests on the in-repo measurement and the
   crate-level probe, and says “any target that delivers both event families” (not “Windows does”).

## 5. Exact probe output (verbatim)

`scripts/h2-cargo run -q -p wc3-sim --example audit_i_hash` → RC=0

```
PROBE save_format_version=9 max_players=8
PROBE base state_hash=0x86cd04795ecf610d save_digest=0xe78b0ca669f55f34 projectiles=1
PROBE carrying_lumber+5            state_hash_same=true  save_digest_same=false
PROBE carrying_gold+5(control)     state_hash_same=false save_digest_same=false
PROBE harvest_timer+10             state_hash_same=true  save_digest_same=false
PROBE stunned_until+30             state_hash_same=true  save_digest_same=false
PROBE channel_until+30             state_hash_same=true  save_digest_same=false
PROBE hero_xp+50                   state_hash_same=true  save_digest_same=false
PROBE learned+push                 state_hash_same=true  save_digest_same=false
PROBE research_push                state_hash_same=true  save_digest_same=false
PROBE items+push                   state_hash_same=true  save_digest_same=false
PROBE goal+move                    state_hash_same=true  save_digest_same=false
PROBE order_queue_retarget         state_hash_same=true  save_digest_same=false
PROBE upgrades_seat7+Rhme          state_hash_same=true  save_digest_same=false
PROBE destructable_hp-10           state_hash_same=true  save_digest_same=false
PROBE item_charges-1               state_hash_same=true  save_digest_same=false
PROBE projectile_damage+1          state_hash_same=true  save_digest_same=false
PROBE projectile_target_clear      state_hash_same=true  save_digest_same=false
PROBE buff_add                     state_hash_same=false save_digest_same=false
PROBE kill_enemy(control)          state_hash_same=false save_digest_same=false
PROBE seat7_bank                  state_hash_same=false gold7=4321 lumber7=123
PROBE save_v9_upgrade_roundtrip stored=Some(3) restored=Some(3) reencode_identical=true
PROBE carried_lumber_divergence first_hash_diff_tick=None tick_end=328
```

`scripts/h2-cargo run -q -p wc3-sim --example audit_i_dup` → RC=0

```
A one_tick   order=Harvest { mine: Vec2(900.0, 600.0), phase: Gathering } timer=60 path=0 gold=500 single_vs_double_order_equal=true
A two_tick   order=Harvest { mine: Vec2(900.0, 600.0), phase: Gathering } timer=60 path=0 gold=500
B gather_tick=Some(0) control_timer_before=59 spammed_timer_after_dup=60 control_order=Harvest { mine: Vec2(900.0, 600.0), phase: Gathering } spammed_order=Harvest { mine: Vec2(900.0, 600.0), phase: Gathering }
B after_dup_1_tick gold_before=500 control_gold_300=500->540 spammed_gold_300=500->540 lost_gold=0
C train x1 production=1 gold=840 | train x2 production=2 gold=680 (double_charge=160)
D cooldown_before=20 once_cd=19 twice_cd=19 once_target=Attack(UnitId(2)) twice_target=Attack(UnitId(2)) hp_before=404
E issue same_tick_suppressed=true next_tick_reissued=true order=Move(Vec2(704.0, 704.0))->Move(Vec2(704.0, 704.0))->Move(Vec2(704.0, 704.0))
G cancel_train queued_before=3 queued_after_dup=1 gold=520->840 refund=320 events=[Error("production cancelled"), Error("production cancelled"), Victory(0)]
H build_x1 buildings=2 gold=550 | build_x2 buildings=3 gold=100
F queue_order duplicated_len=2 entries=[Harvest(Vec2(900.0, 600.0)), Harvest(Vec2(900.0, 600.0))]
```

`scripts/h2-cargo run -q -p wc3-input --example audit_i_mirror` → RC=0

```
1 clean_tap pointer_down=1 pointer_up=1 events=["touch=pointer_down:100.0:100.0", "touch=pointer_up:100.0:100.0"]
2 mirrored_tap pointer_down=2 pointer_up=2 events=["touch=pointer_down:100.0:100.0", "touch=pointer_down:100.0:100.0", "touch=pointer_up:100.0:100.0", "touch=pointer_up:100.0:100.0"]
3 duplicate_same_id pointer_down=1 pointer_up=1 pinches=0 events=["touch=pointer_down:100.0:100.0", "touch=pointer_up:100.0:100.0"]
4 duplicate_new_id pointer_down=1 pinches=1 two_finger_taps=0 events=["touch=pointer_down:100.0:100.0", "touch=pinch_begin:0.0:100.0:100.0", "touch=pinch_end:two_finger_tap:100.0:100.0", "touch=pointer_resume_pan:100.0:100.0", "touch=pointer_up:100.0:100.0"]
5 stale_finger_up consumed=true events=[]
6 reset_mid_tap pointer_down=1 pointer_cancel=1 pointer_up=0 events=["touch=pointer_down:50.0:50.0", "touch=pointer_cancel"]
```

(note on row 4: the counters are `pinches=1 two_finger_taps=0`; the two-finger tap appears in the
event list as `pinch_end:two_finger_tap` — `RecordingSink` has no separate `two_finger_tap` prefix,
so the count filter is 0 by construction. The event string is the evidence.)

`./target/debug/examples/audit_i_replay {roundtrip,alloc,record_len,ticks}`

```
PROBE roundtrip variants=17 ok=17                                        RC_ROUNDTRIP=0
PROBE alloc input_bytes=44 announced_records=4294967295 vm_before="VmSize:\t 2167056 kB"
PROBE alloc vm_after="VmSize:\t237048080 kB" outcome=Err(Io(Error { kind: UnexpectedEof, message: "failed to fill whole buffer" }))
PROBE record_len input_bytes=57 rss_before="VmRSS:\t    4784 kB"
PROBE record_len rss_after="VmRSS:\t    5428 kB" outcome=Err(Io(Error { kind: UnexpectedEof, … }))
PROBE ticks announced=18446744073709551615 records=0                      RC_TICKS=124 (timeout 6s)
```

`scripts/h2-cargo check -p wc3-replay` → before fix `error[E0004]`, `EXACT_RC=101`; after fix
`Finished dev profile`, `RC=0`.

`scripts/h2-cargo test -q -p wc3-sim --test determinism --test save_load` → `RC=0`,
`test result: ok. 3 passed`, `test result: ok. 14 passed`.

`scripts/h2-cargo test -p wc3-replay` (after the fix) → `RC=0`, 5 passed:
`detects_desync`, `lockstep_delivers_after_delay`, `command_encoding_roundtrip`,
`unknown_ability_id_is_a_typed_error`, `replay_roundtrip_and_verify`.

Generated-table id integrity (python, §2.1):

```
ROSTER: rows=122 unique=122 dup=[]
CREEP_DEFS rows 327 collides_with_roster 0 []
SHOP_DEFS rows 5 collides_with_roster 0 []
EXTRA_DEFS rows 102 collides_with_roster 1 ['ugol']
```

## 6. Red-team notes for lane J (and questions back)

1. **My P2 (WC3-SIM-001) is a detection-quality finding, not a gameplay bug.** Do not inflate it to
   P1: no desync is *caused* here; the hash fails to *report* one. If lane J's render work relies on
   `state_hash` for anything visual, say so and I will re-scope.
2. **My P4 (WC3-INP-001) now has three downstream guards** — `SDL_TOUCH_MOUSE_EVENTS=0`
   (`apps/android-native/src/lib.rs:2272`), the Android cfg filter (`:1869-1876`) and
   `is_mirrored_tap` 60 ms/6 px (`crates/wc3-game/src/game.rs:5919-5927`). Challenge: is any consumer
   of `InputState` outside `Game` (SDL smoke binaries, desktop `apps/smoke`) feeding both event
   families? I did not find one, but lane K owns `apps/**`. Also `pinch_end → right_click`
   (`game.rs:5963-5968`) is the one order path with no mirror guard.
3. **WC3-SIM-002's `Build` row** was measured with `Command::Build` (the “legacy instant placement”
   path, `lib.rs:1842-1847`). The HUD path (`game.rs:6415-6470`) calls `place_denial` first, so a
   normal double-tap is denied by occupancy **only if** the first building blocks its cells
   immediately — my probe shows it does not (`build_x2 buildings=3`). Please check from the HUD side
   whether a second tap in the same frame batch can reach `place_from_card` with `placing` still
   `Some`; if yes, escalate to P2 and hand the fix to the HUD owner.
4. **`sync_visuals` vs `wc3_anim::state`** (WC3-ANM-001): I read both branches and they agree today.
   If lane J is about to touch the animation cascade, apply the crate rule instead of editing the
   copy — otherwise the audit finding becomes a live regression.
5. **What I could not disprove**: no evidence that the 8-seat save drops seat >1 state (it does not);
   no evidence of cache growth in assetdb/audio beyond the soft budgets. If lane J finds a memory
   graph that contradicts this, the counters in WC3-ADB-001 are the reason it would be invisible.

## 7. Files changed by this lane

| file | change | why |
|---|---|---|
| `crates/wc3-replay/src/lib.rs` | +3 encode arms (tags 14/15/16), +3 decode arms, comment | WC3-RPL-001: unblocks the workspace build; round-trip 17/17 |
| `analysis/full-audit-2026-11-18/lane-I-sim-input-media.md` | new | this report |
| `analysis/full-audit-2026-11-18/FILE_COVERAGE.csv` | appended lane-I rows | per-file coverage |
| `analysis/full-audit-2026-11-18/probes/lane-I/*` | new | probes + build copies |

No file under `tests/` was added or modified. The replay fix is a compile-level completion of
`b9ccb59`, not a behaviour change: the three commands were previously *unrepresentable*.
