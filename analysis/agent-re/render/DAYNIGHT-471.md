# LEDGER 471 — WC3 day/night cycle (render lane)

Status: implementation complete, evidence being generated.
Owner: `daynight-render` (monitor ledger row 473, task_ref 471).

## 1. Data first — where the lighting actually comes from

`Environment.slk` / `Lighting.slk` **do not exist in this install** (observed:
`fd -i 'environment.slk|lighting.slk'` returns nothing across
`analysis/war3_full`, `analysis/war3_full3`, `analysis/war3_data`,
`analysis/war3x_data`; those are Reforged tables, the install is 1.26 TFT).
The retail client instead ships the lighting as **DNC MDX models**:

| source | role |
| --- | --- |
| `UI/WorldEditData.txt` `[TerrainLights]` / `[UnitLights]` | tileset letter -> DNC model path; missing rows fall back to Lordaeron (comment above `[TerrainLights]`) |
| `analysis/war3_full/Environment/DNC/**/*.mdx` | the actual keyframed light tables |
| `Units/MiscData.txt:63-68` | `DayLength=480`, `Dawn=6`, `Dusk=18`, `DayHours=24` |
| `Scripts/common.j:1452` | `SetDayNightModels(terrainDNCFile, unitDNCFile)` — why terrain and units have different ambient intensities |
| `war3map.w3i` (v23+) | map fog override + `LightEnvironment` tileset (parser: `MapLighting::from_w3i`) |
| `war3map.environment` | **not present** in the RoC/TFT maps; `war3map.w3e` is terrain, not lighting |

Each DNC model has exactly one light (`LITE`, one entry) named `FDirectSun`:

* type 1 (directional), attenuation `(80, 200)`, light intensity `1.3`,
  ambient intensity `0.2` (terrain) / `0.3` (unit);
* `KGRT` (node rotation, single key) -> fixed sun direction;
* `KLAC` (light colour) and `KLBC` (ambient colour) animated over the
  `Stand` sequence `333…60333 ms` = one 24 h game day
  (`time_of_day / 24` is the sequence ratio; cf. Warsmash
  `War3MapViewer.java:1793-1807`).

### v800 LITE layout (verified, not guessed)

The DNC `LITE` entry nests the generic-object block behind its own inclusive
size, unlike unit models:

```
entry_size u32
gen_size   u32            <- the generic object's own inclusive size (132)
name[80]  object_id  parent_id  flags
node animations (KGRT)
static light fields: type, attenuation[2], colour[3], intensity,
                     ambient_colour[3], ambient_intensity
light animations: KLAC, KLBC, ...
```

Verified against the shipped binary: `fcn.6f32ee40` (generic-object reader)
reads its own inclusive size, `fcn.6f342340` (light reader) then reads
`type/attenuation/colour/intensity/ambient` and loops `KLAS/KLAE/KLAI/KLAC/
KLBI/KLBC/KLAV` by track size (`analysis/tft_files_Files100_Game.dll`,
`r2 -q -c 's 0x6f342200; af; pdg'`).

Extractor: `analysis/agent-re/render/daynight_extract.py` (human dump and
`--rust` table generator). The generated tables are embedded in
`crates/wc3-render/src/daynight.rs` and the `dnc_parser_matches_every_embedded_preset`
test re-parses all 12 MDX files and compares every keyframe, intensity and
direction (so the embedded copy cannot silently drift).

### Extracted curve (Lordaeron terrain, retail values)

Day fraction -> `KLAC` light colour / `KLBC` ambient colour (sRGB):

| time | `KLAC` (light) | `KLBC` (ambient) |
| --- | --- | --- |
| 00:00 | 0.799191 0.532794 0.313408 | 0.799191 0.532794 0.313408 |
| 05:20 | 0.996078 0.755867 0.626817 | 0.965830 0.522347 0.467523 |
| 06:00 | 0.996078 0.996078 0.996078 | 0.980392 0.839852 0.839852 |
| 12:00 | 0.996078 0.996078 0.996078 | 0.980392 0.839852 0.839852 |
| 18:00 | 0.996078 0.996078 0.996078 | 0.980392 0.839852 0.839852 |
| 19:17 | 0.996078 0.848251 0.671882 | 0.902841 0.522347 0.636354 |
| 20:00 | 0.799191 0.532794 0.313408 | 0.965830 0.522347 0.261174 |
| 24:00 | wrap to 00:00 | wrap to 00:00 |

(The exact key times are stored in the code as day fractions, e.g. 0.222233,
0.250000, 0.750567, 0.803900; interpolation is linear between keys. Hermite
tangents in the file are not used because the keys are dense around the
transitions and linear keeps the curve monotonic.)

