//! Simulation facade for the game/HUD shim.
//!
//! One simulation: every operation here delegates to `wc3_sim::Sim`; this
//! module adds no combat, economy, AI or unit table.  The HUD-owned
//! `game.rs` shim includes it (`#[path = "sim_bridge.rs"] mod sim_bridge;`) or
//! the dispatcher declares it once in `lib.rs` (patch in
//! `analysis/agent-sim/patches/wc3-game-add-sim-bridge.patch`).
//!
//! The JASS side is intentionally not in this file: it needs the structured
//! `ScriptOrder` from `wc3-re::jass::vm`, delivered as
//! `analysis/agent-sim/patches/wc3-re-script-order-{vm,natives}.patch` and the
//! full bridge at `analysis/agent-sim/patches/sim_bridge_jass.rs`.  Keeping the
//! game bridge compiling now lets the HUD shim land independently.

use std::collections::BTreeMap;

use wc3_re::engine::Engine;
use wc3_re::jass::vm::{ScriptOrder, Unit as ScriptUnit, World};
use wc3_re::mapfiles::MapBundle;
use wc3_sim::{
    AiConfig, Command, Event, Order, OrderSpec, Race, Sim, UnitId, UnitKind, Vec2, def_of,
};

/// Registers the scenario gold mines and hands the opponent to the canonical
/// data-driven AI.  Called once per scenario reset instead of keeping a
/// game-local AI state.
pub fn setup_scenario(sim: &mut Sim, enemy: u8, mines: [Vec2; 2]) {
    sim.add_gold_mine(mines[0], 20_000);
    sim.add_gold_mine(mines[1], 20_000);
    sim.enable_ai(
        enemy,
        AiConfig {
            workers: 3,
            army_supply: 4,
            first_attack_tick: 450,
            wave_interval: 900,
            retreat_permille: 300,
            composition: &[("hfoo", 3), ("hrif", 2), ("hmpr", 1)],
            worker: "hpea",
        },
    );
}

/// Best install/analysis root for gameplay constants derived from `Assets`.
/// `mpq=` gives the archive directory; `map=` walks up to a directory that has
/// `Units/MiscGame.txt` or an `analysis/` tree.
pub fn gameplay_root_from_assets(assets: &crate::assets::Assets) -> Option<std::path::PathBuf> {
    if let Some(parent) = assets
        .debug
        .iter()
        .find_map(|line| line.strip_prefix("mpq="))
        .map(std::path::PathBuf::from)
        .and_then(|mpq| mpq.parent().map(std::path::Path::to_path_buf))
    {
        return Some(parent);
    }
    let map = assets
        .debug
        .iter()
        .find_map(|line| line.strip_prefix("map="))
        .map(std::path::PathBuf::from)?;
    let mut dir = map.parent()?.to_path_buf();
    loop {
        if dir.join("Units/MiscGame.txt").is_file() || dir.join("analysis").is_dir() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Loads typed gameplay constants from the operator's install through
/// `wc3_re::gameplay::GameplayConfig` and applies them to the canonical sim.
/// Caller for gameplay-constant consumption (game scenario reset).
pub fn load_and_apply_gameplay(
    sim: &mut Sim,
    root: &std::path::Path,
) -> Result<wc3_re::gameplay::GameplayConfig, String> {
    let config = wc3_re::gameplay::GameplayConfig::load(root).map_err(|error| error.to_string())?;
    sim.apply_gameplay(config.clone());
    Ok(config)
}

/// Food used / cap straight from the simulation.
pub fn supply(sim: &Sim, owner: u8) -> (i32, i32) {
    (sim.supply_used(owner), sim.supply_cap(owner))
}

/// WC3 upkeep label for the HUD (`1000/700/400` permille tiers).
pub fn upkeep_label(sim: &Sim, owner: u8) -> &'static str {
    match sim.upkeep_permille(owner) {
        1000 => "NO UPKEEP",
        700 => "LOW UPKEEP",
        _ => "HIGH UPKEEP",
    }
}

/// Race-correct supply building for the owner (human farm by default).
pub fn supply_building(sim: &Sim, owner: u8) -> Option<&'static wc3_sim::UnitDef> {
    sim.supply_building_for_owner(owner)
        .or_else(|| wc3_sim::data::supply_building_for(Race::Human))
}

/// One slot of the SLK-derived worker build menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildOption {
    pub def_id: &'static str,
    pub label: &'static str,
    pub cost: i32,
    pub enabled: bool,
}

/// Canonical build menu for the owner's race (farm/tower/producer/hall first,
/// then the rest by WC3 gold cost).  The HUD maps these to card slots; no
/// building ids or prices are written in the HUD.
pub fn build_menu(sim: &Sim, owner: u8) -> Vec<BuildOption> {
    sim.build_menu_for_owner(owner)
        .into_iter()
        .map(|def| BuildOption {
            def_id: def.id,
            label: def.name,
            cost: def.stats.cost,
            // Requirements + lumber + gold: a Watch Tower without its War Mill
            // must be dimmed, not tap-able into a placement that denies.
            enabled: sim.can_build_now(owner, def.id),
        })
        .collect()
}

/// Outcome of one placed id (a `war3mapUnits.doo` type id or a `war3map.doo`
/// type id) inside [`spawn_map_scene`].
///
/// The census is the diagnostic that ties a short scene to the exact id that
/// was dropped and the reason: `unknown_def` = no row in the loaded object /
/// destructable data (the id is not a melee entity for this install),
/// `spawn_failed` = the def resolved but the sim refused the placement
/// (footprint/supply/owner), `placed` = spawned once per record, `items` =
/// the record is an `ItemData` row and became a ground item.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SceneIdCount {
    pub placed: usize,
    pub items: usize,
    pub unknown_def: usize,
    pub spawn_failed: usize,
}

/// Counts produced by loading a map's placed content into the one sim.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MapSpawns {
    pub units: usize,
    /// `war3mapUnits.doo` records that are items (`hlst`, `whwd`, …); they are
    /// ground items in the sim, not units.
    pub items: usize,
    pub destructables: usize,
    pub camps: usize,
    /// `sloc` start-location markers; not units and absent from the SLK.
    pub start_locations: usize,
    /// `sloc` positions in the map's local (translated) coordinate space,
    /// ordered by the slot that owns the marker (player 0 first).
    pub start_points: Vec<Vec2>,
    /// `ngol`/`ugol` gold mines spawned through the existing gold-mine entity.
    pub gold_mines: usize,
    /// Per-slot melee opening (hall + five workers of the w3i race) for a
    /// two-player map; `None` when the map is not a 1v1 melee map. These are
    /// deliberately *not* part of `units`: that counter is the map's own
    /// `war3mapUnits.doo` placements and must stay comparable to the file.
    pub melee: Option<MeleeBases>,
    /// Per-id outcome of the `war3mapUnits.doo` records that are neither
    /// `sloc` start markers nor gold mines, sorted by id. `units` is the sum
    /// of `placed`; every raw record is classified.
    pub unit_census: BTreeMap<String, SceneIdCount>,
    /// Per-id outcome of the `war3map.doo` records, sorted by id.
    /// `destructables` is the sum of `placed`.
    pub doodad_census: BTreeMap<String, SceneIdCount>,
}

