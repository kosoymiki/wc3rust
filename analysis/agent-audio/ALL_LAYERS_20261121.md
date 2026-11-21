# ALL LAYERS 2026-11-21 — Agent E: audio / voices (ledger 601)

Packet: `analysis/tech-lead/all-layers-20261121/PACKET_E_audio.md`
Repo: `~/src/herofolends-rs` (`~/src/wc3rust`)
Owners (only writer): `crates/wc3-audio/**`, `crates/wc3-audio/tests/artE_*`
Date: 2026-11-21 (MSK). All claims below are `observed` unless labelled otherwise.
H2: capture **1252**, experience **4918**, monitor ledger **601** (opened/closed by this agent).

## Goal

Close per-def unit voices (operator bugs #4/#13: "peasant sounds like an orc",
per-def ack sets collapsing into a race default) with a deterministic
`def -> ack set -> row -> file` log and the named test `artE_perdef_voices`
(>= 8 defs, no collisions, peasant != orc); cross-check the resolved sound paths
against the shipped `UI/SoundInfo/*.slk` tables and fix any drift; prove zero
underruns on the reference path.

## Actions

1. **New independent-oracle test file** `crates/wc3-audio/tests/artE_perdef_voices.rs`
   (3 tests). It parses the shipped SLK cells with its own tokenizer — not
   `wc3_audio::parse_slk` — and uses that as the oracle:
   - `artE_perdef_voices` — 8 reference defs; asserts the raw `unitUI.slk`
     `unitSound` cell, the raw `UnitAckSounds.slk` row (`{set}What`), the
     resolved `SoundDef` row/paths, the actually started file, and the voice-log
     file; 8 defs -> 8 sets -> 8 rows -> 8 files; peasant path under
     `Units\Human\Peasant\`, never the Orc tree.
   - `artE_slk_resolution_crosscheck` — every shipped unit row (466), candidate
     order `unitSound` -> display name, 392 with an ack row; resolved set/row/
     paths must equal the raw cells; 74 units are silent *by data* (no ack row
     shipped at all: buildings, wards, critters).
   - `artE_soundinfo_paths_match_slk` — every loaded definition (2 529) across
     the 7 ingested SoundInfo tables must resolve to exactly the raw
     `DirectoryBase` + file cells.
   - `assert_same_object_data` — `analysis/war3_data` (packet oracle path) and
     `analysis/war3_full3` (played tree) must be byte-identical for
     `unitUI.slk`, `UnitAckSounds.slk`, `AnimSounds.slk`.
2. **`crates/wc3-audio/src/game_audio.rs`**
   - `UnitVoiceRecord` now carries `file: Option<String>` (`game_audio.rs:325`),
     so the deterministic log is exactly `def -> set -> row -> file`.
   - `record_unit_voice` (`game_audio.rs:533-556`) records the file only when the
     command actually started a voice (a missing/gated command records `None`,
     never a stale clip from an earlier command).
   - Existing entry points already log one record per command:
     `unit_ack` (`:1008`), `unit_attack` (`:1083`), `unit_death` (`:1118`),
     `unit_pissed` (`:1140`), with the pure resolver `resolve_unit_voice`
     (`:558`) and log accessors `unit_voice_log`/`clear_unit_voice_log`
     (`:566-573`).
3. **`crates/wc3-audio/tests/orc01_session.rs`** — the reference session now
   asserts `buffer_underruns == 0`, `backend_errors == 0`,
   `commands_dropped == 0` (`:543-558`), alongside the existing 64 MB PCM,
   64-voice and crossfade assertions.
4. Removed the older duplicate `artE_perdef_voices` from `tests/audio.rs`; the
   dedicated oracle file subsumes it (same assertions plus the independent
   parser, paths and log-file checks).
5. **Drift investigation.** The independent oracle first reported 214
   mismatches. Root cause was the *oracle*, not the crate: shipped SLK rows can
   be started by an `F;Y<n>` formatting record only, and a value record may omit
   `X` and write column 1 (`C;Y13;K"AmbientSoundTestWind"`); the editor
   annotation region also contains unbalanced quotes, so whole-file quote
   tracking is invalid. After fixing those three semantics the crate shows
   **0 drift**; the production parser (`soundinfo.rs:709-775`) was already
   handling `F;Y` rows correctly. No production resolution change was needed —
   the oracle now guards the behavior permanently.

## Evidence (exact commands + rc)

```
./scripts/h2-cargo check -p wc3-audio --offline
  CHECK_RC=0   (Finished dev profile; only the 2 pre-existing lib warnings)

./scripts/h2-cargo test -p wc3-audio --offline
  FULL_RC=0
  58 lib + 3 artE_perdef_voices + 35 audio + 1 no_alloc + 1 orc01_session
  (113.46 s) + 2 shutdown = 100 passed, 0 failed, 0 ignored

./scripts/h2-cargo test -p wc3-audio --offline --test artE_perdef_voices -- --nocapture
  rc=0   3 passed; 0 failed
