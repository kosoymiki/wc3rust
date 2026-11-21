# PER-DEF UNIT VOICES + DISTANCE CUTOFFS (ledger 442)

**Date:** 2026-11-16
**Symptom (operator row 4):** «Voicing: peasant sounds like an orc».
**Missing artifact:** acks for 8 distinct defs in one run (resolved rows + files).
**Scope:** `crates/wc3-audio/**`, `crates/wc3-assetdb/src/{audio_provider.rs,bin/unit_voice_paths.rs}`, `analysis/audio/**`.

## 1. Mapping contract (observed on the shipped data)

The retail chain is three data cells, no heuristics:

1. `Units\unitUI.slk` row `unitUIID` (`hpea`, `opeo`, `ogru`, ...) → **`unitSound`** cell.
   * TFT (`War3x.mpq`, 812 rows): header `unitUIID … file … unitSound … name`, so
     `hpea`→`Peasant` (X5), `opeo`→`Peon`, `ogru`→`Grunt`, `ohun`→`HeadHunter`,
     `oshm`→`Shaman`, `otau`→`Tauren`, `okod`→`KotoBeast`, `owyv`→`WyvernRider`,
     `ospw`→`SpiritWalker`, `hfoo`→`Footman`, `hrif`→`Rifleman`, `hmpr`→`Priest`,
     `hsor`→`Sorceress`, `hkni`→`Knight`, `hmtm`→`MortarTeam`, `hgry`→`GryphonRider`.
   * RoC (`War3.mpq`, 466 rows): no `unitSound` column — the display `name` (X3)
     doubles as the set (`hpea`→`Peasant`). The bug report's build/dataset used
     the RoC table for TFT content, which is why the fallback matters.
2. `UI\SoundInfo\UnitAckSounds.slk` row **`{set}What`** (first), then `{set}Yes`,
   `{set}YesAttack`, `{set}Pissed`, `{set}Warcry` (retail pick order).
3. The row's `DirectoryBase` + `FileNames` → `Units\Human\Peasant\PeasantWhat1..4.wav`
   (variant chosen per play, `RANDOMPITCH`/`NODUPEUSERNAMES` flags honoured by the mixer).

`DistanceCutoff` is a per-row field on that same row and stays authoritative:
retail acks ship **100000**, `UnitCombatSounds` ships **2100**; the audible gate is
`dist² > cutoff²` (`Game.dll CSoundManager::StartSound` `0x6f102980`, `fcom 0x6f102b0b`,
see `analysis/agent-re/ui_audio/ENGINE_MAP.md` §4.3).

## 2. Root cause of the old wrong voice

* `crates/wc3-game/src/game.rs` `play_unit_ack` built **file names** as definitions
  (`{set}Yes1..3`, `{set}What1..2`). `UnitAckSounds` rows are `{set}What`/`{set}Yes`;
  the numbered entries are *files inside* the row. `play_first` found nothing, so a
  shared/generic fallback could voice the wrong unit (or silence the acks).
  Fixed previously by the set-based hooks in `crates/wc3-audio/src/events.rs`
  (`unit_ack_set` / `unit_attack_set` / `unit_death_set`).
* Ledger 442 closes the remaining gap in the **selection-ack** path
  (`GameAudio::unit_ack`, which resolves the set itself from `unitUI.slk`):
  the lookup now walks the def's own object-data candidates — TFT `unitSound`
  first, display-name set second (RoC schema / empty cell) — and takes the first
  candidate that owns a real `UnitAckSounds` row. It never falls back to a
  race-generic or another def's set, and `unit_ack_definition()` exposes the
  selected set + row so probes/logs can prove which cell chose the file.

## 3. Changes