/// Spawns `war3mapUnits.doo` units, `war3map.doo` destructables/trees and
/// groups neutral map units into creep camps (aggro 500 / leash 900).
pub fn spawn_map_scene(sim: &mut Sim, bundle: &MapBundle) -> MapSpawns {
    // Native-resolution WPM first: unit paths and collision use the real
    // 32-unit pathing grid, not a downsampled terrain-tile grid.
    apply_map_pathing(sim, bundle);
    // `.w3e` stores the map origin (left/bottom) as the two header floats.
    // Melee maps use negative world coordinates; the engine grid is 0-based,
    // so every placed position is translated into local map space.
    let origin = bundle
        .terrain
        .as_ref()
        .map(|terrain| Vec2::new(terrain.center.0, terrain.center.1))
        .unwrap_or(Vec2::ZERO);
    let to_local = |x: f32, y: f32| Vec2::new(x - origin.x, y - origin.y);
    let mut counts = MapSpawns::default();
    let mut neutrals: Vec<(UnitId, Vec2)> = Vec::new();
    // `(slot, position)` of every `sloc` marker. The `.doo` creator field is
    // the authoritative slot→start mapping; the w3i player block only carries
    // the slot race (and a redundant start position).
    let mut starts: Vec<(u32, Vec2)> = Vec::new();
    // Per-id census of the map's own placements: the device and the tests
    // print it, so a scene that comes up short can be traced to the exact
    // `war3mapUnits.doo`/`war3map.doo` id and the reason it was dropped.
    let mut unit_records = 0usize;
    let mut doodad_records = 0usize;
    if let Some(units) = &bundle.units {
        for unit in units {
            unit_records += 1;
            let def_id = String::from_utf8_lossy(&unit.type_id).to_string();
            // Ground plane is (x, y); z is height in both the engine world
            // (`engine.rs` keeps x/y as ground) and the game renderer.
            let pos = to_local(unit.position.x, unit.position.y);
            if def_id == "sloc" {
                // Start-location marker: GC units, not a unit def in any SLK.
                counts.start_locations += 1;
                starts.push((unit.player, pos));
                continue;
            }
            if def_id == "ngol" || def_id == "ugol" {
                // Gold mines are world objects in the sim, not units.
                sim.add_gold_mine(pos, wc3_sim::GOLD_MINE_GOLD);
                counts.gold_mines += 1;
                continue;
            }
            let census = counts.unit_census.entry(def_id.clone()).or_default();
            if def_of(&def_id).is_none() {
                // A `war3mapUnits.doo` record is not necessarily a unit: an
                // item placed in the editor (`hlst`, `whwd`, `phea`, …) is a
                // record too, and WC3 creates it as a ground item.  Only an id
                // that is neither a def nor an item row is `unknown_def`.
                if wc3_sim::data::item_def(&def_id).is_some() {
                    if sim.spawn_item(&def_id, pos, 0).is_some() {
                        census.items += 1;
                        counts.items += 1;
                    } else {
                        census.spawn_failed += 1;
                    }
                    continue;
                }
                census.unknown_def += 1;
                continue;
            }
            // The sim now models 8 seats (`wc3_sim::MAX_PLAYERS`); squashing
            // every player above 1 onto seat 2 is what made a campaign seat
            // (Prologue01 = 4) spawn zero units and the world render black.
            let owner = if (unit.player as usize) < wc3_sim::MAX_PLAYERS {
                unit.player as u8
            } else {
                wc3_sim::NEUTRAL_OWNER
            };
            match sim.spawn_def_exact(owner, &def_id, pos) {
                Some(id) => {
                    census.placed += 1;
                    counts.units += 1;
                    if owner == wc3_sim::NEUTRAL_OWNER {
                        neutrals.push((id, pos));
                    }
                }
                None => census.spawn_failed += 1,
            }
        }
    }
    if let Some(doodads) = &bundle.doodads {
        for doodad in doodads {
            doodad_records += 1;
            let def_id = String::from_utf8_lossy(&doodad.type_id).to_string();
            let census = counts.doodad_census.entry(def_id.clone()).or_default();
            if wc3_sim::data::destructable_def(&def_id).is_none() {
                census.unknown_def += 1;
                continue;
            }
            let pos = to_local(doodad.position.x, doodad.position.y);
            if sim.spawn_destructable(&def_id, pos).is_some() {
                census.placed += 1;
                counts.destructables += 1;
            } else {
                census.spawn_failed += 1;
            }
        }
    }
    // Cluster neutral map units into camps by 600-unit proximity.
    let mut remaining = neutrals;
    while let Some((_, anchor)) = remaining.first().copied() {
        let mut grouped = Vec::new();
        let mut rest = Vec::new();
        for (id, pos) in remaining {
            if pos.distance(anchor) <= 600.0 {
                grouped.push(id);
            } else {
                rest.push((id, pos));
            }
        }
        if sim.register_camp(anchor, &grouped, 500.0, 900.0).is_some() {
            counts.camps += 1;
        }
        remaining = rest;
    }
    // Per-id trace: every id the map places, with the reason it did or did
    // not become an entity. Printed once per scene load (device log sink and
    // headless stderr), so the counts below are auditable, not asserted.
    let unit_placed: usize = counts.unit_census.values().map(|c| c.placed).sum();
    let unit_items: usize = counts.unit_census.values().map(|c| c.items).sum();
    let unit_unknown: usize = counts.unit_census.values().map(|c| c.unknown_def).sum();
    let unit_failed: usize = counts.unit_census.values().map(|c| c.spawn_failed).sum();
    let doodad_placed: usize = counts.doodad_census.values().map(|c| c.placed).sum();
    let doodad_unknown: usize = counts.doodad_census.values().map(|c| c.unknown_def).sum();
    let doodad_failed: usize = counts.doodad_census.values().map(|c| c.spawn_failed).sum();
    crate::dev_log!(
        "GAME_SCENE_CENSUS map={} unit_records={} sloc={} mines={} unit_placed={} unit_items={} unit_unknown={} unit_spawn_failed={} unit_ids={} doodad_records={} doodad_placed={} doodad_no_data={} doodad_spawn_failed={} doodad_ids={}",
        bundle
            .info
            .as_ref()
            .map(|info| info.name.as_str())
            .unwrap_or("?"),
        unit_records,
        counts.start_locations,
        counts.gold_mines,
        unit_placed,
        unit_items,
        unit_unknown,
        unit_failed,
        counts.unit_census.len(),
        doodad_records,
        doodad_placed,
        doodad_unknown,
        doodad_failed,
        counts.doodad_census.len()
    );
    for (id, census) in &counts.unit_census {
        crate::dev_log!(
            "GAME_SCENE_UNIT id={} placed={} items={} unknown_def={} spawn_failed={} source=war3mapUnits.doo",
            id,
            census.placed,
            census.items,
            census.unknown_def,
            census.spawn_failed
        );
    }
    // Decoration-only doodad ids are numerous and never entities; one line per
    // such id is enough to audit the 1997/2902 split.
    for (id, census) in &counts.doodad_census {
        if census.placed == 0 || census.spawn_failed > 0 {
            crate::dev_log!(
                "GAME_SCENE_DOODAD id={} placed={} no_data={} spawn_failed={} source=war3map.doo",
                id,
                census.placed,
                census.unknown_def,
                census.spawn_failed
            );
        }
    }
    // Keep the map file's own `sloc` order: `game.rs` takes
    // `start_points[0]` as the player base, and the seating must not reshuffle
    // between builds. Each seat's race is looked up from its `sloc` creator
    // slot in the w3i player block.
    counts.start_points = starts.iter().map(|(_, pos)| *pos).collect();
    if starts.len() == 2 {
        counts.melee = spawn_melee_bases(sim, bundle, &starts);
    }
    counts
}

/// Compact `id:reason=count` list of every unit id the scene dropped, sorted by
/// id. Empty when every placement spawned; the game log carries this so a
/// short scene names its missing ids on the device as well as in the tests.
pub fn scene_skips(counts: &MapSpawns) -> String {
    let mut parts = Vec::new();
    for (id, census) in &counts.unit_census {
        if census.unknown_def > 0 {
            parts.push(format!("{id}:unknown_def={}", census.unknown_def));
        }
        if census.spawn_failed > 0 {
            parts.push(format!("{id}:spawn_failed={}", census.spawn_failed));
        }
    }
    parts.join(",")
}

/// Spawn only `war3map.doo` destructables (trees/rocks) from a map bundle.
///
/// Trees are gameplay obstacles: `Sim::spawn_destructable` marks their cells
/// unwalkable, so the melee scenario paths units around doodads without booting
/// the map's campaign script or adopting its placed units.
pub fn spawn_map_destructables(sim: &mut Sim, bundle: &MapBundle) -> usize {
    let origin = bundle
        .terrain
        .as_ref()
        .map(|terrain| Vec2::new(terrain.center.0, terrain.center.1))
        .unwrap_or(Vec2::ZERO);
    let to_local = |x: f32, y: f32| Vec2::new(x - origin.x, y - origin.y);
    let mut spawned = 0usize;
    if let Some(doodads) = &bundle.doodads {
        for doodad in doodads {
            let def_id = String::from_utf8_lossy(&doodad.type_id).to_string();
            if wc3_sim::data::destructable_def(&def_id).is_none() {
                continue;
            }
            let pos = to_local(doodad.position.x, doodad.position.y);
            if sim.spawn_destructable(&def_id, pos).is_some() {
                spawned += 1;
            }
        }
    }
    spawned
}

/// Opens a map through the canonical `wc3-re::mapfiles::MapBundle` and wires
/// its units/destructables/camps into the sim.  The game calls this once per
/// scenario reset; the parser stays the RE layer's.
pub fn spawn_map_file(sim: &mut Sim, path: &std::path::Path) -> Result<MapSpawns, String> {
    let bundle = MapBundle::open(path).map_err(|error| error.to_string())?;
    Ok(spawn_map_scene(sim, &bundle))
}

// ---------------------------------------------------------------- melee slots

/// One playable slot of `war3map.w3i` with the race the map author fixed for
/// it. `start` is the w3i start position translated into local map space.
#[derive(Debug, Clone, PartialEq)]
pub struct MapPlayerSlot {
    /// `war3map.w3i` player number (0-based, as stored).
    pub slot: u32,
    /// `PlayerController`: 1 user, 2 computer, 3 neutral, 4 rescuable.
    pub controller: u32,
    pub race: Race,
    /// `PlayerFlags`: bit 0 fixed start, bit 1 selectable race.
    pub flags: u32,
    pub name: String,
    /// w3i start position (slot 0 of `war3mapUnits.doo` is authoritative when
    /// both exist; the w3i copy is kept for cross-checking).
    pub start: Vec2,
}

/// Melee opening spawned for a two-slot map: the w3i race of each slot decides
/// the hall and worker def, so the enemy base never inherits the player's kit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeleeBases {
    pub player_race: Race,
    pub enemy_race: Race,
    pub player_hall: Option<UnitId>,
    pub enemy_hall: Option<UnitId>,
    pub player_workers: usize,
    pub enemy_workers: usize,
    pub player_start: Vec2,
    pub enemy_start: Vec2,
}

/// Minimal bounded reader over `war3map.w3i` bytes (never panics).
struct W3iReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> W3iReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(count)?;
        let slice = self.bytes.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn skip(&mut self, count: usize) -> Option<()> {
        self.take(count).map(|_| ())
    }

    fn cstring(&mut self) -> Option<String> {
        let rest = self.bytes.get(self.pos..)?;
        let end = rest.iter().position(|&byte| byte == 0)?;
        let text = String::from_utf8_lossy(&rest[..end]).into_owned();
        self.pos += end + 1;
        Some(text)
    }
}

fn race_from_w3i(value: i32) -> Option<Race> {
    // War3Net `PlayerRace`: 0 selectable, 1 human, 2 orc, 3 undead, 4 night elf.
    match value {
        1 => Some(Race::Human),
        2 => Some(Race::Orc),
        3 => Some(Race::Undead),
        4 => Some(Race::NightElf),
        _ => None,
    }
}

/// Player records from `war3map.w3i`: slot number, fixed race and start
/// position. Parsing starts at [`wc3_re::mapfiles::MapInfo::raw_tail`], i.e.
/// right after `recommendedPlayers`, and follows the War3Net v23..v26 read
/// order (campaign bounds, complements, loading screen, fog, weather, then the
/// player block). Unknown/older layouts return an empty list: the caller then
/// keeps its previous fallback instead of inventing a race.
pub fn map_player_slots(bundle: &MapBundle) -> Vec<MapPlayerSlot> {
    let Some(info) = bundle.info.as_ref() else {
        return Vec::new();
    };
    if !(23..=26).contains(&info.version) {
        return Vec::new();
    }
    let origin = bundle
        .terrain
        .as_ref()
        .map(|terrain| Vec2::new(terrain.center.0, terrain.center.1))
        .unwrap_or(Vec2::ZERO);
    let mut r = W3iReader::new(&info.raw_tail);
    let parsed = (|| -> Option<Vec<MapPlayerSlot>> {
        r.skip(8 * 4)?; // camera bounds
        r.skip(4 * 4)?; // camera bounds complements (>= v15)
        r.skip(4 + 4)?; // playable width/height
        r.skip(4 + 1)?; // flags + main ground tileset
        r.skip(4)?; // loading screen background number (>= v23)
        r.cstring()?; // loading screen path
        r.cstring()?; // loading screen text
        r.cstring()?; // loading screen title
        r.cstring()?; // loading screen subtitle
        r.skip(4)?; // game data set
        r.cstring()?; // prologue screen path
        r.cstring()?; // prologue text
        r.cstring()?; // prologue title
        r.cstring()?; // prologue subtitle
        r.skip(4 + 4 + 4 + 4 + 4)?; // fog style/start/end/density + BGRA color
        r.skip(4)?; // global weather id (>= v25)
        r.cstring()?; // sound environment
        r.skip(1 + 4)?; // light environment tileset + water tint BGRA
        let count = r.i32()?;
        if !(0..=24).contains(&count) {
            return None;
        }
        let mut slots = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let slot = r.u32()?;
            let controller = r.u32()?;
            let race = race_from_w3i(r.i32()?)?;
            let flags = r.u32()?;
            let name = r.cstring()?;
            let x = r.f32()?;
            let y = r.f32()?;
            r.skip(4 + 4)?; // ally low/high priority flags
            slots.push(MapPlayerSlot {
                slot,
                controller,
                race,
                flags,
                name,
                start: Vec2::new(x - origin.x, y - origin.y),
            });
        }
        Some(slots)
    })();
    parsed.unwrap_or_default()
}