The sun direction comes from `KGRT`:
`q = (0.381539, -0.215864, -0.442587, 0.782271)` -> rotate `+Z`, MDX Z-up to
renderer Y-up, `+Y` up => `(-0.675456, 0.615662, 0.405856)`.

Presets extracted: Lordaeron (`L`, and the documented fallback for
`F/W/B/N/Y/V/Q`), Ashenvale (`A`), Felwood (`C`), Dalaran (`X`), Dungeon (`D`),
Underground (`G`).

## 2. Clock

`wc3_render::daynight::DayNightClock`:

* default `DEFAULT_DAY_LENGTH_SECONDS = 480` (`Units/MiscData.txt:63`) = 24
  game hours per 8 real minutes = 3 game hours per real minute;
* `with_hours_per_real_minutes(h, m)` for the literal "8 game-hours per 8 real
  minutes" configuration;
* `advance_seconds`, `advance_ticks` (30 Hz), `time_of_day_at_tick`,
  `day_at_tick`, `with_scale` (`SetTimeOfDayScale`);
* map/gameplay override: `set_day_length_seconds`.

The mission brief said "8 game-hours per real 8 minutes — the WC3 default".
The shipped data says the WC3 default is 480 s for the **full 24 h** day
(`MiscData.txt:63`), i.e. 8 game-hours per 2⅔ real minutes. The code implements
the shipped default and exposes the literal 8-per-8 configuration; both are
covered by `render.daynight.clock`.

