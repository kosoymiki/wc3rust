# LANE 539 — sim integrity fix (WC3-SIM-001/002, WC3-RPL-002, WC3-ADB-001, WC3-I-005)

Ledger: **539** (fix lane, dispatched by the tech lead).
Repository: `~/src/wc3rust`, HEAD `d7af459` + the shared uncommitted working tree
(other lanes landing in parallel; this lane touched only its owner paths).
Rules honored: **no new tests** (no `#[test]`, nothing under `tests/`), **no device
commands**, `scripts/h2-cargo` only. Evidence is `check` rc + one-shot probes under
`analysis/full-audit-2026-11-18/probes/lane-I/` (sources kept; transient
`crates/*/examples/` copies moved to `probes/lane-I/build-copies/`).

## Files changed (owner paths only)

```
crates/wc3-sim/src/lib.rs       state_hash -> save-payload digest; per-tick duplicate guard in step()
crates/wc3-sim/src/save.rs      encode_payload_layout + simulation_digest (payload minus the view-only seat frame)
crates/wc3-replay/src/lib.rs    bounded read/parse, strict command framing, MAX_VERIFY_TICKS + verify_with_limit + ReplayError::Duration
crates/wc3-assetdb/src/lib.rs   seven AssetStats counters wired to the caches
```

Not touched: `crates/wc3-re/**` (537), `crates/wc3-formats/**` (538),
`crates/wc3-game/**` (540), `crates/wc3-sim/tests/**` (test freeze).

## WC3-SIM-001 (P2) — `state_hash` blind to live state

Root cause (from the lane-I audit): the hash was a hand-written field list
(`lib.rs:7074-7281` at HEAD). It fed the `order_queue` variant tag but not the
payload, and never fed `carrying_lumber`, `harvest_timer`, `stunned_until`,
`channel_until`, `xp`/`learned`, `research`, `items`, `goal`, `upgrades`,
destructables, ground items or projectiles.

Fix: `Sim::state_hash` is now FNV-1a over the canonical save payload without the
`SEC_SEATS` frame (`save.rs:5720` `simulation_digest`; shared encoder at
`save.rs:4313` `encode_payload_layout`). Every saved field is therefore covered by
construction, and a save/load round trip keeps the hash because the restored
simulation re-encodes the same bytes. The seat frame is omitted on purpose: the
seat count is layout metadata and `human_seat` is a view parameter (two lockstep
peers may bind different seats), matching the previous documented behaviour.
`save_digest` still uses the full v9 payload; both survive a round trip.

Probe `audit_i_hash` (before → after, `probes/lane-I/logs/audit_i_hash.after.log`):

```
before: carrying_lumber+5 true, harvest_timer+10 true, stunned_until+30 true,
        channel_until+30 true, hero_xp+50 true, learned+push true, research_push true,
        items+push true, goal+move true, order_queue_retarget true, upgrades_seat7 true,
        destructable_hp-10 true, item_charges-1 true, projectile_damage+1 true,
        projectile_target_clear true                      -> 15 blind mutations
        carried_lumber_divergence first_hash_diff_tick=None tick_end=328
after:  every one of them state_hash_same=false; save_digest_same=false
        grep -c "state_hash_same=true" log = 0
        PROBE carried_lumber_divergence first_hash_diff_tick=Some(0) tick_end=328
        PROBE save_v9_upgrade_roundtrip stored=Some(3) restored=Some(3) reencode_identical=true
```

Round-trip equality is additionally pinned by the existing `save_load` suite
(8/8 pass, including `restored.state_hash() == hash_before`).

## WC3-SIM-002 (P3) — duplicate `CancelTrain`/`Build` double-apply

Root cause: `Sim::apply` had no per-tick duplicate guard (the guard existed only in
the `issue()` API), `CancelTrain` removes by index (the second copy removes the next
entry), and `build_instant_impl` has no placement validation (the second copy places
a second building).