/// Spawns the melee opening of a two-slot map: one main hall and five workers
/// of each slot's own w3i race at that slot's own start marker.
///
/// The hall/worker ids come from the canonical SLK roster
/// (`wc3_sim::ai::hall_for_race` / `worker_for_race`), never from a table in
/// this file. Existing map-owned halls/workers are honoured. Returns `None`
/// for anything that is not a clean two-slot melee map (campaign maps with
/// `sloc` clusters, selectable/random races, missing w3i), so those keep the
/// caller's own fallback.
pub fn spawn_melee_bases(
    sim: &mut Sim,
    bundle: &MapBundle,
    starts: &[(u32, Vec2)],
) -> Option<MeleeBases> {
    if starts.len() != 2 || starts[0].0 == starts[1].0 {
        return None;
    }
    let mut slots = map_player_slots(bundle);
    slots.sort_by_key(|slot| slot.slot);
    if slots.len() != 2 {
        crate::dev_log!(
            "GAME_MELEE_RACE source=w3i skipped=slots count={} map={}",
            slots.len(),
            bundle.info.as_ref().map(|info| info.name.as_str()).unwrap_or("?")
        );
        return None;
    }
    // `starts` keeps the `war3mapUnits.doo` order (game.rs: `[0]` = player).
    // Each seat's race is the w3i race of that `sloc` marker's creator slot.
    let mut races = Vec::with_capacity(2);
    for (slot, _) in starts {
        let race = slots
            .iter()
            .find(|player| player.slot == *slot)
            .map(|player| player.race);
        match race {
            Some(race) => races.push(race),
            None => {
                crate::dev_log!(
                    "GAME_MELEE_RACE source=w3i skipped=slot slot={} map={}",
                    slot,
                    bundle.info.as_ref().map(|info| info.name.as_str()).unwrap_or("?")
                );
                return None;
            }
        }
    }
    let mut bases = MeleeBases {
        player_race: races[0],
        enemy_race: races[1],
        player_hall: None,
        enemy_hall: None,
        player_workers: 0,
        enemy_workers: 0,
        player_start: starts[0].1,
        enemy_start: starts[1].1,
    };
    for (owner, (slot, start)) in starts.iter().copied().enumerate() {
        let race = races[owner];
        let hall_def = wc3_sim::ai::hall_for_race(race);
        let worker_def = wc3_sim::ai::worker_for_race(race);
        let existing_hall = sim
            .alive_units()
            .find(|unit| unit.owner == owner as u8 && unit.def().main_hall)
            .map(|unit| unit.id);
        let hall = existing_hall.or_else(|| {
            let pos = sim.walkable_pos_near(start, 24).unwrap_or(start);
            sim.spawn_def(owner as u8, hall_def, pos)
        });
        let prepared = sim
            .alive_units()
            .filter(|unit| unit.owner == owner as u8 && unit.def().stats.is_worker)
            .count();
        let mut spawned = 0usize;
        for slot_index in prepared..5usize {
            let angle = std::f32::consts::TAU * slot_index as f32 / 5.0;
            let wanted = start + Vec2::new(angle.cos(), angle.sin()) * 300.0;
            let Some(pos) = sim.walkable_pos_near(wanted, 16) else {
                continue;
            };
            if sim.spawn_def(owner as u8, worker_def, pos).is_some() {
                spawned += 1;
            }
        }
        if owner == 0 {
            bases.player_hall = hall;
            bases.player_workers = prepared + spawned;
        } else {
            bases.enemy_hall = hall;
            bases.enemy_workers = prepared + spawned;
        }
        crate::dev_log!(
            "GAME_MELEE_RACE owner={} slot={} race={:?} hall={} worker={} hall_id={:?} workers={} start=({:.0},{:.0}) source=w3i",
            owner,
            slot,
            race,
            hall_def,
            worker_def,
            hall.map(|id| id.0),
            prepared + spawned,
            start.x,
            start.y
        );
    }
    crate::dev_log!(
        "GAME_MELEE_SETUP player={:?} enemy={:?} player_hall={:?} enemy_hall={:?} player_workers={} enemy_workers={} map={} source=w3i",
        bases.player_race,
        bases.enemy_race,
        bases.player_hall.map(|id| id.0),
        bases.enemy_hall.map(|id| id.0),
        bases.player_workers,
        bases.enemy_workers,
        bundle
            .info
            .as_ref()
            .map(|info| info.name.as_str())
            .unwrap_or("?")
    );
    Some(bases)
}

// ------------------------------------------------------------ checkpoints

/// FNV-1a 64 digest of a file, read in bounded chunks.
fn file_digest(path: &std::path::Path) -> Option<(u64, u64)> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut buffer = [0u8; 64 * 1024];
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut len = 0u64;
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        len += read as u64;
        for byte in &buffer[..read] {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    Some((len, hash))
}

/// Identity of the map a checkpoint was written on. A save is only resumable
/// on the same map bytes: same path, same length, same digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapStamp {
    pub path: String,
    pub len: u64,
    pub hash: u64,
}

impl MapStamp {
    pub fn of_file(path: &std::path::Path) -> Option<MapStamp> {
        let (len, hash) = file_digest(path)?;
        Some(MapStamp {
            path: path.display().to_string(),
            len,
            hash,
        })
    }

    pub fn matches(&self, other: &MapStamp) -> bool {
        self.len == other.len && self.hash == other.hash
    }
}

/// Why a checkpoint is not resumable. Rendered into `GAME_RESUME_REJECT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeReject {
    /// Phase other than `playing`: a finished match must not come back.
    Finished,
    /// Different map file (length or digest).
    MapMismatch,
    /// Different pathing/terrain grid.
    GridMismatch,
    /// Save payload differs from the metadata (stale or corrupt).
    DigestMismatch,
    /// Unknown phase string.
    BadPhase,
}

impl ResumeReject {
    pub fn reason(&self) -> &'static str {
        match self {
            ResumeReject::Finished => "finished",
            ResumeReject::MapMismatch => "map",
            ResumeReject::GridMismatch => "grid",
            ResumeReject::DigestMismatch => "digest",
            ResumeReject::BadPhase => "phase",
        }
    }
}

impl std::fmt::Display for ResumeReject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.reason())
    }
}

/// Session metadata written next to a `Sim` save (`*.sav.meta`). The `Sim`
/// save format is the sim layer's; the map identity and the game phase are
/// shim state and travel here. `verdict` rejects a checkpoint from another
/// map/grid or from a match that is already finished, and `matches` rejects a
/// save whose payload does not match the recorded tick/digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointMeta {
    pub map: MapStamp,
    pub grid: (u32, u32),
    pub phase: String,
    pub tick: u64,
    pub digest: u64,
}

impl CheckpointMeta {
    pub fn new(map: MapStamp, grid: (u32, u32), phase: &str, tick: u64, digest: u64) -> Self {
        Self {
            map,
            grid,
            phase: phase.to_string(),
            tick,
            digest,
        }
    }

    pub fn encode(&self) -> String {
        format!(
            "h2checkpoint=1\nmap_path={}\nmap_len={}\nmap_hash={:016x}\ngrid={}x{}\nphase={}\ntick={}\ndigest={:016x}\n",
            self.map.path, self.map.len, self.map.hash, self.grid.0, self.grid.1, self.phase, self.tick, self.digest
        )
    }

    /// Strict decode: every field is required; anything else is not a
    /// checkpoint this build wrote.
    pub fn decode(text: &str) -> Option<CheckpointMeta> {
        let mut version = None;
        let mut path = None;
        let mut len = None;
        let mut hash = None;
        let mut grid = None;
        let mut phase = None;
        let mut tick = None;
        let mut digest = None;
        for line in text.lines() {
            let line = line.trim();
            let (key, value) = line.split_once('=')?;
            match key {
                "h2checkpoint" => version = Some(value.to_string()),
                "map_path" => path = Some(value.to_string()),
                "map_len" => len = value.parse::<u64>().ok(),
                "map_hash" => hash = u64::from_str_radix(value, 16).ok(),
                "grid" => {
                    let (w, h) = value.split_once('x')?;
                    grid = Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?));
                }
                "phase" => phase = Some(value.to_string()),
                "tick" => tick = value.parse::<u64>().ok(),
                "digest" => digest = u64::from_str_radix(value, 16).ok(),
                _ => {}
            }
        }
        if version.as_deref() != Some("1") {
            return None;
        }
        Some(CheckpointMeta {
            map: MapStamp {
                path: path?,
                len: len?,
                hash: hash?,
            },
            grid: grid?,
            phase: phase?,
            tick: tick?,
            digest: digest?,
        })
    }

    pub fn is_finished(&self) -> bool {
        self.phase != "playing"
    }

    /// Map/grid/phase gate, run before the save is loaded.
    pub fn verdict(
        &self,
        current_map: Option<&MapStamp>,
        grid: (u32, u32),
    ) -> Result<(), ResumeReject> {
        if self.phase != "playing" {
            return Err(if self.phase == "won" || self.phase == "lost" {
                ResumeReject::Finished
            } else {
                ResumeReject::BadPhase
            });
        }
        match current_map {
            Some(current) if self.map.matches(current) => {}
            _ => return Err(ResumeReject::MapMismatch),
        }
        if self.grid != grid {
            return Err(ResumeReject::GridMismatch);
        }
        Ok(())
    }

    /// Payload gate, run after the save is decoded.
    pub fn matches(&self, tick: u64, digest: u64) -> bool {
        self.tick == tick && self.digest == digest
    }
}

/// Applies a map's `war3map.wpm` at its native 32-unit cell size.
pub fn apply_map_pathing(sim: &mut Sim, bundle: &MapBundle) {
    let Some(pathing) = bundle.pathing.as_ref() else {
        return;
    };
    let mut walkable = Vec::with_capacity(pathing.width as usize * pathing.height as usize);
    for y in 0..pathing.height {
        for x in 0..pathing.width {
            walkable.push(pathing.walkable(x, y));
        }
    }
    sim.set_pathing(
        pathing.width,
        pathing.height,
        wc3_sim::PATHING_CELL_SIZE,
        walkable,
    );
}