| file | change |
|---|---|
| `crates/wc3-audio/src/soundinfo.rs` | `UnitNameMap::sound_set_candidates()` — authoritative `unitSound` first, display-name set second, de-duplicated, data-only (empty for unknown defs). |
| `crates/wc3-audio/src/game_audio.rs` | `GameAudio::{unit_ack,unit_attack,unit_death,unit_pissed}_definition()` return `(set, SoundDef)` from the candidate list; the four per-def hooks use them instead of a single `sound_set` lookup. `play_sound` keeps the `AUDIO_GATED` line when a row is culled. |
| `crates/wc3-audio/tests/audio.rs` | `audio_voices_per_def_each_def_resolves_its_own_ack_file` and `audio_voices_distance_gate_ack_1200_combat_5000` over the production layered install. |
| `crates/wc3-assetdb/src/bin/unit_voice_paths.rs` | headless probe: 8 Orc + 8 Human defs, prints `VOICE_ROW`/`VOICE_PATH`, exits 1 on a cross-row file collision. |
| `crates/wc3-assetdb/tests/{hook_resolution,audio_hooks,audio_parity}.rs` | pre-existing break met in-session: `AudioConfig` initializers missed `channel_voices` (field added by the engine channel work), so the asset-side audio tests did not compile. Added `channel_voices: DEFAULT_CHANNEL_VOICES` — no behaviour change, tree green. |

## 4. Evidence

### 4.1 Probe (8 Orc defs + 8 Human counterparts, one run)

```text
VOICE_WIRING source=wc3-install layers=["War3xlocal", "War3x", "War3"] tables=8 defs=2025 units=812
VOICE_ROW def=opeo group=orc set=Peon expected_set=Peon row=PeonWhat table=UnitAckSounds dir="Units\Orc\Peon\" files=4 cutoff=100000
VOICE_PATH def=opeo group=orc row=PeonWhat file=Units\Orc\Peon\PeonWhat4.wav
VOICE_ROW def=ogru group=orc set=Grunt expected_set=Grunt row=GruntWhat table=UnitAckSounds dir="Units\Orc\Grunt\" files=4 cutoff=100000
VOICE_PATH def=ogru group=orc row=GruntWhat file=Units\Orc\Grunt\GruntWhat4.wav
VOICE_ROW def=ohun group=orc set=HeadHunter expected_set=HeadHunter row=HeadHunterWhat table=UnitAckSounds dir="Units\Orc\HeadHunter\" files=4 cutoff=100000
VOICE_PATH def=ohun group=orc row=HeadHunterWhat file=Units\Orc\HeadHunter\HeadHunterWhat2.wav
VOICE_ROW def=oshm group=orc set=Shaman expected_set=Shaman row=ShamanWhat table=UnitAckSounds dir="Units\Orc\Shaman\" files=4 cutoff=100000
VOICE_PATH def=oshm group=orc row=ShamanWhat file=Units\Orc\Shaman\ShamanWhat3.wav
VOICE_ROW def=otau group=orc set=Tauren expected_set=Tauren row=TaurenWhat table=UnitAckSounds dir="Units\Orc\Tauren\" files=3 cutoff=100000
VOICE_PATH def=otau group=orc row=TaurenWhat file=Units\Orc\Tauren\TaurenWhat2.wav
VOICE_ROW def=okod group=orc set=KotoBeast expected_set=KotoBeast row=KotoBeastWhat table=UnitAckSounds dir="Units\Orc\KotoBeast\" files=3 cutoff=100000
VOICE_PATH def=okod group=orc row=KotoBeastWhat file=Units\Orc\KotoBeast\KotoBeastWhat2.wav
VOICE_ROW def=owyv group=orc set=WyvernRider expected_set=WyvernRider row=WyvernRiderWhat table=UnitAckSounds dir="Units\Orc\WyvernRider\" files=4 cutoff=100000
VOICE_PATH def=owyv group=orc row=WyvernRiderWhat file=Units\Orc\WyvernRider\WyvernRiderWhat2.wav
VOICE_ROW def=ospw group=orc set=SpiritWalker expected_set=SpiritWalker row=SpiritWalkerWhat table=UnitAckSounds dir="Units\Orc\SpiritWalker\" files=5 cutoff=100000
VOICE_PATH def=ospw group=orc row=SpiritWalkerWhat file=Units\Orc\SpiritWalker\WhiteTaurenWhat1.wav
VOICE_ROW def=hpea group=human set=Peasant expected_set=Peasant row=PeasantWhat table=UnitAckSounds dir="Units\Human\Peasant\" files=4 cutoff=100000
VOICE_PATH def=hpea group=human row=PeasantWhat file=Units\Human\Peasant\PeasantWhat4.wav
VOICE_ROW def=hfoo group=human set=Footman expected_set=Footman row=FootmanWhat table=UnitAckSounds dir="Units\Human\Footman\" files=4 cutoff=100000
VOICE_PATH def=hfoo group=human row=FootmanWhat file=Units\Human\Footman\FootmanWhat2.wav
VOICE_ROW def=hrif group=human set=Rifleman expected_set=Rifleman row=RiflemanWhat table=UnitAckSounds dir="Units\Human\Rifleman\" files=4 cutoff=100000
VOICE_PATH def=hrif group=human row=RiflemanWhat file=Units\Human\Rifleman\RiflemanWhat1.wav
VOICE_ROW def=hmpr group=human set=Priest expected_set=Priest row=PriestWhat table=UnitAckSounds dir="Units\Human\Priest\" files=4 cutoff=100000
VOICE_PATH def=hmpr group=human row=PriestWhat file=Units\Human\Priest\PriestWhat4.wav
VOICE_ROW def=hsor group=human set=Sorceress expected_set=Sorceress row=SorceressWhat table=UnitAckSounds dir="Units\Human\Sorceress\" files=5 cutoff=100000
VOICE_PATH def=hsor group=human row=SorceressWhat file=Units\Human\Sorceress\SorceressWhat5.wav
VOICE_ROW def=hkni group=human set=Knight expected_set=Knight row=KnightWhat table=UnitAckSounds dir="Units\Human\Knight\" files=4 cutoff=100000
VOICE_PATH def=hkni group=human row=KnightWhat file=Units\Human\Knight\KnightWhat4.wav
VOICE_ROW def=hmtm group=human set=MortarTeam expected_set=MortarTeam row=MortarTeamWhat table=UnitAckSounds dir="Units\Human\MortarTeam\" files=4 cutoff=100000
VOICE_PATH def=hmtm group=human row=MortarTeamWhat file=Units\Human\MortarTeam\MortarTeamWhat3.wav
VOICE_ROW def=hgry group=human set=GryphonRider expected_set=GryphonRider row=GryphonRiderWhat table=UnitAckSounds dir="Units\Human\GryphonRider\" files=5 cutoff=100000
VOICE_PATH def=hgry group=human row=GryphonRiderWhat file=Units\Human\GryphonRider\GryphonRiderWhat3.wav
VOICE_DISTINCT defs=16 rows=16 files=16 collisions=0
VOICE_ORC_DISTINCT defs=8 files=8
VOICE_CUTOFF def=- group=combat row=AxeMediumChopWood table=UnitCombatSounds cutoff=2100
```