Fix (`lib.rs:1650`): `step` records the commands it applied in this call and drops an
exact repeat before it reaches the systems. `Train` and `Cast` are exempt because
they are repeatable by design (two clicks queue two units / cast twice); every other
variant either sets an order (already idempotent) or is one-shot (`Build`,
`CancelTrain`). A duplicate in a later tick is still a real second action.

Probe `audit_i_dup` (before → after, `probes/lane-I/logs/audit_i_dup.after.log`):

```
before: G cancel_train queued_before=3 queued_after_dup=1 gold=520->840 refund=320
        H build_x1 buildings=2 gold=550 | build_x2 buildings=3 gold=100
after:  G cancel_train queued_before=3 queued_after_dup=2 gold=520->680 refund=160
        H build_x1 buildings=2 gold=550 | build_x2 buildings=2 gold=550
        C train x2 production=2 (unchanged: two clicks are two units)
```

## WC3-RPL-002 (P3) — replay trusts announced count/length/ticks

Root cause: `Replay::read` used the file's `count` for `Vec::with_capacity`, its
record `len` for `vec![0u8; len]`, and `verify` used the file's `ticks` as a loop
bound, with no relation to the bytes present (44-byte file → 226 GiB VA; `ticks =
u64::MAX` → non-terminating verify).

Fix (`wc3-replay/src/lib.rs`):

- `read` pulls the stream through a `take(MAX_REPLAY_BYTES + 1)` cap and parses a
  slice (`:159` `read`, `:171` `parse`). `MAX_REPLAY_BYTES` = 256 MiB.
- `count` is bounded by `MAX_RECORDS` **and** `remaining / MIN_RECORD_BYTES` (14);
  each record `len` is bounded by `MAX_COMMAND_BYTES` **and** the remaining bytes.
- record ticks must be `< ticks` and non-decreasing; trailing bytes are rejected.
- `decode_command` rejects trailing bytes inside an announced command.
- `verify_with_limit(width, height, max_ticks, setup)` refuses with
  `ReplayError::Duration { ticks, limit }` before `Sim::new`/`setup`; `verify_with`
  uses the external `MAX_VERIFY_TICKS` (24 h at `TICKS_PER_SECOND`), and a caller can
  only tighten the bound, never widen it.

Probe `audit_i_replay_bounds` (`probes/lane-I/logs/audit_i_replay_bounds.after.log`):

```
PROBE record_count   rejected=corrupt replay: record count
PROBE command_len    rejected=corrupt replay: command length
PROBE tick_count     rejected=corrupt replay: tick count
PROBE record_past_end rejected=corrupt replay: record tick outside the replay
PROBE trailing_cmd   rejected=corrupt replay: trailing command bytes
REPLAY_VERIFY_REFUSED reason=duration end_tick=2592001 limit=2592000 setup_ran=false
REPLAY_VERIFY_REFUSED reason=duration requested_limit=u64::MAX effective_limit=2592000 end_tick=2592001 setup_ran=false
REPLAY_VERIFY_OK ticks=120 records=2 hash_matches=true
```

`audit_i_replay -- roundtrip` still prints `PROBE roundtrip variants=17 ok=17`
(RC=0), so WC3-RPL-001's fix is unaffected.

## WC3-I-005 (interim TZ) — visible end_tick/duration bound

Covered by the same fix: the external duration bound is the public
`MAX_VERIFY_TICKS` constant (`wc3-replay/src/lib.rs:30`), refusal is a typed
`ReplayError::Duration` raised *before* the setup closure and the simulation exist,
and the probe prints the required `REPLAY_VERIFY_REFUSED reason=duration` (with
`setup_ran=false`) while a normal replay prints `REPLAY_VERIFY_OK`. A caller that
asks for a wider bound (`u64::MAX`) still gets the external limit
(`effective_limit=2592000`).

## WC3-ADB-001 (P3) — `AssetStats` seven counters hardcoded 0

Root cause: the struct carried `negative_hits`, `texture_*`, `model_*`, `sound_*`
but only the aggregate `hits`/`misses` were incremented; `stats()` wrote `0` for
the other seven.