```

Determinism: two consecutive runs produced byte-identical `ART_E_PERDEF` lines
(`diff` empty, `DETERMINISTIC_SAME`), including the chosen variant file.

### Per-def table (observed, `ART_E_PERDEF_LOG`)

| def | set | row | file (started) | voice |
|---|---|---|---|---|
| hpea | Peasant | PeasantWhat | Units/Human/Peasant/PeasantWhat4.wav | 1 |
| hfoo | Footman | FootmanWhat | Units/Human/Footman/FootmanWhat4.wav | 2 |
| opeo | Peon | PeonWhat | Units/Orc/Peon/PeonWhat2.wav | 3 |
| ogru | Grunt | GruntWhat | Units/Orc/Grunt/GruntWhat3.wav | 4 |
| uaco | Acolyte | AcolyteWhat | Units/Undead/Acolyte/AcolyteWhat3.wav | 5 |
| ugho | Ghoul | GhoulWhat | Units/Undead/Ghoul/GhoulWhat4.wav | 6 |
| ewsp | Wisp | WispWhat | Units/NightElf/Wisp/WispWhat2.wav | 7 |
| earc | Archer | ArcherWhat | Units/NightElf/Archer/ArcherWhat2.wav | 8 |

Raw oracle cells (independent parse of `analysis/war3_data`, byte-identical to
`analysis/war3_full3`): every def's `unitUI.slk` `unitSound` cell equals the set
above; each `{set}What` row in `UnitAckSounds.slk` owns its own directory
(`Units\Human\Peasant\`, `Units\Orc\Peon\`, ...) and file list; resolved
`SoundDef::paths()` equals those raw cells exactly.

### SLK cross-check (observed)

```
ART_E_SLK_CROSSCHECK units=466 with_ack_row=392 silent=74 drift=0
ART_E_SOUNDINFO_PATH tables=["AbilitySounds","AmbienceSounds","AnimSounds",
  "DialogSounds","UISounds","UnitAckSounds","UnitCombatSounds"]
  defs=2529 checked=2529 drift=0
```

`silent=74` = shipped units whose `unitSound` family has no ack/attack/death
row in any SoundInfo table (Circle of Power, wards, tents, critters, some
campaign models). They stay silent by data; they never fall back to another
race's set.

### Reference path — zero underruns (observed)

- Orc01 300 s session (`orc01_five_minute_audio_session`): `ok` (113.46 s),
  engine `underruns=0 backend_errors=0 dropped=0`; the test now fails if any of
  those counters moves.
- Device path (real SDL3 queue), shipped layered install:

```
GAME_AUDIO_WIRING audio device=sdl3 source=wc3-install
  layers=["War3xlocal","War3x","War3"] tables=8 defs=2025 units=812 themes=42
GAME_AUDIO_SELECT unit=hpea set=Peasant def=PeasantWhat voice=1
GAME_AUDIO_FIGHT_ATTACK unit=hfoo set=Footman def=FootmanYesAttack voice=7
GAME_AUDIO_FIGHT_DEATH unit=ogru set=Grunt def=GruntDeath voice=8
GAME_AUDIO_SUMMARY started=13/13 voices=12 underruns=0 backend_errors=0
```

Run with `./scripts/h2-cargo run --offline -q -p wc3-assetdb --bin audio_scenario_probe -- --install analysis/agent-assets/wc3-install`, rc=0.
(`voices=12` vs `started=13` is the summary's live/replaced count for the
victory jingle step; the probe rc and every step line are green.)

## Result

Complete for the packet scope:

- named test `artE_perdef_voices` green with >= 8 defs, no set/row/file
  collisions, peasant != orc — now proven against the shipped SLK cells, not
  against the crate's own parser;
- deterministic `def -> set -> row -> file` log recorded by `GameAudio` and
  asserted from the log itself;
- cross-check vs shipped `UI/SoundInfo`: 392 ack rows and 2 529 loaded
  definitions, **drift = 0**;
- `wc3-audio` lib/check rc=0 and full suite rc=0 (100 tests);
- zero underruns on the reference path: headless Orc01 counters 0/0/0 and a
  real SDL3 device probe with `underruns=0 backend_errors=0`.

## Risks / Next (honest boundaries)

- A headless engine has no device queue, so `buffer_underruns` is structurally
  zero there; real queue underruns were measured on the 13-step SDL3 probe, not
  on a 300 s device session. A long device run belongs to device QA (agent G).
- The layered `War3/War3x/War3xlocal` install is present
  (`analysis/agent-assets/wc3-install`), and the layered per-def tests ran (not
  skipped). TTS-only paths were not exercised — this layer contains no TTS
  assets; they remain an honest boundary that needs the layered install if TTS
  is added.
- 74 shipped units are silent because their object data ships no ack rows; this
  is asserted as `silent`, not hidden as drift. Any future fallback to a race
  default for those would be a regression and must be caught by the crosscheck.
- All changes are uncommitted in the shared tree (many agents in flight); the
  parent tech lead owns the merge. No files outside `crates/wc3-audio/**` were
  modified; the device probe binary was only executed.

## Artifacts / logs

- `crates/wc3-audio/tests/artE_perdef_voices.rs`
- `crates/wc3-audio/src/game_audio.rs`, `crates/wc3-audio/tests/orc01_session.rs`
- `$PREFIX/tmp/opencode/artE_oracle_601_run5.log` (named test, rc=0)
- `$PREFIX/tmp/opencode/artE_full_601c.log` (full suite, rc=0)
- `$PREFIX/tmp/opencode/artE_check_601_final.log` (check, rc=0)
- `$PREFIX/tmp/opencode/artE_device_601.log` (SDL3 probe, rc=0)

H2_PERSISTED: capture 1252, experience 4918, monitor 601 checkpoints (resume,
phase 2, final test, this close).