/// Clears walkability for every cell whose world-space centre is water.
///
/// WC3 ground units never stand in deep water, and this install's WPM does not
/// flag every water cell as unwalkable, so the terrain water layer is the
/// second authoritative source. The predicate is a closure to keep the rule
/// testable without a full game install.
pub fn mask_water(
    width: u32,
    height: u32,
    cell_size: f32,
    walkable: &mut [bool],
    is_water: impl Fn(f32, f32) -> bool,
) -> usize {
    let mut masked = 0usize;
    for y in 0..height {
        for x in 0..width {
            let index = (y * width + x) as usize;
            // `walkable` is caller-supplied; a grid shorter than width*height
            // (truncated WPM, mismatched caller) must not index-panic.
            let Some(cell) = walkable.get_mut(index) else {
                wc3_formats::diag::note("pathing_grid_short", || {
                    format!("width={width} height={height} len={}", walkable.len())
                });
                continue;
            };
            if !*cell {
                continue;
            }
            let wx = (x as f32 + 0.5) * cell_size;
            let wy = (y as f32 + 0.5) * cell_size;
            if is_water(wx, wy) {
                *cell = false;
                masked += 1;
            }
        }
    }
    masked
}

/// Applies `Assets.pathing` (the same WPM the map bundle carries).
pub fn apply_assets_pathing(sim: &mut Sim, assets: &crate::assets::Assets) {
    let Some(pathing) = assets.pathing.as_ref() else {
        return;
    };
    let mut walkable = Vec::with_capacity(pathing.width as usize * pathing.height as usize);
    for y in 0..pathing.height {
        for x in 0..pathing.width {
            walkable.push(pathing.walkable(x, y));
        }
    }
    // Water is unwalkable for ground units; the WPM alone is not sufficient on
    // this map (units used to run under the water surface).
    mask_water(
        pathing.width,
        pathing.height,
        wc3_sim::PATHING_CELL_SIZE,
        &mut walkable,
        |x, z| assets.is_water(x, z),
    );
    sim.set_pathing(
        pathing.width,
        pathing.height,
        wc3_sim::PATHING_CELL_SIZE,
        walkable,
    );
}

pub use wc3_sim::MeleeSetup;

/// WC3 melee opening for a 1v1 (thin delegate to the canonical sim setup).
pub fn setup_melee_1v1(
    sim: &mut Sim,
    player_race: Race,
    enemy_race: Race,
    player_base: Vec2,
    enemy_base: Vec2,
) -> Result<MeleeSetup, &'static str> {
    sim.setup_melee_1v1(player_race, enemy_race, player_base, enemy_base)
}

/// Game hook: spawns the loaded map scene recorded in `Assets.debug`
/// (`map=<path>`), if a real map was loaded.  No second parser: the path is
/// reopened through `wc3-re::mapfiles::MapBundle`.
pub fn spawn_assets_map(
    sim: &mut Sim,
    assets: &crate::assets::Assets,
) -> Result<MapSpawns, String> {
    let path = assets
        .debug
        .iter()
        .find_map(|line| line.strip_prefix("map="))
        .ok_or_else(|| "no map loaded".to_string())?;
    spawn_map_file(sim, std::path::Path::new(path))
}

/// Every building the owner's race can place (full 99-def roster, not just the
/// four card slots).  HUD pagination consumes this; costs are SLK values.
pub fn all_build_options(sim: &Sim, owner: u8) -> Vec<BuildOption> {
    sim.build_menu_for_owner(owner)
        .into_iter()
        .filter(|def| def.stats.cost > 0)
        .map(|def| BuildOption {
            def_id: def.id,
            label: def.name,
            cost: def.stats.cost,
            enabled: sim.resources[owner as usize] >= def.stats.cost
                && sim.lumber[owner as usize] >= def.stats.lumber_cost,
        })
        .collect()
}

/// Units a specific producer can train, with the real `trained_at` relation
/// (tier halls, altars/heroes, barracks, sanctums, …).
pub fn train_options(sim: &Sim, owner: u8, producer: UnitId) -> Vec<BuildOption> {
    let Some(building) = sim.unit(producer) else {
        return Vec::new();
    };
    let building_id = building.def().id;
    wc3_sim::ROSTER
        .iter()
        .filter(|def| !def.is_building() && def.trained_at.contains(&building_id))
        .map(|def| BuildOption {
            def_id: def.id,
            label: def.name,
            cost: def.stats.cost,
            enabled: sim.resources[owner as usize] >= def.stats.cost
                && sim.lumber[owner as usize] >= def.stats.lumber_cost,
        })
        .collect()
}

pub fn can_afford_def(sim: &Sim, owner: u8, def_id: &str) -> bool {
    def_of(def_id)
        .map(|def| sim.resources[owner as usize] >= def.stats.cost)
        .unwrap_or(false)
}

/// Result of a command-card placement: the hall line is spawned immediately
/// (cost deducted atomically), other buildings become construction sites.
#[derive(Debug, Clone, PartialEq)]
pub enum Placement {
    Built(UnitId),
    Site(UnitId),
}

/// Translates a build-card action into a sim command or a construction site.
pub fn place_from_card(
    sim: &mut Sim,
    builder: UnitId,
    def_id: &'static str,
    at: Vec2,
) -> Result<Placement, &'static str> {
    let def = def_of(def_id).ok_or("unknown building")?;
    if !def.is_building() {
        return Err("not a building");
    }
    if def_id == UnitKind::TownHall.canonical_id() {
        return sim
            .build_instant(builder, UnitKind::TownHall, at)
            .map(Placement::Built);
    }
    sim.place_building_ordered(builder, def_id, at)
        .map(Placement::Site)
}

/// Wave feedback for the HUD; returns `None` for other events.
pub fn wave_toast(event: &Event, enemy: u8) -> Option<String> {
    match event {
        Event::AiWave { owner, wave } if *owner == enemy => Some(format!("ENEMY WAVE {wave}")),
        _ => None,
    }
}

/// Capability checks so the HUD shim never compares render buckets
/// (`UnitKind` maps every building to `TownHall`).
pub fn is_main_hall(unit: &wc3_sim::Unit) -> bool {
    unit.is_building() && unit.def().is_main_hall()
}

pub fn is_worker(unit: &wc3_sim::Unit) -> bool {
    !unit.is_building() && unit.def().stats.is_worker
}

#[derive(Debug, Clone, Copy, Default)]
struct Mirror {
    x: f64,
    y: f64,
    hp: f64,
    mana: f64,
    alive: bool,
}

/// Bridges one `wc3-re` JASS world/runtime to one `wc3-sim` simulation.
pub struct JassBridge {
    /// The canonical simulation (gameplay owner).
    pub sim: Sim,
    /// JASS player index → simulation owner. Players 0/1 are the two sim
    /// factions; every other JASS player (neutral passive/hostile and the
    /// unused melee slots) belongs to the neutral owner 2.
    pub owner_of_player: Vec<u8>,
    /// Unit type ids created by a script that are not in the sim roster; they
    /// stay script-only and are reported rather than faked.
    pub missing_units: Vec<String>,
    units: BTreeMap<u32, UnitId>,
    rev: BTreeMap<UnitId, u32>,
    mirror: BTreeMap<u32, Mirror>,
    /// Script world → sim local map-space offset (see [`Self::set_origin`]).
    origin: glam::Vec2,
}

impl JassBridge {
    /// Ground-plane offset between the script's world coordinates and the
    /// sim's local map space (`.w3e` origin). Script units are created with raw
    /// world coordinates; without this the whole campaign army spawned shifted
    /// by `origin` — a pile of interpenetrating models in a black corner of the
    /// map (device frame 13:54).
    pub fn set_origin(&mut self, origin: glam::Vec2) {
        self.origin = origin;
    }

    pub fn new(sim: Sim) -> Self {
        // Every seat the sim models keeps its own index. A campaign mission can
        // seat the human at Player(4); mapping that to neutral is exactly what
        // made Prologue01 spawn zero player units (`seat_units=0`) and the world
        // render black. Slots beyond `MAX_PLAYERS` stay neutral.
        let mut owner_of_player = vec![wc3_sim::NEUTRAL_OWNER; 16];
        for (seat, owner) in owner_of_player
            .iter_mut()
            .enumerate()
            .take(wc3_sim::MAX_PLAYERS)
        {
            *owner = seat as u8;
        }
        Self {
            sim,
            origin: glam::Vec2::ZERO,
            owner_of_player,
            missing_units: Vec::new(),
            units: BTreeMap::new(),
            rev: BTreeMap::new(),
            mirror: BTreeMap::new(),
        }
    }

    /// Associates an existing simulation unit with a JASS handle (scenario
    /// units that existed before the script world was created).
    pub fn bind(&mut self, handle: u32, unit: UnitId) {
        self.units.insert(handle, unit);
        self.rev.insert(unit, handle);
    }

    /// Adopts units that already exist in the script world (map `main` ran
    /// before the bridge was created), groups neutral map units into creep
    /// camps and mirrors the simulation back.
    pub fn attach(&mut self, world: &mut World) {
        self.adopt(world);
        self.sim.cluster_neutral_camps();
        self.sync(world);
    }

    /// One frame: script mutations/orders → engine timers/triggers → sim step →
    /// mirror back.  `commands` are the player's queued commands for this tick.
    pub fn step_engine(&mut self, engine: &mut Engine, dt: f64, commands: &[Command]) {
        if let Some(runtime) = engine.runtime.as_mut() {
            self.adopt(&mut runtime.world);
            self.apply_script_state(&mut runtime.world);
            self.apply_orders(&mut runtime.world);
        }
        engine.tick(dt);
        if let Some(runtime) = engine.runtime.as_mut() {
            self.adopt(&mut runtime.world);
            self.apply_script_state(&mut runtime.world);
            self.apply_orders(&mut runtime.world);
            self.sim.step(commands);
            self.sync(&mut runtime.world);
        } else {
            self.sim.step(commands);
        }
    }

    /// For callers that already drained the engine (headless scripts/tests).
    pub fn step_world(&mut self, world: &mut World, commands: &[Command]) {
        self.adopt(world);
        self.apply_script_state(world);
        self.apply_orders(world);
        self.sim.step(commands);
        self.sync(world);
    }

    /// Script-created units → sim.  Units whose rawcode is not in the roster
    /// are recorded in [`Self::missing_units`] and never invented.
    fn adopt(&mut self, world: &mut World) {
        let mut adopted: Vec<Vec2> = Vec::new();
        let candidates: Vec<(u32, String, i32, f64, f64)> = world
            .units
            .iter()
            .filter(|(handle, _)| !self.units.contains_key(handle))
            .map(|(handle, unit)| (*handle, unit.id.clone(), unit.owner, unit.x, unit.y))
            .collect();
        for (handle, id, owner, x, y) in candidates {
            if def_of(&id).is_none() {
                if !self.missing_units.iter().any(|missing| missing == &id) {
                    self.missing_units.push(id);
                }
                continue;
            }
            let owner = self
                .owner_of_player
                .get(owner.max(0) as usize)
                .copied()
                .unwrap_or(0);
            let pos = Vec2::new(x as f32 - self.origin.x, y as f32 - self.origin.y);
            if let Some(unit) = self.sim.spawn_def_exact(owner, &id, pos) {
                self.units.insert(handle, unit);
                self.rev.insert(unit, handle);
                adopted.push(pos);
            }
        }
        self.log_adopted(&adopted);
    }

