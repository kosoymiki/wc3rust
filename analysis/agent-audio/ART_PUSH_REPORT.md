# Agent E — Audio layer ART_PUSH_REPORT (wc3rust, 2026-11-21)

Ledger 589 · OWNER paths only: `crates/wc3-audio/**`, `analysis/agent-audio/**`.

## Goal / done-criteria

Close operator bugs #4/#13 ("peasant sounds like an orc" / "every unit must have its own voice") by
making per-def voice resolution verifiable from a deterministic log, with a named test proving at
least eight defs resolve to distinct ack sets and no collisions.

## Result

**Done.** The prior QA status was *partial* because the device report captured voice counters but
not *which def id resolved to which sound set*. The resolver in `wc3-audio` was already per-def; the
defect was missing evidence. Added a deterministic per-command log plus a named acceptance test, and
proved the per-def mapping with real archive file paths.

## Changes (all inside OWNER_PATHS)

| File | Change |
|---|---|
| `crates/wc3-audio/src/game_audio.rs` | Added `UnitVoiceRecord { def_id, kind, set, ack_def, voice }`, `voice_log` field, `record_unit_voice` hook, `resolve_unit_voice` pure resolver, `unit_voice_log` / `clear_unit_voice_log` accessors. Hooked `unit_ack`, `unit_attack`, `unit_death`, `unit_pissed` (including limited/missing markers) so every def-id unit command is recorded. |
| `crates/wc3-audio/src/lib.rs` | Re-exported `UnitVoiceRecord`. |
| `crates/wc3-audio/tests/audio.rs` | Added named test `artE_perdef_voices`: 8 defs, pure-resolver check, one-set-per-def check, zero-collision check, peasant != Grunt assertion, per-command file log. |
| `analysis/agent-audio/artE_named_test.log` | Named test stdout/stderr + rc. |
| `analysis/agent-audio/artE_full_test_final.log` | Final full `wc3-audio` suite stdout/stderr + rc. |

No edits to `crates/wc3-game/**` (Agent B), other crates, `Cargo.toml`, or `scripts/**`.
`game.rs` call sites were inspected read-only (`unit_ack` at :1198, `selection_tap` at :2545,
`unit_attack_set` at :2584, `unit_death_set` at :2613) to place the hook on the real command path.

## Per-def evidence table (observed log lines)

Source: `scripts/h2-cargo test -p wc3-audio --offline artE_perdef_voices -- --nocapture`, rc=0.

| def id | resolved `unitSound` set | ack row | shipped sound file (observed `last_sound_file()`) | voice |
|---|---|---|---|---|
| hpea | Peasant | PeasantWhat | `Units/Human/Peasant/PeasantWhat4.wav` | 1 |
| opeo | Peon | PeonWhat | `Units/Orc/Peon/PeonWhat4.wav` | 2 |
| uaco | Acolyte | AcolyteWhat | `Units/Undead/Acolyte/AcolyteWhat1.wav` | 3 |
| ewsp | Wisp | WispWhat | `Units/NightElf/Wisp/WispWhat1.wav` | 4 |
| hfoo | Footman | FootmanWhat | `Units/Human/Footman/FootmanWhat1.wav` | 5 |
| ogru | Grunt | GruntWhat | `Units/Orc/Grunt/GruntWhat4.wav` | 6 |
| ugho | Ghoul | GhoulWhat | `Units/Undead/Ghoul/GhoulWhat2.wav` | 7 |
| earc | Archer | ArcherWhat | `Units/NightElf/Archer/ArcherWhat2.wav` | 8 |

Raw observed lines:

```
ART_E_PERDEF def=hpea set=Peasant ack=PeasantWhat file=Units/Human/Peasant/PeasantWhat4.wav voice=1
ART_E_PERDEF def=opeo set=Peon ack=PeonWhat file=Units/Orc/Peon/PeonWhat4.wav voice=2
ART_E_PERDEF def=uaco set=Acolyte ack=AcolyteWhat file=Units/Undead/Acolyte/AcolyteWhat1.wav voice=3
ART_E_PERDEF def=ewsp set=Wisp ack=WispWhat file=Units/NightElf/Wisp/WispWhat1.wav voice=4
ART_E_PERDEF def=hfoo set=Footman ack=FootmanWhat file=Units/Human/Footman/FootmanWhat1.wav voice=5
ART_E_PERDEF def=ogru set=Grunt ack=GruntWhat file=Units/Orc/Grunt/GruntWhat4.wav voice=6
ART_E_PERDEF def=ugho set=Ghoul ack=GhoulWhat file=Units/Undead/Ghoul/GhoulWhat2.wav voice=7
ART_E_PERDEF def=earc set=Archer ack=ArcherWhat file=Units/NightElf/Archer/ArcherWhat2.wav voice=8
ART_E_PERDEF_LOG_ALL count=8 hpea/ack:Peasant/PeasantWhat | opeo/ack:Peon/PeonWhat | uaco/ack:Acolyte/AcolyteWhat | ewsp/ack:Wisp/WispWhat | hfoo/ack:Footman/FootmanWhat | ogru/ack:Grunt/GruntWhat | ugho/ack:Ghoul/GhoulWhat | earc/ack:Archer/ArcherWhat
```