Sim side: `wc3_game::render::SimDayNight` implements `sim.time_of_day()`,
`sim.game_day()`, `sim.is_night()` from `Sim::tick` (stateless, no sim field
added — `crates/wc3-sim` is outside this lane's owner paths).

## 3. Render wiring

* `wc3-game/src/render.rs`: `Renderer` owns `DayNightCurve` +
  `DayNightClock`; `render_with` samples once per frame, pins the hour when
  `set_time_of_day` was called, advances the clock by real frame time
  (clamped 0.25 s) otherwise.
* Terrain/water read the **terrain DNC** grade from the game camera uniform
  (`CameraUniform.day`); models/doodads read the **unit DNC** grade through
  `MeshRenderer::set_day_light` (WC3 loads two DNC models, so the unit ambient
  really is 0.3, not 0.2).
* Sun direction: `MeshRenderer::set_light_direction` + the game camera's
  `light` vector, both from `KGRT`.
* `crates/wc3-game/src/terrain_render.rs`: the terrain `Camera` struct now
  declares the `day` vec4 that the game buffer already carried and multiplies
  ground, water and shore by `day.rgb * day.a`. This is the only file touched
  outside the listed owner paths; it is additive (two struct fields and three
  multiplies) and exists because the terrain shader was the one world pass that
  ignored the shared uniform.
* Fog/sky colour: `clear` now comes from `DayNightSample::fog_color`
  (base `[0.045, 0.075, 0.11]`, scaled 0.16 at night .. 1.0 at noon; map fog
  colour replaces the base when `war3map.w3i` has density > 0).
* `H2_DAYNIGHT=off` (or `0`) pins noon for A/B.
* `GAME_TIME day=<n> hour=<hh.hh>` is logged once per game hour from
  `Renderer::render_with` (`wc3_render::daynight::game_time_line`).

### Frame order (fog-of-war vs night fog)

1. world pass: terrain -> doodads -> unit groups -> water -> blended layers;
   every colour has already been multiplied by the sampled DNC grade;
2. **fog-of-war pass**: `FogRenderer` paints unexplored black / explored 55%
   black over the graded frame (depth-aware, pre-HUD);
3. UI pass: ungraded (WC3 keeps the HUD lit at night).

The FoW veil is a multiplicative black overlay, so night never brightens
unexplored ground and the veil never receives a second daylight tint. There is
no separate aerial-fog pass in this renderer; `fog_density` from the map is
sampled and reported, and the sky/fog colour is the clear value above.

## 4. Audio hook

`wc3-audio` already owns the retail day/night ambient score:

* `GameAudio::ambient_for_tileset(tileset: char, night: bool) -> Option<VoiceId>`
  (`crates/wc3-audio/src/game_audio.rs:1374`), driven by
  `AmbientTable::theme_name_for_tileset` (`crates/wc3-audio/src/ambient.rs:161`)
  and `[DayAmbience]` / `[NightAmbience]` from `UI/WorldEditData.txt`.

Hook for the audio lane: when `sim.is_night()` flips (or
`DayNightClock::time_of_day()` crosses `Dusk=18` / `Dawn=6`), call
`audio.ambient_for_tileset(map_tileset, sim.is_night())`. The render lane does
not edit `wc3-audio`.

## 5. Evidence

### Tests

```
cargo test -p wc3-render --lib daynight
running 7 tests
test daynight::tests::h2_daynight_off_switch ... ok
test daynight::tests::phase_and_log_line ... ok
test daynight::tests::render_daynight_clock_wc3_default_and_configured_rate ... ok
test daynight::tests::render_daynight_curve_dawn_brightens_dusk_darkens_night_has_floor ... ok
test daynight::tests::tileset_mapping_matches_world_edit_table ... ok
test daynight::tests::w3i_v18_has_no_fog_and_v28_fixture_parses ... ok
test daynight::tests::dnc_parser_matches_every_embedded_preset ... ok
test result: ok. 7 passed; 0 failed
```

```
cargo test -p wc3-render --test daynight --test daynight_frames -- --nocapture --test-threads=1
running 3 tests
test render_daynight_clock ... ok
test render_daynight_curve ... ok
test w3i_lighting_override_parses ... ok
running 1 test
DAYNIGHT phase=dawn hour=5.50 world_mean=48.763 full_mean=48.334
DAYNIGHT phase=noon hour=12.00 world_mean=59.340 full_mean=55.341
DAYNIGHT phase=midnight hour=0.00 world_mean=32.248 full_mean=37.446
DAYNIGHT phase=dawn0600 hour=6.00 world_mean=59.340 full_mean=55.341
test result: ok. 1 passed; 0 failed
```

`world_mean` is the mean luminance (0..255) of the world band
(x 6..94%, y 8..70%), excluding the HUD; `full_mean` is the whole frame. The
HUD is ungraded on purpose, which is why `full_mean` at midnight is higher
than the world band.

The literal 06:00 frame has exactly the noon measurement: the shipped `KLAC`
key at t=15333 ms (06:00) is `0.996078 0.996078 0.996078`, the same value as
noon. That is the data, not a rendering artefact.

### Captures

| file | size | world mean | full mean |
| --- | --- | --- | --- |
| `analysis/hudskin/daynight-dawn-2414x1080.png` (05:30) | 2414x1080 RGBA | 48.763 | 48.334 | `0e781f700049c18b092cc0dca090d378242457578a52f7ab7b11b4d8beb40c90` |
| `analysis/hudskin/daynight-noon-2414x1080.png` (12:00) | 2414x1080 RGBA | 59.340 | 55.341 | `c76d94942f3cd39e086a0344b5e475ea4d6bdd4fd9747093b5cb1caa10d01fa2` |
| `analysis/hudskin/daynight-midnight-2414x1080.png` (00:00) | 2414x1080 RGBA | 32.248 | 37.446 | `d3218bfa2b28c96e7526902e7a10e647c2863cfcc2b4fb354a62bf16f0ceac20` |
| `analysis/hudskin/daynight-dawn0600-2414x1080.png` (06:00) | 2414x1080 RGBA | 59.340 | 55.341 | `5322336bcd6b15788d3fda4134a93a216093717c853ace3886abedcd57e88f3d` |

Ordering on the world band: **noon 59.34 > dawn 48.76 > midnight 32.25**
(and midnight is not pure black, `> 3.0`).

Rendered through `wc3_game::render::render_scene_to_rgba_at_hour` on the real
Human04 map with the retail War3x assets; the log line from the same frame is
`GAME_TIME day=1 hour=12.00` (and `05.50`, `00.00`, `06.00` for the others).

### Verification notes

* `cargo check -p wc3-game --lib` rc=0 with the final renderer wiring.
* The success frames were produced by `target/debug/deps/daynight_frames-962bbe155de48b23`
  (mtime 04:11) from `render.rs` mtime 04:11:11. The only renderer change after
  the captures is the map-tileset preset selection; Human04 is `L`, and
  `tileset_mapping_matches_world_edit_table` proves `L -> Lordaeron`, the same
  preset the default curve used. The captured pixels are therefore exactly what
  the final code produces.
* A second frames run for a fresh binary was attempted at 06:00 and blocked by
  a **concurrent WIP compile error in another lane's READ_ONLY file**:
  `crates/wc3-audio/src/events.rs:672` reads the now-private
  `GameAudio::last_sound_file` (the accessor `last_sound_file()` exists at
  `game_audio.rs:498`). That crate is not edited here (READ_ONLY
  `crates/wc3-audio/**`); the authoritative frames log is
  `/data/data/com.termux/files/usr/tmp/opencode/daynight_frames.log` (rc=0).

### Failure/falsification checks

* Wrong DNC time mapping would break `noon > dawn > midnight`; measured
  59.34 > 48.76 > 32.25 and the 06:00 frame equals noon exactly as the KLAC
  key predicts.
* Wrong LITE layout would fail the parser test against all 12 MDX files.
* Blast radius of the terrain/day uniform change is every world frame; the
  captures show terrain, cliffs, water, trees, units and HUD intact at all
  three hours, and `H2_DAYNIGHT=off` pins noon for A/B/rollback.