`rc=0`; 16/16 distinct rows and files, 8/8 Orc defs distinct. No two defs share a
file, so the "peasant speaks with an Orc file" defect is not present on this tree;
the old root cause (row named `{set}What1` instead of row `{set}What`) is documented
in §2. Individual variant files differ per run (RNG pick inside the row) — the row
and directory are deterministic.

### 4.2 Tests

```text
running 2 tests
test audio_voices_distance_gate_ack_1200_combat_5000 ... VOICE_GATE line=AUDIO_GATED AxeMediumChopWood distance=5000.0 cutoff=2100.0 radius=0.0 voice=none
test audio_voices_per_def_each_def_resolves_its_own_ack_file ... VOICE_PATH def=opeo set=Peon row=PeonWhat file=Units\Orc\Peon\PeonWhat4.wav
... (16 VOICE_PATH lines, one per reference def)
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 20 filtered out; finished in 0.89s
```

### 4.3 SLK rows (observed)

| def | unitSound (unitUI.slk) | ack row | DirectoryBase (UnitAckSounds.slk) | Files | DistanceCutoff |
|---|---|---|---|---|---|
| opeo | Peon | PeonWhat | `Units\Orc\Peon\` | PeonWhat1..4.wav | 100000 |
| ogru | Grunt | GruntWhat | `Units\Orc\Grunt\` | GruntWhat1..4.wav | 100000 |
| ohun | HeadHunter | HeadHunterWhat | `Units\Orc\HeadHunter\` | HeadHunterWhat1..4.wav | 100000 |
| oshm | Shaman | ShamanWhat | `Units\Orc\Shaman\` | ShamanWhat1..4.wav | 100000 |
| otau | Tauren | TaurenWhat | `Units\Orc\Tauren\` | TaurenWhat1..3.wav | 100000 |
| okod | KotoBeast | KotoBeastWhat | `Units\Orc\KotoBeast\` | KotoBeastWhat1..3.wav | 100000 |
| owyv | WyvernRider | WyvernRiderWhat | `Units\Orc\WyvernRider\` | WyvernRiderWhat1..4.wav | 100000 |
| ospw | SpiritWalker | SpiritWalkerWhat | `Units\Orc\SpiritWalker\` | WhiteTaurenWhat1..5.wav | 100000 |
| hpea | Peasant | PeasantWhat | `Units\Human\Peasant\` | PeasantWhat1..4.wav | 100000 |
| hfoo | Footman | FootmanWhat | `Units\Human\Footman\` | FootmanWhat1..4.wav | 100000 |
| hrif | Rifleman | RiflemanWhat | `Units\Human\Rifleman\` | RiflemanWhat1..4.wav | 100000 |
| hmpr | Priest | PriestWhat | `Units\Human\Priest\` | PriestWhat1..4.wav | 100000 |
| hsor | Sorceress | SorceressWhat | `Units\Human\Sorceress\` | SorceressWhat1..5.wav | 100000 |
| hkni | Knight | KnightWhat | `Units\Human\Knight\` | KnightWhat1..4.wav | 100000 |
| hmtm | MortarTeam | MortarTeamWhat | `Units\Human\MortarTeam\` | MortarTeamWhat1..4.wav | 100000 |
| hgry | GryphonRider | GryphonRiderWhat | `Units\Human\GryphonRider\` | GryphonRiderWhat1..4.wav | 100000 |

`UnitCombatSounds.slk` reference row `AxeMediumChopWood`: `Sound\Units\Combat\`,
4 files, `MinDistance 600`, `MaxDistance 10000`, **`DistanceCutoff 2100`**.

### 4.4 Distance gate

| sound | row cutoff | distance | result |
|---|---:|---:|---|
| `PeonWhat` (unit ack) | 100000 | 1200 | starts (`1200² < 100000²`) |
| `AxeMediumChopWood` (combat) | 2100 | 1200 | starts (inside cutoff) |
| `AxeMediumChopWood` (combat) | 2100 | 5000 | silent (`5000² > 2100²`), `AUDIO_GATED … cutoff=2100`, `voices_gated += 1` |
| `PeasantWhat` (unit ack) | 100000 | 5000 | starts (acks are not distance-culled at combat range) |

## 5. rc

| check | rc |
|---|---:|
| `scripts/h2-cargo check -p wc3-audio --tests --offline` | **0** |
| `scripts/h2-cargo test -p wc3-audio --test audio audio_voices_ -- --nocapture --test-threads=1` | **0** (2 passed) |
| `scripts/h2-cargo test -p wc3-audio --offline` (full crate: lib + 4 suites) | **0** (47 + 22 + 1 + 2 passed, 0 failed) |
| `./target/debug/unit_voice_paths` (headless, layered install) | **0** (16/16 distinct, 0 collisions) |
| `scripts/h2-cargo build -p wc3-assetdb --bins --offline` | **0** (all bins, incl. `unit_voice_paths`) |
| `scripts/h2-cargo test -p wc3-assetdb --test hook_resolution --test audio_hooks --test audio_parity` | **0** (4 + 3 + 2 passed; tree green after the `channel_voices` fix) |

## 6. H2

Ledger 442 (`wc3rust/audio-acks`): register → checkpoint per phase → capture
`wc3rust/audio-acks` → experience-add → close. Evidence IDs in the session report.