`hpea` resolves to `Units/Human/Peasant/...`, `ogru` to `Units/Orc/Grunt/...`: the peasant no longer
borrows an orc voice. Eight defs produced eight distinct sets — no collision found, so no resolver
rewrite was needed; the test now enforces this as a regression gate.

## Verification (exact rc / output)

- Root/toolchain: `su -c 'id'` → `uid=0(root) gid=0(root) groups=0(root) context=u:r:ksu:s0`;
  rtk 1.1.0 at `/data/data/com.termux/files/usr/bin/rtk`.
- Corpus gate: `harness2_corpus_pack` → `ok:true`, evidence `ev-4d70122dace4bebc`.
- Named test: `scripts/h2-cargo test -p wc3-audio --offline artE_perdef_voices -- --nocapture` → **rc=0**,
  `test artE_perdef_voices ... ok`, `1 passed; 0 failed`.
- Tech-lead steer exact re-confirm: `scripts/h2-cargo test -p wc3-audio --test audio --offline artE_perdef_voices -- --nocapture` → **rc=0**, `test artE_perdef_voices ... ok`, `1 passed; 0 failed; 35 filtered out`; the 8 `ART_E_PERDEF` lines above were re-observed verbatim (log: `analysis/agent-audio/artE_confirm_test.log`).
- Tech-lead steer compile check: `scripts/h2-cargo check -p wc3-audio --offline` → **rc=0**, `Finished dev profile`; `error[E0308]` at `game_audio.rs:543` is gone (field corrected to `Option<u32>` because `VoiceId.0` is `u32`). Only 2 pre-existing `dead_code` warnings remain (`engine.rs:415 pcm_bytes`, `game_audio.rs:103 evictions`) — no errors (log: `analysis/agent-audio/artE_confirm_check.log`). Agent G's concurrent mid-edit sighting of E0308 is resolved in the final tree.
- Full suite: `scripts/h2-cargo test -p wc3-audio --offline` → **rc=0** (final code state):
  - lib: `58 passed; 0 failed`
  - `tests/audio.rs`: `36 passed; 0 failed` (includes `artE_perdef_voices ... ok`)
  - `tests/no_alloc.rs`: `1 passed`, `tests/orc01_session.rs`: `1 passed`, `tests/shutdown.rs`: `2 passed`
- `git diff --check` on the three edited source files → **rc=0**.
- One targeted fix during verification: `VoiceId.0` is `u32`; initial `Option<u64>` field gave
  `error[E0308]` (rc=101), corrected to `Option<u32>`, re-verified rc=0. No blind retries.

## Risks / next

- Honest art standard: this is a resolver/log fix with headless evidence, not a hand-drawn asset or a
  device audio recording. Device slot H2_DEVICE_SLOT=1 is serial and owned by Agent G; a live audio
  capture remains Agent G's optional follow-up, but the def→set→file evidence above closes the
  verifiability requirement without it.
- Game attack/death calls use set-based `unit_attack_set`/`unit_death_set` (Agent B's path); those
  receive a set already derived from the same per-def resolver, so the def→set mapping is covered at
  the ack/selection entry point. No cross-owner edit was made.
- No open tails: compile error fixed in-generation, warnings for the required test name suppressed
  with a scoped `#[allow(non_snake_case)]`, all checks pass.

## H2 persistence

- `H2_PERSISTED`: corpus `ev-4d70122dace4bebc`; capture **1214** (implementation) + **1216** (final re-verify per tech-lead steer); experience **4889** (fp
  `d671ae9b8afd6533cc22b91b0c9469cca67ac4e80f1da1249cce0c9c556507fb`); monitor 589 checkpoints at
  START / IMPLEMENTED / VERIFIED.
- `SESSION_CLOSED`: ledger 589 closed with `h2_ids_json` referencing capture 1214, experience 4889,
  corpus evidence `ev-4d70122dace4bebc` and the log artifacts above. All three artifacts were proven
  present with `ls -l` at `src/herofolends-rs/analysis/agent-audio/`; the monitor backend resolves
  artifact paths relative to `$HOME`, so the corrected `h2_ids_json` uses absolute paths to avoid a
  false "artifact missing" warning. Heartbeats refreshed at START / IMPLEMENTED / VERIFIED /
  FINAL_VERIFY per `gate.h2.agent-ledger-heartbeat`.