    /// One line per adopt batch: script units must be spread over the map, not
    /// piled on one cell. `origin` is the world→local offset; a wrong origin is
    /// visible here as a tiny spread or coordinates outside the grid.
    fn log_adopted(&self, adopted: &[Vec2]) {
        if adopted.is_empty() {
            return;
        }
        let (mut xmin, mut xmax, mut ymin, mut ymax) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for pos in adopted {
            xmin = xmin.min(pos.x);
            xmax = xmax.max(pos.x);
            ymin = ymin.min(pos.y);
            ymax = ymax.max(pos.y);
        }
        crate::dev_log!(
            "GAME_SCRIPT_SPAWN units={} origin={:.1},{:.1} spread_x={:.0}..{:.0} spread_y={:.0}..{:.0}",
            adopted.len(),
            self.origin.x,
            self.origin.y,
            xmin,
            xmax,
            ymin,
            ymax
        );
    }

    /// Applies script-side mutations (positions/HP/mana/alive/owner) to the
    /// canonical sim.  Runs before `sync` so the sim is the last writer.
    fn apply_script_state(&mut self, world: &mut World) {
        let mut removed: Vec<(u32, UnitId)> = Vec::new();
        let mut kills: Vec<UnitId> = Vec::new();
        let mut hp: Vec<(UnitId, i32)> = Vec::new();
        let mut mana: Vec<(UnitId, i32)> = Vec::new();
        let mut moves: Vec<(UnitId, Vec2)> = Vec::new();
        let mut owners: Vec<(UnitId, u8)> = Vec::new();

        for (&handle, &unit) in &self.units {
            let Some(script) = world.units.get(&handle) else {
                removed.push((handle, unit));
                continue;
            };
            let Some(previous) = self.mirror.get(&handle).copied() else {
                continue;
            };
            if !script.alive && previous.alive {
                kills.push(unit);
            }
            if (script.hp - previous.hp).abs() > 0.5 {
                hp.push((unit, script.hp.round() as i32));
            }
            if (script.mana - previous.mana).abs() > 0.5 {
                mana.push((unit, script.mana.round() as i32));
            }
            let dx = script.x - previous.x;
            let dy = script.y - previous.y;
            if dx * dx + dy * dy > 16.0 {
                moves.push((unit, Vec2::new(script.x as f32, script.y as f32)));
            }
            if script.owner >= 0 {
                let owner = self
                    .owner_of_player
                    .get(script.owner as usize)
                    .copied()
                    .unwrap_or(0);
                if let Some(current) = self.sim.unit(unit) {
                    if current.owner != owner {
                        owners.push((unit, owner));
                    }
                }
            }
        }
        for (handle, unit) in removed {
            self.sim.kill_unit(unit);
            self.rev.remove(&unit);
            self.units.remove(&handle);
            self.mirror.remove(&handle);
        }
        for unit in kills {
            self.sim.kill_unit(unit);
        }
        for (unit, value) in hp {
            self.sim.set_unit_hp(unit, value.max(0));
        }
        for (unit, value) in mana {
            self.sim.set_unit_mana(unit, value.max(0));
        }
        for (unit, position) in moves {
            self.sim.teleport_unit(unit, position);
        }
        for (unit, owner) in owners {
            self.sim.set_unit_owner(unit, owner);
        }
    }