Fix (`wc3-assetdb/src/lib.rs`): per-cache counters on `AssetDb` (`:231`), incremented
in `read` (negative-cache hit), `texture_shared`, `model_shared`, `sound_shared`
(hit/miss at the cache probe), and returned by `stats()` (`:739`). The aggregate
`hits`/`misses` are unchanged.

Probe `audit_i_assetstats` against the retail archive, read-only
(`probes/lane-I/logs/audit_i_assetstats.after.log`):

```
PROBE assetstats load=texture/model/sound/raw/missing pass=0/1 ok=true (10 loads)
PROBE assetstats negative_hits=0->1 texture=1/1 model=1/1 sound=1/1 raw_hits_aggregate=6 misses=7
```

## Verification

```
scripts/h2-cargo check -p wc3-sim -p wc3-replay -p wc3-assetdb   -> Finished dev profile, RC=0
scripts/h2-cargo test -p wc3-sim --test save_load                 -> 8 passed, 0 failed
scripts/h2-cargo test -p wc3-sim --test determinism --test ai \
    --test production --test game_compat                          -> 8 + 3 + 1 + 10 passed, 0 failed
scripts/h2-cargo test -p wc3-replay                               -> 5 passed, 0 failed
scripts/h2-cargo test -p wc3-assetdb                              -> 21 passed, 0 failed
```

Workspace-wide `scripts/h2-cargo check --workspace` is recorded in
`probes/lane-I/logs/check-workspace.log` / `.rc`: **rc=101**, and all four errors are in
`crates/wc3-game/src/hudskin/manifest.rs` (`E0063` initializer fields `license`/`name`/
`notes`/`gutter`), i.e. the HUD-manifest lane's uncommitted tree. Zero errors in
`wc3-sim`/`wc3-replay`/`wc3-assetdb`; the scoped check above is rc=0.

### Cross-lane observations (not this lane's files)

- `wc3-sim --test multiplayer`: 13 pass, 3 fail in
  `net_match_1v1_{map_starts_deterministic_victory,save_load_resume_identical_hash,tcp_deterministic_victory}`
  because `melee_map()` asserts
  `donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/(2)Ponds_v1.4.w3x`
  exists and that donor file is absent from this checkout (`ls` → No such file).
  Environmental fixture gap, not a hash/command regression.
- `wc3-sim --test merge neutral_map_units_cluster_into_creep_camps`: 28/29 pass,
  this one fails with `created = 0`. Evidence: the test spawns `owner = 2`, the
  working tree adds `pub const NEUTRAL_OWNER: u8 = 12;` (not present at HEAD) and
  `cluster_neutral_camps()` filters on it; both `crates/wc3-sim/src/lib.rs` and
  `crates/wc3-sim/tests/merge.rs` are uncommitted edits owned by the neutral-owner
  lane. Diagnostic probe `debug_camp_cluster` shows all five units spawn and
  `is_camp_guard() = true`, so the only mismatch is the owner id
  (`2 != NEUTRAL_OWNER = 12`). Fix owner: the lane that changed `NEUTRAL_OWNER`;
  next step is to spawn `NEUTRAL_OWNER` in the test (test file is outside this
  lane's ownership and under the test freeze).

## Reproducers

```
cp analysis/full-audit-2026-11-18/probes/lane-I/audit_i_hash.rs crates/wc3-sim/examples/
scripts/h2-cargo run -q -p wc3-sim --example audit_i_hash

cp analysis/full-audit-2026-11-18/probes/lane-I/audit_i_dup.rs crates/wc3-sim/examples/
scripts/h2-cargo run -q -p wc3-sim --example audit_i_dup

cp analysis/full-audit-2026-11-18/probes/lane-I/audit_i_replay_bounds.rs crates/wc3-replay/examples/
scripts/h2-cargo run -q -p wc3-replay --example audit_i_replay_bounds

cp analysis/full-audit-2026-11-18/probes/lane-I/audit_i_assetstats.rs crates/wc3-assetdb/examples/
scripts/h2-cargo run -q -p wc3-assetdb --example audit_i_assetstats
```
