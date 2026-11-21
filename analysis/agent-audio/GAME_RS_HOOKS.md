# game.rs hook changes — exact line numbers (READ_ONLY file)

`crates/wc3-game/src/game.rs` is owned by another layer agent for this pass, so
the call-site changes are specified here instead of applied. All referenced
`wc3-audio` APIs exist and are covered by `cargo test -p wc3-audio --offline`.

Line numbers are against the current `game.rs` (2 900+ lines, checked
2026-11-15). Apply in this order; every patch is independent.

---

## 1. Music + tileset ambience on scenario start

`game.rs:444-455` (`start_audio_for_scenario`), currently:

```rust
    fn start_audio_for_scenario(&mut self) {
        if let Some(audio) = self.assets.audio.as_mut() {
            audio.set_listener(
                Vec3::new(self.camera.target.x, 0.0, self.camera.target.y),
                0.0,
            );
            let _ = audio.music("Human1");
        }
        self.audio_listener = Some(self.camera.target);
    }
```

Replace with:

```rust
    fn start_audio_for_scenario(&mut self) {
        // The player's race decides the in-game theme (Justice melee: Orc).
        let race = self
            .player_audio_race()
            .unwrap_or(wc3_audio::events::Race::Human);
        let tileset = self
            .assets
            .terrain
            .as_ref()
            .map(|terrain| terrain.tileset as char);
        if let Some(audio) = self.assets.audio.as_mut() {
            audio.set_listener(
                Vec3::new(self.camera.target.x, 0.0, self.camera.target.y),
                0.0,
            );
            if audio.music_for_player_race(race).is_none() {
                let _ = audio.music("Human1");
            }
            if let Some(tileset) = tileset {
                // One loop per region: a stable key keeps the loop running
                // without restarts; a change stops the old loop first.
                let _ = audio.ambient_for_region(self.assets.map_name.as_str(), tileset, false);
            }
        }
        self.audio_listener = Some(self.camera.target);
    }
```

Why: `music("Human1")` was hardcoded; `player_audio_race()` already exists at
`game.rs:2183` and returns the player hall's race. `ambient_for_tileset` takes
`Terrain::tileset` (`'Y'` for Justice -> CityScapeDay, `'L'` for the campaign
maps -> LordaeronSummerDay). The ambient loop is rendered from the shipped
MIDI+DLS pair; no clip is invented.

Day/night: `night` is `false` here. When a day/night clock exists, call
`ambient_for_region(region, letter, night)` again; the loop restarts only when
the region/tileset/day-night key changes. `leave_ambient_region()` stops it
(e.g. on scenario exit). The region key is the map name until map-region
(`war3map.w3r`) tracking is wired; any changing key works.

### 1b. Audibility gate — no game.rs change needed

`GameAudio` applies the retail `StartSound` rule per row:
`dist² > DistanceCutoff²` never starts (row cutoff, or 3000 when the row stores
0). `update_audio` already calls `audio.set_listener(...)` every frame, so the
device log's `GAME_AUDIO_DEATH … dist=` lines can be asserted per class
(`analysis/tech-lead/check_audio_earshot.py`): ack 100000, combat 2100,
ability 2000, ambience 900–2500, death 3000. Ambient/music are never gated.
`audio.set_audible_radius(units)` remains only an optional QA cap (default 0 =
off).

---

## 2. Do not acknowledge the same selection twice

`game.rs:481-500` (`update_audio`) acknowledges whenever the selection leader
changes; `game.rs:2765` (`tap_action`) also calls `play_unit_ack`. A plain tap
therefore started two lines (one from the tap, one from the next frame).

### 2a. `game.rs:2764-2766` — mark the tap as acknowledged

```rust
                    crate::ui::selection::tap_select(self, id, self.touch.additive());
                    if let Some(first) = self.selection.last().copied() {
                        self.play_unit_ack(first);
                        // The tap already acknowledged: stop update_audio from
                        // repeating the same line on the next frame.
                        self.audio_last_selection = Some(first);
                    }
```

### 2b. `game.rs:483-484` — reset the tap counter on a real selection change

```rust
        if leader != self.audio_last_selection {
            self.audio_last_selection = leader;
            if let Some(audio) = self.assets.audio.as_mut() {
                audio.selection_reset();
            }
```