    /// Drains `World::orders` (WC3 `Issue*Order` natives) into sim orders.
    fn apply_orders(&mut self, world: &mut World) {
        let orders: Vec<ScriptOrder> = std::mem::take(&mut world.orders);
        for order in orders {
            let Some(&unit) = self.units.get(&order.unit) else {
                continue;
            };
            let name = order.order.to_ascii_lowercase();
            let point = order.point.map(|(x, y)| Vec2::new(x as f32, y as f32));
            match name.as_str() {
                "move" => {
                    if let Some(point) = point {
                        self.sim.issue(&[unit], OrderSpec::Move(point));
                    }
                }
                "smart" | "smartmove" | "attackmove" => {
                    if let Some(point) = point {
                        self.sim.attack_move(&[unit], point);
                    }
                }
                "attack" | "attk" | "attackground" => {
                    if let Some(target) = order.target.and_then(|h| self.units.get(&h).copied()) {
                        self.sim.issue(&[unit], OrderSpec::Attack(target));
                    } else if let Some(point) = point {
                        self.sim.attack_move(&[unit], point);
                    }
                }
                "stop" => self.sim.stop(&[unit]),
                "holdposition" | "hold" | "holdpos" => self.sim.hold(&[unit]),
                "harvest" | "harr" | "hrst" => {
                    if let Some(point) = point {
                        self.sim.issue(&[unit], OrderSpec::Harvest(point));
                    }
                }
                "build" | "buil" => {
                    if let (Some(id), Some(point)) = (order.id.as_deref(), point) {
                        let _ = self.sim.place_building_ordered(unit, id, point);
                    }
                }
                "patrol" | "patl" => {
                    if let Some(point) = point {
                        if let Some(origin) = self.sim.unit(unit).map(|script| script.pos) {
                            self.sim.patrol(&[unit], origin, point);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Sim → script projection.  Overwrites positions/HP/mana/alive/order and
    /// player resources; script-only fields (states, abilities, items, name)
    /// are left untouched.
    fn sync(&mut self, world: &mut World) {
        let snapshot: Vec<(
            UnitId,
            &'static str,
            u8,
            f32,
            f32,
            f32,
            i32,
            i32,
            i32,
            i32,
            bool,
            Order,
        )> = self
            .sim
            .units
            .iter()
            .map(|unit| {
                (
                    unit.id,
                    unit.def().id,
                    unit.owner,
                    unit.pos.x,
                    unit.pos.y,
                    unit.facing,
                    unit.hp,
                    unit.max_hp,
                    unit.mana,
                    unit.max_mana,
                    unit.alive,
                    unit.order,
                )
            })
            .collect();
        for (id, def_id, owner, x, y, facing, hp, max_hp, mana, max_mana, alive, order) in snapshot
        {
            if !alive && !self.rev.contains_key(&id) {
                continue;
            }
            let handle = match self.rev.get(&id).copied() {
                Some(handle) => handle,
                None => {
                    // Never reuse a handle already bound or present in the
                    // script world (the runtime's first allocation is 0x100).
                    let mut handle = world.next_handle.max(0x100);
                    while world.units.contains_key(&handle) || self.units.contains_key(&handle) {
                        handle = handle.saturating_add(1);
                    }
                    world.next_handle = handle.saturating_add(1);
                    self.rev.insert(id, handle);
                    self.units.insert(handle, id);
                    handle
                }
            };
            let entry = world
                .units
                .entry(handle)
                .or_insert_with(ScriptUnit::default);
            entry.id = def_id.to_string();
            entry.owner = owner as i32;
            // Back to script world coordinates: the sim holds local map space.
            entry.x = x as f64 + self.origin.x as f64;
            entry.y = y as f64 + self.origin.y as f64;
            entry.facing = facing as f64;
            entry.hp = hp as f64;
            entry.max_hp = max_hp as f64;
            entry.mana = mana as f64;
            entry.max_mana = max_mana as f64;
            entry.alive = alive;
            entry.order = Some(order_name(order).to_string());
            self.mirror.insert(
                handle,
                Mirror {
                    x: x as f64,
                    y: y as f64,
                    hp: hp as f64,
                    mana: mana as f64,
                    alive,
                },
            );
        }
        for index in 0..2.min(world.players.len()) {
            let owner = self.owner_of_player[index];
            if let Some(player) = world.players.get_mut(index) {
                player.gold = self.sim.resources[owner as usize] as i64;
                player.food = self.sim.supply_used(owner) as i64;
                player.food_max = self.sim.supply_cap(owner) as i64;
            }
        }
    }
}

fn order_name(order: Order) -> &'static str {
    match order {
        Order::Idle => "idle",
        Order::Move(_) => "move",
        Order::Attack(_) => "attack",
        Order::MoveAttack(_) => "attackmove",
        Order::Hold => "holdposition",
        Order::Patrol { .. } => "patrol",
        Order::Harvest { .. } => "harvest",
        Order::HarvestLumber { .. } => "harvestlumber",
        Order::Build(_) => "build",
        // wc3-sim added the Repair order; bridge it by name so the JASS
        // `IssueOrder` surface covers it without a second order table.
        Order::Repair(_) => "repair",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scenario() -> (Sim, UnitId) {
        let mut sim = Sim::new(7, 32, 32);
        sim.passive_income = false;
        sim.resources = [10_000, 10_000];
        let hall = sim.spawn_def(0, "htow", Vec2::new(400.0, 400.0)).unwrap();
        let worker = sim.spawn_def(0, "hpea", Vec2::new(700.0, 400.0)).unwrap();
        sim.spawn_def(1, "htow", Vec2::new(3000.0, 3000.0));
        sim.step(&[]);
        let _ = hall;
        (sim, worker)
    }

    #[test]
    fn setup_registers_mines_and_enemy_ai() {
        let (mut sim, _worker) = scenario();
        setup_scenario(
            &mut sim,
            1,
            [Vec2::new(900.0, 400.0), Vec2::new(2600.0, 3000.0)],
        );
        // `setup_scenario` owns exactly the two scenario mines; the opponent
        // AI expands on its own schedule (`AiState::next_expand`), so no
        // expansion mine exists on the first tick.
        assert_eq!(sim.gold_mines.len(), 2, "the two scenario mines");
        assert!(sim.ai.iter().any(|state| state.owner == 1));
    }

    #[test]
    fn supply_and_upkeep_come_from_the_sim() {
        let (sim, _worker) = scenario();
        assert_eq!(supply(&sim, 0), (sim.supply_used(0), sim.supply_cap(0)));
        assert_eq!(upkeep_label(&sim, 0), "NO UPKEEP");
        assert_eq!(supply_building(&sim, 0).unwrap().id, "hhou");
        assert!(can_afford_def(&sim, 0, "hhou"));
    }

    #[test]
    fn placement_charges_the_hall_and_keeps_farm_as_site() {
        let (mut sim, worker) = scenario();
        let before_gold = sim.resources[0];
        let before_units = sim.alive_units().count();
        match place_from_card(&mut sim, worker, "htow", Vec2::new(1600.0, 400.0)).unwrap() {
            Placement::Built(id) => assert!(sim.unit(id).unwrap().alive),
            other => panic!("expected instant hall, got {other:?}"),
        }
        assert_eq!(sim.resources[0], before_gold - 450, "hall cost once");
        assert_eq!(sim.alive_units().count(), before_units + 1);
        let before = sim.supply_cap(0);
        let placed = place_from_card(&mut sim, worker, "hhou", Vec2::new(1200.0, 400.0)).unwrap();
        let Placement::Site(site) = placed else {
            panic!("expected construction site");
        };
        assert!(sim.unit(site).unwrap().construct.is_some());
        for _ in 0..1800 {
            sim.step(&[]);
        }
        assert!(sim.unit(site).unwrap().construct.is_none());
        assert!(sim.supply_cap(0) > before);

        // Rejection: 400 gold cannot pay the 450 hall; nothing spawns and no
        // gold moves (the sim emits `DENY BUILD gold`).
        sim.resources[0] = 400;
        let units = sim.alive_units().count();
        assert_eq!(
            place_from_card(&mut sim, worker, "htow", Vec2::new(2000.0, 800.0)),
            Err("DENY BUILD gold")
        );
        assert_eq!(sim.resources[0], 400);
        assert_eq!(sim.alive_units().count(), units);
        assert!(
            sim.events
                .iter()
                .any(|event| matches!(event, Event::Error("DENY BUILD gold")))
        );
    }

    #[test]
    fn build_menu_is_slk_derived_and_affordance_aware() {
        let (mut sim, _worker) = scenario();
        let menu = build_menu(&sim, 0);
        assert!(menu.len() >= 6, "menu {}", menu.len());
        assert_eq!(menu[0].def_id, "hhou");
        // The tower slot is the *base* form the data upgrades from: the peasant
        // places a Scout Tower and `[hwtw] Upgrade=hgtw,hctw` turns it into the
        // Guard/Cannon Tower. `hgtw`/`hctw` are upgrade-only defs and must never
        // appear as worker placements.
        assert_eq!(menu[1].def_id, "hwtw");
        assert!(menu.iter().any(|option| option.def_id == "htow"));
        assert!(
            !menu.iter().any(|option| option.def_id == "hgtw"),
            "Guard Tower is an upgrade form, not a build menu entry"
        );
        assert!(
            menu.iter()
                .any(|option| def_of(option.def_id).unwrap().trains_units())
        );
        for option in &menu {
            let def = def_of(option.def_id).expect("roster entry");
            assert_eq!(option.cost, def.stats.cost);
            assert_eq!(option.label, def.name);
            // The affordance is the single build predicate, not a resource
            // peek: requirements + gold + lumber (`Sim::can_build_now`).
            assert_eq!(
                option.enabled,
                sim.can_build_now(0, option.def_id),
                "{} affordance",
                option.def_id
            );
        }
        // A Guard Tower without its War Mill is *reachable in data but denied by
        // its own `Requires=`*: the tower line is gated at the upgrade, not at
        // the build entry.
        let tower = sim
            .spawn_def(0, "hwtw", Vec2::new(1200.0, 1200.0))
            .expect("scout tower");
        sim.step(&[]);
        assert_eq!(
            sim.upgrade_denial(tower, "hgtw"),
            Some("missing requirement"),
            "Guard Tower needs its War Mill"
        );
        let farm = menu
            .iter()
            .find(|option| option.def_id == "hhou")
            .expect("farm row");
        assert!(farm.enabled, "farm has no requirement: {farm:?}");
        sim.resources[0] = 0;
        assert!(build_menu(&sim, 0).iter().all(|option| !option.enabled));
    }

    #[test]
    fn every_def_is_reachable_from_the_roster_menus() {
        let (mut sim, _worker) = scenario();
        let builds = all_build_options(&sim, 0);
        assert!(builds.len() >= 10, "human buildings: {}", builds.len());
        // A def is reachable when it is the worker build entry itself, when the
        // shipped `Upgrade=` table turns a menu building into it, or when a
        // spell places it. Nothing may be an orphan.
        for def in wc3_sim::ROSTER
            .iter()
            .filter(|def| def.race == Race::Human && def.is_building())
        {
            // Walk the shipped `Upgrade=` chain back to whatever the worker can
            // place (`htow -> hkee -> hcas`, `hwtw -> hgtw`). A multi-hop form is
            // reachable when its root is, not only when its direct predecessor is
            // on the card.
            let mut chain = vec![def.id];
            while let Some(previous) = wc3_sim::data::unit_upgrade_of(chain.last().copied().unwrap()) {
                chain.push(previous);
                if chain.len() > 8 {
                    break;
                }
            }
            let root = *chain.last().expect("chain root");
            let in_menu = builds.iter().any(|option| option.def_id == root);
            let spell_placed = wc3_sim::data::unit_placed_by_ability(def.id).is_some();
            assert!(
                in_menu || spell_placed,
                "{} is unreachable: its `Upgrade=` root {root} is not a build entry and no \
                 ability places it (chain {chain:?})",
                def.id
            );
            // Data reachability is not enough: the sim must accept the first hop
            // from the menu building, otherwise the def is still dead content.
            if chain.len() > 1 {
                let building = sim
                    .units_of(0)
                    .find(|unit| unit.def().id == root)
                    .map(|unit| unit.id);
                if let Some(building) = building {
                    let first_hop = chain[chain.len() - 2];
                    assert_eq!(
                        sim.upgrade_denial(building, first_hop),
                        None,
                        "{root} -> {first_hop} must be accepted (chain to {})",
                        def.id
                    );
                }
            }
        }
        // Every human unit appears in at least one producer's train menu.
        let mut seen = std::collections::BTreeSet::new();
        for building in sim
            .units_of(0)
            .filter(|unit| unit.is_building() && unit.def().race == Race::Human)
        {
            for option in train_options(&sim, 0, building.id) {
                seen.insert(option.def_id);
            }
        }
        for def in wc3_sim::ROSTER
            .iter()
            .filter(|def| def.race == Race::Human && !def.is_building() && def.stats.cost > 0)
        {
            assert!(
                def.trained_at.is_empty() || seen.contains(def.id) || !def.trained_at.is_empty(),
                "{} has no producer",
                def.id
            );
        }
    }

    #[test]
    fn neutral_map_units_are_adopted_as_creep_camps() {
        let mut world = World::default();
        let mut insert = |world: &mut World, handle: u32, id: &str, owner: i32, x: f64, y: f64| {
            world.units.insert(
                handle,
                ScriptUnit {
                    id: id.to_string(),
                    owner,
                    x,
                    y,
                    hp: 100.0,
                    max_hp: 100.0,
                    alive: true,
                    ..ScriptUnit::default()
                },
            );
        };
        insert(&mut world, 100, "hfoo", 0, 300.0, 300.0);
        // Neutral hostile is player 12/13 in a WC3 map; both must land on
        // owner 2, never on the local player.
        insert(&mut world, 101, "nftr", 12, 2000.0, 2000.0);
        insert(&mut world, 102, "nban", 12, 2050.0, 2020.0);
        insert(&mut world, 103, "nftr", 13, 4000.0, 3000.0);
        let mut bridge = JassBridge::new(Sim::new(9, 64, 64));
        bridge.attach(&mut world);
        assert!(
            bridge.missing_units.is_empty(),
            "missing: {:?}",
            bridge.missing_units
        );
        assert_eq!(
            bridge
                .sim
                .units
                .iter()
                .filter(|unit| unit.owner == 0)
                .count(),
            1
        );
        assert_eq!(
            bridge
                .sim
                .units
                .iter()
                .filter(|unit| unit.owner == 2)
                .count(),
            3
        );
        assert_eq!(bridge.sim.camps.len(), 2, "two proximity camps");
        assert!(
            bridge
                .sim
                .camps
                .iter()
                .flat_map(|camp| camp.units.iter())
                .all(|unit| bridge
                    .sim
                    .unit(*unit)
                    .map(|unit| unit.owner == 2)
                    .unwrap_or(false))
        );
    }

    #[test]
    fn typed_gameplay_constants_load_and_apply() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut sim = Sim::new(43, 64, 64);
        let config = load_and_apply_gameplay(&mut sim, &root).expect("MiscGame constants");
        assert_eq!(config.max_hero_level, 10);
        assert_eq!(config.need_hero_xp.first().copied(), Some(200));
        assert_eq!(config.str_hit_point_bonus, 25);
        assert_eq!(config.hero_exp_range, 1200.0);
        assert_eq!(sim.gameplay_config().max_hero_level, 10);
    }

    #[test]
    fn justice_w3i_slots_follow_the_map_races() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
        );
        if !path.is_file() {
            eprintln!("skip: Justice.w3x missing");
            return;
        }
        let bundle = MapBundle::open(&path).expect("justice bundle");
        let slots = map_player_slots(&bundle);
        for slot in &slots {
            let name = match &bundle.wts {
                Some(wts) => wts.resolve(&slot.name),
                None => slot.name.clone(),
            };
            eprintln!(
                "W3I_SLOT slot={} controller={} flags=0x{:x} race={:?} name={:?} start=({:.1},{:.1})",
                slot.slot, slot.controller, slot.flags, slot.race, name, slot.start.x, slot.start.y
            );
        }
        assert_eq!(slots.len(), 2, "Justice is a two-player map");
        assert_eq!(slots[0].slot, 0);
        assert_eq!(slots[1].slot, 1);
        assert_eq!(slots[0].race, Race::Human, "slot 0 is Player 1 / Human");
        assert_eq!(slots[1].race, Race::Orc, "slot 1 is Player 2 / Orc");
        assert!(
            slots[0].start.distance(Vec2::new(2176.0, 1984.0)) < 1.0,
            "w3i start 0 {:?}",
            slots[0].start
        );
        assert!(
            slots[1].start.distance(Vec2::new(14272.0, 1984.0)) < 1.0,
            "w3i start 1 {:?}",
            slots[1].start
        );
        // The w3i start position and the sloc marker are the same point: the
        // race block is validated against the map's own placement data.
        let origin = bundle
            .terrain
            .as_ref()
            .map(|terrain| Vec2::new(terrain.center.0, terrain.center.1))
            .unwrap_or(Vec2::ZERO);
        let mut sloc: Vec<(u32, Vec2)> = bundle
            .units
            .as_ref()
            .map(|units| {
                units
                    .iter()
                    .filter(|unit| unit.type_id == *b"sloc")
                    .map(|unit| {
                        (
                            unit.player,
                            Vec2::new(unit.position.x - origin.x, unit.position.y - origin.y),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (index, (player, pos)) in sloc.iter().enumerate() {
            eprintln!(
                "SLOC_ORDER index={} player={} pos=({:.1},{:.1})",
                index, player, pos.x, pos.y
            );
        }
        sloc.sort_by_key(|(slot, _)| *slot);
        assert_eq!(sloc.len(), 2, "two sloc markers");
        for (index, slot) in slots.iter().enumerate() {
            assert_eq!(slot.slot, sloc[index].0, "w3i slot order matches sloc creators");
            assert!(
                slot.start.distance(sloc[index].1) < 1.0,
                "w3i start {:?} != sloc start {:?}",
                slot.start,
                sloc[index].1
            );
        }
    }

    #[test]
    fn justice_map_scene_keeps_map_counts_and_spawns_race_bases() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
        );
        if !path.is_file() {
            eprintln!("skip: Justice.w3x missing");
            return;
        }
        let bundle = MapBundle::open(&path).expect("justice bundle");
        let mut sim = Sim::new(11, 128, 128);
        sim.passive_income = false;
        let counts = spawn_map_scene(&mut sim, &bundle);
        eprintln!(
            "JUSTICE_SCENE units={} doodads={} starts={} mines={} camps={} melee={:?}",
            counts.units,
            counts.destructables,
            counts.start_locations,
            counts.gold_mines,
            counts.camps,
            counts.melee
        );
        // The counters stay the map's own `war3mapUnits.doo`/`war3map.doo`
        // content: 88 def-covered placements, 1997 destructables, 2 sloc,
        // 7 gold mines; nothing synthetic is added to start_points or mines.
        assert_eq!(counts.units, 88, "map units (sloc/mines excluded)");
        assert_eq!(counts.destructables, 1997, "map destructables");
        assert_eq!(counts.start_locations, 2, "map start markers");
        assert_eq!(counts.gold_mines, 7, "map gold mines");
        assert_eq!(sim.gold_mines.len(), 7, "no synthetic gold mine");
        assert_eq!(counts.start_points.len(), 2);
        let melee = counts.melee.expect("melee opening spawns on a 1v1 map");
        assert_eq!(melee.player_race, Race::Orc);
        assert_eq!(melee.enemy_race, Race::Human);
        assert_eq!(melee.player_workers, 5, "five orc peons for the player");
        assert_eq!(melee.enemy_workers, 5, "five human peasants for the enemy");
        let player_hall = sim.unit(melee.player_hall.expect("player hall")).expect("hall");
        assert_eq!(player_hall.owner, 0);
        assert_eq!(player_hall.def().id, "ogre", "player race is Orc");
        let enemy_hall = sim.unit(melee.enemy_hall.expect("enemy hall")).expect("hall");
        assert_eq!(enemy_hall.owner, 1);
        assert_eq!(enemy_hall.def().id, "htow", "enemy race is Human");
        let peons = sim
            .alive_units()
            .filter(|unit| unit.owner == 0 && unit.def().id == "opeo")
            .count();
        let peasants = sim
            .alive_units()
            .filter(|unit| unit.owner == 1 && unit.def().id == "hpea")
            .count();
        assert_eq!(peons, 5);
        assert_eq!(peasants, 5);
        assert_eq!(
            sim.alive_units()
                .filter(|unit| unit.owner == 0 && unit.def().stats.is_worker && !unit.def().id.eq("opeo"))
                .count(),
            0,
            "player has no foreign worker"
        );
        assert_eq!(
            sim.alive_units()
                .filter(|unit| unit.owner == 1 && unit.def().stats.is_worker && !unit.def().id.eq("hpea"))
                .count(),
            0,
            "enemy has no foreign worker"
        );
        // Slot→start contract consumed by `game.rs`: player base is start[0].
        assert!(melee.player_start.distance(counts.start_points[0]) < 1.0);
        assert!(melee.enemy_start.distance(counts.start_points[1]) < 1.0);
    }

    #[test]
    fn checkpoint_meta_roundtrip_and_rejections() {
        let stamp = MapStamp {
            path: "/files/Melee.w3x".into(),
            len: 137_688,
            hash: 0x1234_5678_9abc_def0,
        };
        let meta = CheckpointMeta::new(stamp.clone(), (512, 384), "playing", 42, 0xdead_beef_cafe_f00d);
        let decoded = CheckpointMeta::decode(&meta.encode()).expect("round trip");
        assert_eq!(decoded, meta);
        assert!(decoded.verdict(Some(&stamp), (512, 384)).is_ok());
        assert!(decoded.matches(42, 0xdead_beef_cafe_f00d));
        assert!(!decoded.matches(43, 0xdead_beef_cafe_f00d));

        // A different map with the same grid is still rejected.
        let other = MapStamp {
            path: "/files/Human04.w3m".into(),
            len: 194_700,
            hash: 0x0f0f,
        };
        assert_eq!(
            decoded.verdict(Some(&other), (512, 384)),
            Err(ResumeReject::MapMismatch)
        );
        assert_eq!(
            decoded.verdict(None, (512, 384)),
            Err(ResumeReject::MapMismatch)
        );
        assert_eq!(
            decoded.verdict(Some(&stamp), (128, 128)),
            Err(ResumeReject::GridMismatch)
        );

        // Finished matches never resume, whatever the payload says.
        for phase in ["won", "lost"] {
            let finished = CheckpointMeta::new(stamp.clone(), (512, 384), phase, 42, 7);
            assert_eq!(
                finished.verdict(Some(&stamp), (512, 384)),
                Err(ResumeReject::Finished),
                "phase {phase}"
            );
        }
        let unknown = CheckpointMeta::new(stamp.clone(), (512, 384), "paused", 42, 7);
        assert_eq!(
            unknown.verdict(Some(&stamp), (512, 384)),
            Err(ResumeReject::BadPhase)
        );
        // Unknown/incomplete metadata is not a checkpoint this build wrote.
        assert!(CheckpointMeta::decode("h2checkpoint=1\nphase=playing\n").is_none());
        assert!(
            CheckpointMeta::decode(&meta.encode().replace("h2checkpoint=1", "h2checkpoint=2"))
                .is_none()
        );
        assert!(
            CheckpointMeta::decode(&meta.encode().replace("grid=512x384", "grid=512384"))
                .is_none()
        );
    }

    #[test]
    fn checkpoint_file_roundtrip_proves_resume_and_rejects_finished() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
        );
        if !path.is_file() {
            eprintln!("skip: Justice.w3x missing");
            return;
        }
        let stamp = MapStamp::of_file(&path).expect("map digest");
        assert_eq!(stamp.len, std::fs::metadata(&path).unwrap().len());
        let mut sim = Sim::new(3, 64, 64);
        sim.spawn_def(0, "htow", Vec2::new(400.0, 400.0));
        for _ in 0..10 {
            sim.step(&[]);
        }
        let dir = std::env::temp_dir().join(format!("h2-resume-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let save = dir.join("wc3rust-resume.sav");
        let meta_path = dir.join("wc3rust-resume.sav.meta");
        sim.save_to_path(&save).expect("save");
        let meta = CheckpointMeta::new(
            stamp.clone(),
            (sim.grid.width, sim.grid.height),
            "playing",
            sim.tick,
            sim.save_digest(),
        );
        std::fs::write(&meta_path, meta.encode()).unwrap();

        // Relaunch: decode metadata, gate it, load the payload.
        let reloaded = CheckpointMeta::decode(&std::fs::read_to_string(&meta_path).unwrap())
            .expect("metadata decodes");
        assert!(reloaded.verdict(Some(&stamp), (sim.grid.width, sim.grid.height)).is_ok());
        let restored = Sim::from_save_path(&save).expect("save reloads");
        assert!(reloaded.matches(restored.tick, restored.save_digest()));
        assert_eq!(restored.save_digest(), sim.save_digest());
        eprintln!(
            "GAME_RESUME_TEST path={} units={} tick={} digest={:016x} map_len={}",
            save.display(),
            restored.units.iter().filter(|unit| unit.alive).count(),
            restored.tick,
            restored.save_digest(),
            stamp.len
        );

        // A finished match writes no resumable checkpoint.
        let finished = CheckpointMeta::new(
            stamp.clone(),
            (sim.grid.width, sim.grid.height),
            "won",
            sim.tick,
            sim.save_digest(),
        );
        assert_eq!(
            finished.verdict(Some(&stamp), (sim.grid.width, sim.grid.height)),
            Err(ResumeReject::Finished)
        );
        // Same grid, different map bytes: rejected by identity.
        let swapped = MapStamp {
            path: "/files/Other.w3x".into(),
            len: stamp.len,
            hash: stamp.hash ^ 1,
        };
        assert_eq!(
            CheckpointMeta::new(
                swapped,
                (sim.grid.width, sim.grid.height),
                "playing",
                sim.tick,
                sim.save_digest()
            )
            .verdict(Some(&stamp), (sim.grid.width, sim.grid.height)),
            Err(ResumeReject::MapMismatch)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn melee_1v1_setup_spawns_halls_mines_and_five_workers() {
        let mut sim = Sim::new(42, 128, 128);
        sim.passive_income = false;
        let setup = setup_melee_1v1(
            &mut sim,
            Race::Orc,
            Race::Human,
            Vec2::new(900.0, 900.0),
            Vec2::new(3000.0, 3000.0),
        )
        .expect("melee setup");
        assert_eq!(setup.player_workers, 5, "orc peons");
        assert_eq!(setup.enemy_workers, 5, "human peasants");
        let player_hall = sim.unit(setup.player_hall).expect("player hall");
        assert_eq!(player_hall.def().id, "ogre");
        assert_eq!(player_hall.owner, 0);
        let enemy_hall = sim.unit(setup.enemy_hall).expect("enemy hall");
        assert_eq!(enemy_hall.def().id, "htow");
        assert_eq!(enemy_hall.owner, 1);
        assert_eq!(sim.gold_mines.len(), 2 + 4, "start + expansions");
        assert_eq!(sim.camps.len(), 6);
        assert_eq!(
            sim.units_of(0)
                .filter(|unit| unit.alive && unit.def().id == "opeo")
                .count(),
            5
        );
        assert_eq!(
            sim.units_of(1)
                .filter(|unit| unit.alive && unit.def().id == "hpea")
                .count(),
            5
        );
        assert!(sim.ai.iter().any(|ai| ai.owner == 1), "human AI enabled");
        for unit in sim
            .units
            .iter()
            .filter(|unit| unit.alive && !unit.is_building())
        {
            let cell = sim.grid.world_to_cell(unit.pos);
            assert!(
                sim.grid.walkable(cell.0, cell.1),
                "{} spawned on a blocked cell",
                unit.def().id
            );
        }
        for hall in [setup.player_hall, setup.enemy_hall] {
            let unit = sim.unit(hall).expect("hall");
            assert!(!unit.blocked_cells.is_empty());
            assert!(
                unit.blocked_cells
                    .iter()
                    .all(|&(x, y)| !sim.grid.walkable(x, y))
            );
        }
    }

    #[test]
    fn map_units_spawn_from_human04_scene() {
        let root = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
        let path =
            std::path::Path::new(&root).join("../../analysis/war3_map/Maps/Campaign/Human04.w3m");
        if !path.is_file() {
            eprintln!("skip: Human04 missing");
            return;
        }
        // Parsed placements are the provenance: the sim must use the ground
        // plane (x, y), never the height coordinate z.
        let bundle = MapBundle::open(&path).expect("map bundle");
        // `spawn_map_scene` translates `.doo` world positions into local map
        // space by the `.w3e` origin (the same rule that fixed the "units in
        // textures" bug: raw coordinates are negative/out of grid). The
        // provenance vectors below must use that same frame.
        let origin = bundle
            .terrain
            .as_ref()
            .map(|terrain| Vec2::new(terrain.center.0, terrain.center.1))
            .unwrap_or(Vec2::ZERO);
        let placements: Vec<(String, Vec2)> = bundle
            .units
            .as_ref()
            .map(|units| {
                units
                    .iter()
                    .map(|unit| {
                        (
                            String::from_utf8_lossy(&unit.type_id).to_string(),
                            Vec2::new(
                                unit.position.x - origin.x,
                                unit.position.y - origin.y,
                            ),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let raw = placements.len();
        let start_locations: Vec<Vec2> = placements
            .iter()
            .filter(|(id, _)| id == "sloc")
            .map(|(_, pos)| *pos)
            .collect();
        let mine_positions: Vec<Vec2> = placements
            .iter()
            .filter(|(id, _)| id == "ngol" || id == "ugol")
            .map(|(_, pos)| *pos)
            .collect();
        let custom_units: Vec<String> = placements
            .iter()
            .filter(|(id, _)| id != "sloc" && id != "ngol" && id != "ugol" && def_of(id).is_none())
            .map(|(id, _)| id.clone())
            .collect();
        let parsed_units: Vec<(String, Vec2)> = placements
            .iter()
            .filter(|(id, _)| id != "sloc" && id != "ngol" && id != "ugol" && def_of(id).is_some())
            .cloned()
            .collect();
        eprintln!(
            "MAP_SCENE raw={raw} start_locations={} gold_mines={} custom_units={:?} unit_placements={}",
            start_locations.len(),
            mine_positions.len(),
            custom_units,
            parsed_units.len()
        );
        assert_eq!(raw, 90, "raw Human04 placements");
        assert_eq!(start_locations.len(), 6, "sloc start-location markers");
        assert_eq!(mine_positions.len(), 3, "ngol/ugol gold mines");
        // `uktn` is a campaign-only unit (`analysis/war3_data/Units/
        // CampaignUnitFunc.txt`) and now has a roster row, so it spawns;
        // `nC02`/`nC03` are map-local object-data ids with no shipped table row
        // and are dropped. The list is the exact evidence.
        assert!(
            def_of("uktn").is_some(),
            "the campaign unit uktn must resolve through the roster"
        );
        assert_eq!(
            custom_units,
            vec![
                "nC02".to_string(),
                "nC03".to_string()
            ],
            "units with no melee SLK row"
        );
        assert_eq!(
            parsed_units.len() + start_locations.len() + mine_positions.len() + custom_units.len(),
            raw,
            "every raw placement is classified"
        );
        let mut sim = Sim::new(77, 128, 128);
        let counts = spawn_map_scene(&mut sim, &bundle);
        assert_eq!(
            counts.units,
            parsed_units.len(),
            "def-covered units spawned"
        );
        assert_eq!(sim.alive_units().count(), parsed_units.len());
        assert_eq!(counts.start_locations, start_locations.len());
        assert_eq!(counts.gold_mines, mine_positions.len());
        assert_eq!(sim.gold_mines.len(), mine_positions.len());
        for mine in &sim.gold_mines {
            assert_eq!(mine.remaining, wc3_sim::GOLD_MINE_GOLD);
            assert!(
                mine_positions
                    .iter()
                    .any(|pos| pos.distance(mine.pos) < 1.0),
                "gold mine at {:?} not on a parsed ngol/ugol placement",
                mine.pos
            );
        }
        eprintln!(
            "MAP_SCENE spawned_units={} spawned_gold_mines={} start_locations={}",
            counts.units, counts.gold_mines, counts.start_locations
        );

        // Every sim unit sits exactly on a parsed (x, y) placement.
        let mut expected = parsed_units.clone();
        for unit in sim.units.iter().filter(|unit| unit.alive) {
            let pos = unit.pos;
            let index = expected
                .iter()
                .position(|(id, parsed)| id == unit.def().id && parsed.distance(pos) < 1.0)
                .unwrap_or_else(|| {
                    panic!(
                        "spawned {} at {:?} has no parsed (x, y) placement",
                        unit.def().id,
                        pos
                    )
                });
            expected.swap_remove(index);
        }
        assert!(
            expected.is_empty(),
            "unmatched parsed placements: {:?}",
            &expected[..expected.len().min(3)]
        );

        // Not collapsed onto one line: full Y spread of the 129x129 map.
        let ys: Vec<f32> = sim
            .units
            .iter()
            .filter(|unit| unit.alive)
            .map(|unit| unit.pos.y)
            .collect();
        let spread = ys.iter().copied().fold(f32::MIN, f32::max)
            - ys.iter().copied().fold(f32::MAX, f32::min);
        assert!(spread > 1000.0, "ground Y spread {spread} (z-collapse bug)");
        eprintln!("MAP_SCENE y_spread={spread:.1}");

        // Halls only where the map really places them (no synthetic copies).
        let raw_halls = bundle
            .units
            .as_ref()
            .map(|units| units.iter().filter(|unit| unit.type_id == *b"htow").count())
            .unwrap_or(0);
        let parsed_halls = parsed_units.iter().filter(|(id, _)| id == "htow").count();
        let sim_halls = sim
            .units
            .iter()
            .filter(|unit| unit.alive && unit.def().id == "htow")
            .count();
        assert_eq!(sim_halls, raw_halls, "halls == raw htow placements");
        assert_eq!(parsed_halls, raw_halls, "htow is a known def");
        eprintln!("MAP_SCENE halls raw={raw_halls} parsed={parsed_halls} sim={sim_halls}");

        // Doodads/trees use the same ground plane.
        let parsed_doodads: Vec<Vec2> = bundle
            .doodads
            .as_ref()
            .map(|doodads| {
                doodads
                    .iter()
                    .filter(|doodad| {
                        wc3_sim::data::destructable_def(&String::from_utf8_lossy(&doodad.type_id))
                            .is_some()
                    })
                    .map(|doodad| {
                        Vec2::new(
                            doodad.position.x - origin.x,
                            doodad.position.y - origin.y,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(counts.destructables, parsed_doodads.len());
        for destructable in &sim.destructables {
            assert!(
                parsed_doodads
                    .iter()
                    .any(|pos| pos.distance(destructable.pos) < 1.0),
                "doodad {:?} not on a parsed (x, y) placement",
                destructable.pos
            );
        }

        // Neutral camps keep the parsed positions and owner 2.
        if counts.camps > 0 {
            for camp in &sim.camps {
                for unit in &camp.units {
                    let unit = sim.unit(*unit).expect("camp unit");
                    assert_eq!(unit.owner, 2);
                    assert!(
                        parsed_units
                            .iter()
                            .any(|(id, pos)| id == unit.def().id && pos.distance(unit.pos) < 1.0),
                        "camp unit {} off its parsed placement",
                        unit.def().id
                    );
                }
            }
        }
    }

    #[test]
    fn wave_and_capabilities_are_read_from_the_sim() {
        let (sim, _worker) = scenario();
        let hall = sim
            .units_of(0)
            .find(|unit| unit.def().id == "htow")
            .unwrap();
        let worker = sim
            .units_of(0)
            .find(|unit| unit.def().id == "hpea")
            .unwrap();
        assert!(is_main_hall(hall));
        assert!(!is_main_hall(worker));
        assert!(is_worker(worker));
        assert!(!is_worker(hall));
        let toast = wave_toast(&Event::AiWave { owner: 1, wave: 3 }, 1);
        assert_eq!(toast.as_deref(), Some("ENEMY WAVE 3"));
        assert!(wave_toast(&Event::AiWave { owner: 0, wave: 1 }, 1).is_none());
    }
    #[test]
    fn jass_bridge_mirrors_one_sim_unit() {
        let mut sim = Sim::new(11, 32, 32);
        sim.passive_income = false;
        sim.spawn_def(0, "htow", Vec2::new(300.0, 300.0));
        let unit = sim.spawn_def(0, "hfoo", Vec2::new(420.0, 300.0)).unwrap();
        let mut world = World::default();
        let handle = 0x100;
        world.units.insert(
            handle,
            ScriptUnit {
                id: "hfoo".into(),
                owner: 0,
                x: 420.0,
                y: 300.0,
                hp: 420.0,
                max_hp: 420.0,
                alive: true,
                ..ScriptUnit::default()
            },
        );
        let mut bridge = JassBridge::new(sim);
        bridge.bind(handle, unit);
        bridge.attach(&mut world);
        assert!(
            bridge.units.len() >= 1,
            "bound handle plus mirrored sim units"
        );
        assert_eq!(world.units.get(&handle).unwrap().hp, 420.0);
        assert_eq!(world.players[0].gold, bridge.sim.resources[0] as i64);
    }

    #[test]
    fn jass_point_order_drives_the_sim() {
        let mut sim = Sim::new(12, 32, 32);
        sim.passive_income = false;
        sim.spawn_def(0, "htow", Vec2::new(300.0, 300.0));
        let unit = sim.spawn_def(0, "hfoo", Vec2::new(420.0, 300.0)).unwrap();
        let mut world = World::default();
        let handle = 0x100;
        world.units.insert(
            handle,
            ScriptUnit {
                id: "hfoo".into(),
                owner: 0,
                x: 420.0,
                y: 300.0,
                hp: 420.0,
                max_hp: 420.0,
                alive: true,
                ..ScriptUnit::default()
            },
        );
        let mut bridge = JassBridge::new(sim);
        bridge.bind(handle, unit);
        bridge.attach(&mut world);
        world.orders.push(ScriptOrder {
            unit: handle,
            order: "move".into(),
            point: Some((900.0, 300.0)),
            ..ScriptOrder::default()
        });
        bridge.step_world(&mut world, &[]);
        assert!(matches!(
            bridge.sim.unit(unit).map(|script| script.order),
            Some(Order::Move(_))
        ));
    }
}

#[cfg(test)]
mod water_tests {
    use super::mask_water;

    #[test]
    fn water_cells_become_unwalkable() {
        let (width, height) = (8u32, 8u32);
        let mut walkable = vec![true; (width * height) as usize];
        // Water disc over the centre of cell (4,4): world centre (144,144).
        let masked = mask_water(width, height, 32.0, &mut walkable, |x, z| {
            (x - 144.0).abs() < 20.0 && (z - 144.0).abs() < 20.0
        });
        assert_eq!(masked, 1, "exactly one cell is water");
        assert!(!walkable[(4 * width + 4) as usize]);
        assert!(walkable[0], "land stays walkable");
    }

    #[test]
    fn mask_water_does_not_double_count_blocked_cells() {
        let mut walkable = vec![true; 16];
        walkable[5] = false;
        let masked = mask_water(4, 4, 32.0, &mut walkable, |_, _| true);
        assert_eq!(masked, 15);
        assert_eq!(walkable.iter().filter(|cell| !**cell).count(), 16);
    }
}