(`selection_reset` clears the consecutive-tap counter so a different unit starts
again at "What?" instead of continuing another unit's Pissed streak.)

### 2c. `game.rs:1294-1319` — `play_unit_ack` uses the retail tap path

Current body calls `audio.unit_ack_set(&set, ...)`. Change the play call to:

```rust
        let Some(_set) = self.assets.unit_sound_set(def_id) else {
            return;
        };
        // Every 4th consecutive tap on the same unit plays {set}Pissed.
        let played = self
            .assets
            .audio
            .as_mut()
            .and_then(|audio| audio.selection_tap(def_id, Some(pos)));
        let last = self
            .assets
            .audio
            .as_ref()
            .and_then(|audio| audio.last_event_line())
            .unwrap_or("")
            .to_string();
        match played {
            Some(voice) => {
                crate::dev_log!("GAME_AUDIO_ACK unit={def_id} voice={} {last}", voice.0)
            }
            None => crate::dev_log!("GAME_AUDIO_ACK unit={def_id} def=none"),
        }
```

`last_event_line()` is `AUDIO_ACK set=Footman def=FootmanWhat …` or
`AUDIO_PISSED set=Footman def=FootmanPissed …`, so logcat shows the exact
shipped row. If the `_set` guard is unwanted, drop it — `selection_tap` already
resolves by def id (alias-aware).

---

## 3. Research completion sound (currently silent)

`game.rs:2107-2116` handles `Event::UnitTrained`; the next arm is
`Event::Damage` at `game.rs:2117`. Insert **between them**:

```rust
                Event::UpgradeResearched { owner, id } => {
                    if owner == self.player
                        && let Some(race) = self.player_audio_race()
                    {
                        let played = self.assets.audio.as_mut().and_then(|audio| {
                            audio.play_event(
                                wc3_audio::events::AudioEvent::UpgradeComplete,
                                None,
                                Some(race),
                                None,
                            )
                        });
                        if let Some(voice) = played {
                            crate::dev_log!(
                                "GAME_AUDIO_UPGRADE upgrade={id} race={} voice={}",
                                race.id(),
                                voice.0
                            );
                        }
                    }
                }
```

The sound row is `UpgradeComplete<Race>` from `UISounds.slk`; it exists for all
four races (verified by `audio_events_every_retail_event_has_a_mapped_sound`).

---

## 4. Build-complete line in the device log

`game.rs:2097-2106` (`Event::BuildingComplete`) currently discards the voice id.
Replace the inner `if let Some(race)` block with:

```rust
                        if let Some(audio) = self.assets.audio.as_mut() {
                            if let Some(race) = audio_race(unit.def().race) {
                                if let Some(voice) = audio.job_done(race, Some(position)) {
                                    crate::dev_log!(
                                        "GAME_AUDIO_BUILD_COMPLETE race={} voice={}",
                                        race.id(),
                                        voice.0
                                    );
                                }
                            }
                        }
```

`job_done` routes through `play_event`, which also sets
`last_event_line() = AUDIO_EVENT event=build_complete …`.

---

## 5. Map `war3map.w3s` ambience (optional, separate from tileset ambience)

The melee maps ship their own loops (`gg_snd_WaterLakeLoop1`,
`gg_snd_WaterStreamLoop1`, …). The hook layer already has
`GameAudio::play_map_ambient(&MapSound, &MapRegion)`, but `parse_w3s` keeps the
60-byte trailer opaque and `MapSound` has no position, so a game-side hook needs
one of:

* **A (preferred):** extend `wc3-audio::map_sounds` with the w3s v1 trailer
  fields (`fadeIn/fadeOut/volume/pitch/pitchVariance/priority/channel/
  minDistance/maxDistance/cutoff/orientation/cone/x/y/z`) and start every
  looping record at its own `(x, y, z)`. The record layout is in the file, but
  it is **not decoded yet** — do not guess it in game.rs.
* **B (interim):** keep the existing `MapSounds` + region-centre API and pair a
  record with a region only where the map author used matching names (the
  Human04 pair used by the probe is `gg_snd_WaterLakeLoop1` / `LakeLoop01`;
  that pairing is a probe convenience, not a retail rule).

Until A lands, tileset ambience (section 1) already removes the silence on
Justice, and the per-unit voices are unaffected.

---

## 6. Device stats line (app layer, not game.rs)

`apps/android-native/src/lib.rs:1460-1482` formats `GAME_AUDIO_STATS` from
`audio.audio_stats()`. Replace the block with:

```rust
            if let Some(audio) = game.assets.audio.as_ref() {
                log(&audio.stats_line());
            }
```

`stats_line()` emits `GAME_AUDIO_HOOKS` with hook counters
(`acks/missing/gated/ui/events/events_gated/music/ambient/taps/unresolved`)
**and** the engine
counters (`buffered_ms`, `underruns`, `dropped`, `occluded`,
`ducked_blocks`, `voices`, `max_active`) in one line. If the old
`GAME_AUDIO_STATS` prefix is required by existing log greps, keep the old line
and add `log(&audio.stats_line());` next to it.

---

## 7. Scenario reset

`game.rs:2335` re-runs `start_audio_for_scenario()` on the menu→game transition;
section 1 covers the music/ambience switch. If a scenario change must silence
the previous map, call `audio.stop_ambient()` + `audio.stop_music()` before
re-entering (both exist on `GameAudio`); the current flow replaces the music
voice and the ambient voice, so no stale loop is left.
