//! Headless feature-parity gates for the WC3 feature surface.
//!
//! Owner: QA (`scripts/qa/**`). The engine crates are read-only dependencies:
//! every gate drives the public API and records observed counts, not claims.
//!
//! Gates map 1:1 to the feature rows requested for the parity matrix:
//!   train all defs, build all defs, cast abilities, hero stats/level, items
//!   data/runtime, JASS triggers, game audio, FDF menus/build menu.
//!
//! Machine output: `--out <json>` (compact JSON, consumed by
//! `scripts/qa/parity.py`). Exit code: 0 = PASS, 2 = PARTIAL (gaps only),
//! 1 = FAIL (a implemented mechanism is broken).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use wc3_audio::{
    decode_wav, AudioConfig, AudioEngine, GameAudio, Sound, SoundInfoDb, SoundProvider, UnitNameMap,
};
use wc3_data::source::DirSource;
use wc3_data::GameData;
use wc3_formats::fdf::FdfDocument;
use wc3_re::mapfiles::MapBundle;
use wc3_sim::data::{build_menu, main_hall_for, supply_building_for};
use wc3_sim::{
    def_of, ArmorType, Event, Order, Race, Sim, Spell, SpellTarget, TargetKind, UnitDef, UnitId,
    Vec2, CELL_SIZE, ROSTER,
};

// ---------------------------------------------------------------------------
// Gate bookkeeping
// ---------------------------------------------------------------------------

struct Gate {
    id: &'static str,
    area: &'static str,
    total: usize,
    pass: usize,
    fail: usize,
    gap: usize,
    detail: String,
    failures: Vec<String>,
    metrics: Vec<(String, String)>,
    evidence: Vec<String>,
}

impl Gate {
    fn new(id: &'static str, area: &'static str) -> Self {
        Self {
            id,
            area,
            total: 0,
            pass: 0,
            fail: 0,
            gap: 0,
            detail: String::new(),
            failures: Vec::new(),
            metrics: Vec::new(),
            evidence: Vec::new(),
        }
    }

    fn status(&self) -> &'static str {
        if self.fail > 0 {
            "FAIL"
        } else if self.gap > 0 {
            "GAP"
        } else {
            "PASS"
        }
    }

    fn fail(&mut self, message: impl Into<String>) {
        self.fail += 1;
        if self.failures.len() < 40 {
            self.failures.push(message.into());
        }
    }

    fn note(&mut self, key: &str, value: impl ToString) {
        self.metrics.push((key.to_string(), value.to_string()));
    }

    fn evidence(&mut self, value: impl Into<String>) {
        self.evidence.push(value.into());
    }

    fn finish(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        if self.total < self.pass + self.fail + self.gap {
            self.total = self.pass + self.fail + self.gap;
        }
        self
    }
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "unknown panic".to_string()
    }
}

fn safe_gate<F>(id: &'static str, area: &'static str, run: F) -> Gate
where
    F: FnOnce() -> Gate + std::panic::UnwindSafe,
{
    match catch_unwind(AssertUnwindSafe(run)) {
        Ok(gate) => gate,
        Err(payload) => {
            let mut gate = Gate::new(id, area);
            gate.total = 1;
            gate.fail = 1;
            gate.failures.push(format!("panic: {}", panic_message(payload)));
            gate.finish("gate panicked; see failures")
        }
    }
}

// ---------------------------------------------------------------------------
// Geometry helpers
// ---------------------------------------------------------------------------

fn cell_center(cx: u32, cy: u32) -> Vec2 {
    Vec2::new(
        cx as f32 * CELL_SIZE + CELL_SIZE * 0.5,
        cy as f32 * CELL_SIZE + CELL_SIZE * 0.5,
    )
}

/// Mirrors `Sim::footprint_cells` (private) so QA can pick a free site.
fn cells_for(sim: &Sim, at: Vec2, side: u8) -> Vec<(u32, u32)> {
    let side = side.max(1) as i32;
    let (cx, cy) = sim.grid.world_to_cell(at);
    let start_x = cx as i32 - (side - 1) / 2;
    let start_y = cy as i32 - (side - 1) / 2;
    let mut cells = Vec::new();
    for y in start_y..start_y + side {
        for x in start_x..start_x + side {
            if x < 0 || y < 0 || x >= sim.grid.width as i32 || y >= sim.grid.height as i32 {
                return Vec::new();
            }
            cells.push((x as u32, y as u32));
        }
    }
    cells
}

fn site_free(sim: &Sim, at: Vec2, side: u8) -> bool {
    let cells = cells_for(sim, at, side);
    if cells.is_empty() {
        return false;
    }
    if cells.iter().any(|&(x, y)| !sim.grid.walkable(x, y)) {
        return false;
    }
    !sim.units.iter().any(|unit| {
        unit.alive && cells.contains(&sim.grid.world_to_cell(unit.pos))
    })
}

fn find_site(sim: &Sim, base: (u32, u32), side: u8) -> Option<Vec2> {
    for radius in 0..16i32 {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if radius > 0 && dx.abs() != radius && dy.abs() != radius {
                    continue;
                }
                let cx = base.0 as i32 + dx;
                let cy = base.1 as i32 + dy;
                if cx < 1 || cy < 1 {
                    continue;
                }
                let at = cell_center(cx as u32, cy as u32);
                if site_free(sim, at, side) {
                    return Some(at);
                }
            }
        }
    }
    None
}

fn spawn_free(
    sim: &mut Sim,
    owner: u8,
    def_id: &str,
    base: (u32, u32),
    side: u8,
) -> Option<UnitId> {
    let at = find_site(sim, base, side)?;
    sim.spawn_def(owner, def_id, at)
}

/// Spawn the prerequisite building chain for `def` (recursively, depth-bounded).
///
/// The sim enforces `UnitDef::requires` on placement, so a per-def build gate
/// must first satisfy the tech tree with completed buildings. Prerequisites are
/// spawned through the same `spawn_def` path the map loader uses; nothing here
/// bypasses the requirement check on the def under test.
fn spawn_prerequisites(
    sim: &mut Sim,
    owner: u8,
    def: &'static UnitDef,
    base: (u32, u32),
    depth: usize,
) -> Result<(), String> {
    if depth > 6 {
        return Err(format!("{}: requirement chain too deep", def.id));
    }
    for required in def.requires {
        let Some(req_def) = ROSTER.iter().find(|candidate| candidate.id == *required) else {
            return Err(format!("{}: requirement {} not in roster", def.id, required));
        };
        let present = sim.units.iter().any(|unit| {
            unit.alive
                && unit.owner == owner
                && unit.is_building()
                && unit.construct.is_none()
                && unit.def().id == *required
        });
        if present {
            continue;
        }
        if let Err(reason) = spawn_prerequisites(sim, owner, req_def, base, depth + 1) {
            return Err(reason);
        }
        if spawn_free(sim, owner, required, base, 4).is_none() {
            return Err(format!("{}: no site for requirement {}", def.id, required));
        }
    }
    Ok(())
}

fn worker_for(race: Race) -> Option<&'static UnitDef> {
    ROSTER
        .iter()
        .find(|def| def.race == race && def.stats.is_worker && !def.is_building())
        .or_else(|| def_of("hpea"))
}

fn first_unit_def() -> &'static UnitDef {
    ROSTER
        .iter()
        .find(|def| !def.is_building() && def.stats.hp > 0)
        .expect("roster has a unit")
}

// ---------------------------------------------------------------------------
// train.all_defs
// ---------------------------------------------------------------------------

fn gate_train() -> Gate {
    let mut gate = Gate::new("train.all_defs", "train");
    let defs: Vec<&UnitDef> = ROSTER
        .iter()
        .filter(|def| !def.is_building() && !def.trained_at.is_empty())
        .collect();
    gate.total = defs.len();
    gate.note("roster_units", ROSTER.iter().filter(|d| !d.is_building()).count());
    gate.note("untrainable_units", ROSTER.iter().filter(|d| !d.is_building() && d.trained_at.is_empty()).count());

    for (index, def) in defs.iter().enumerate() {
        let mut sim = Sim::new(0x7A11 + index as u64, 64, 64);
        sim.passive_income = false;
        sim.resources[0] = 1_000_000;

        let mut setup_error: Option<String> = None;
        if let Some(hall) = main_hall_for(def.race) {
            if spawn_free(&mut sim, 0, hall.id, (10, 10), hall.footprint).is_none() {
                setup_error = Some(format!("{}: hall {} failed to spawn", def.id, hall.id));
            }
        }
        if let Some(supply) = supply_building_for(def.race) {
            for slot in 0..4u32 {
                let base = (18 + slot * 5, 10);
                if spawn_free(&mut sim, 0, supply.id, base, supply.footprint).is_none() {
                    setup_error = Some(format!("{}: supply {} failed to spawn", def.id, supply.id));
                }
            }
        }
        for (slot, required) in def.requires.iter().enumerate() {
            let base = (12 + slot as u32 * 5, 34);
            let Some(required_def) = def_of(required) else {
                setup_error = Some(format!("{}: unknown requirement {required}", def.id));
                continue;
            };
            if spawn_free(&mut sim, 0, required, base, required_def.footprint).is_none() {
                setup_error = Some(format!("{}: requirement {required} failed to spawn", def.id));
            }
        }

        let producer = def.trained_at[0];
        let Some(producer_def) = def_of(producer) else {
            gate.fail(format!("{}: unknown producer {producer}", def.id));
            continue;
        };
        let Some(building) =
            spawn_free(&mut sim, 0, producer, (24, 24), producer_def.footprint)
        else {
            gate.fail(format!("{}: producer {producer} failed to spawn", def.id));
            continue;
        };
        if let Some(error) = setup_error {
            gate.fail(error);
            continue;
        }

        let gold_before = sim.resources[0];
        if let Err(reason) = sim.train_def(building, def.id) {
            gate.fail(format!("{}: train_def({producer}) -> {reason}", def.id));
            continue;
        }
        let gold_delta = gold_before - sim.resources[0];
        if gold_delta != def.stats.cost {
            gate.fail(format!(
                "{}: cost charged {gold_delta}, roster {}",
                def.id, def.stats.cost
            ));
            continue;
        }

        let limit = def.stats.train_ticks.max(1) + 900;
        let mut trained: Option<UnitId> = None;
        let mut blocked: Option<&'static str> = None;
        for _ in 0..limit {
            sim.step(&[]);
            for event in &sim.events {
                match event {
                    Event::UnitTrained(id) => trained = Some(*id),
                    Event::Error(reason) => blocked = Some(*reason),
                    _ => {}
                }
            }
            if trained.is_some() {
                break;
            }
        }

        match trained {
            Some(id) => {
                let observed = sim.unit(id).map(|unit| {
                    (unit.alive, unit.owner, unit.def().id, unit.def_index)
                });
                let ok = observed
                    .map(|(alive, owner, def_id, _)| alive && owner == 0 && def_id == def.id)
                    .unwrap_or(false);
                if ok {
                    gate.pass += 1;
                } else {
                    gate.fail(format!("{}: trained unit mismatch {observed:?}", def.id));
                }
            }
            None => gate.fail(format!(
                "{}: no UnitTrained within {limit} ticks (last error: {blocked:?})",
                def.id
            )),
        }
    }
    let (pass, total) = (gate.pass, gate.total);
    gate.finish(format!(
        "{pass}/{total} trainable roster defs trained through the data path"
    ))
}

// ---------------------------------------------------------------------------
// train.menu_linkage
// ---------------------------------------------------------------------------

fn gate_train_linkage() -> Gate {
    let mut gate = Gate::new("train.menu_linkage", "train");
    // These three are produced by abilities (militia, shade, mounted hippogryph),
    // not by a `Trains=` link; the matrix lists them as legitimate.
    let ability_produced = ["hmil", "ushd", "ehpr"];
    let mut total = 0usize;
    for def in ROSTER.iter().filter(|def| !def.is_building()) {
        let expected_only = ability_produced.contains(&def.id);
        for producer in def.trained_at {
            total += 1;
            if expected_only {
                gate.pass += 1;
                continue;
            }
            match def_of(producer) {
                None => gate.fail(format!("{}: producer {producer} missing from roster", def.id)),
                Some(building) => {
                    if building.trains.contains(&def.id) {
                        gate.pass += 1;
                    } else {
                        gate.fail(format!(
                            "{}: producer {producer} does not list it in Trains",
                            def.id
                        ));
                    }
                }
            }
        }
    }
    for def in ROSTER.iter() {
        for trained in def.trains {
            total += 1;
            if def_of(trained).is_some() {
                gate.pass += 1;
            } else {
                gate.fail(format!("{}: Trains entry {trained} does not resolve", def.id));
            }
        }
    }
    gate.total = total;
    gate.note("ability_produced", ability_produced.join(","));
    let (pass, total) = (gate.pass, gate.total);
    gate.finish(format!(
        "{pass}/{total} command-card train links are bidirectional and resolvable"
    ))
}

// ---------------------------------------------------------------------------
// build.all_defs
// ---------------------------------------------------------------------------

fn gate_build() -> Gate {
    let mut gate = Gate::new("build.all_defs", "build");
    let defs: Vec<&UnitDef> = ROSTER.iter().filter(|def| def.is_building()).collect();
    gate.total = defs.len();

    for (index, def) in defs.iter().enumerate() {
        let Some(worker_def) = worker_for(def.race) else {
            gate.fail(format!("{}: no worker for {:?}", def.id, def.race));
            continue;
        };
        let mut sim = Sim::new(0xB011 + index as u64, 64, 64);
        sim.passive_income = false;
        sim.resources[0] = 1_000_000;
        let Some(worker) = sim.spawn_def(0, worker_def.id, cell_center(10, 10)) else {
            gate.fail(format!("{}: worker {} failed to spawn", def.id, worker_def.id));
            continue;
        };
        if let Err(reason) = spawn_prerequisites(&mut sim, 0, def, (20, 20), 0) {
            gate.fail(reason);
            continue;
        }
        let Some(site) = find_site(&sim, (13, 10), def.footprint) else {
            gate.fail(format!("{}: no free site", def.id));
            continue;
        };
        let gold_before = sim.resources[0];
        let building = match sim.place_building(worker, def.id, site) {
            Ok(id) => id,
            Err(reason) => {
                gate.fail(format!("{}: place_building -> {reason}", def.id));
                continue;
            }
        };
        let gold_delta = gold_before - sim.resources[0];
        if gold_delta != def.stats.cost {
            gate.fail(format!(
                "{}: cost charged {gold_delta}, roster {}",
                def.id, def.stats.cost
            ));
            continue;
        }

        let limit = def.stats.build_ticks.max(1) + 1_200;
        let mut complete = false;
        for _ in 0..limit {
            sim.step(&[]);
            if sim
                .events
                .iter()
                .any(|event| matches!(event, Event::BuildingComplete(id) if *id == building))
            {
                complete = true;
                break;
            }
        }
        let built = sim.unit(building).map(|unit| {
            (
                unit.alive,
                unit.construct.is_none(),
                unit.def().id,
                unit.blocked_cells.clone(),
            )
        });
        let ok = complete
            && built
                .as_ref()
                .map(|(alive, done, id, cells)| {
                    *alive
                        && *done
                        && *id == def.id
                        && !cells.is_empty()
                        && cells
                            .iter()
                            .all(|&(x, y)| !sim.grid.walkable(x, y))
                })
                .unwrap_or(false);
        if ok {
            gate.pass += 1;
        } else {
            gate.fail(format!(
                "{}: complete={complete} state={built:?} within {limit} ticks",
                def.id
            ));
        }
    }
    let (pass, total) = (gate.pass, gate.total);
    gate.finish(format!(
        "{pass}/{total} building defs placed, charged and completed"
    ))
}

fn gate_tech_requires() -> Gate {
    let mut gate = Gate::new("tech.requires", "tech");
    gate.total = 2;

    // 1. Building prerequisite: hkee (Keep) requires htow (Town Hall).
    {
        let mut sim = Sim::new(0x7EC0, 32, 32);
        sim.passive_income = false;
        sim.resources[0] = 100_000;
        let Some(worker) = sim.spawn_def(0, "hpea", cell_center(8, 8)) else {
            gate.fail("worker spawn failed");
            let (pass, total) = (gate.pass, gate.total);
            return gate.finish(format!("{pass}/{total} tech-tree checks passed"));
        };
        let site = find_site(&sim, (11, 8), 4).unwrap_or(cell_center(11, 8));
        match sim.place_building(worker, "hkee", site) {
            Err(_) => gate.pass += 1,
            Ok(_) => {
                gate.gap += 1;
                gate.failures.push(
                    "hkee placed without its htow prerequisite (place_building_impl ignores def.requires)"
                        .to_string(),
                );
            }
        }
    }

    // 2. Producer check: hfoo trains only at hbar, not at the town hall.
    {
        let mut sim = Sim::new(0x7EC1, 32, 32);
        sim.passive_income = false;
        sim.resources[0] = 100_000;
        if let Some(hall) = sim.spawn_def(0, "htow", cell_center(10, 10)) {
            match sim.train_def(hall, "hfoo") {
                Err(_) => gate.pass += 1,
                Ok(()) => {
                    gate.gap += 1;
                    gate.failures
                        .push("hfoo accepted at htow (producer gate missing)".to_string());
                }
            }
        } else {
            gate.fail("hall spawn failed");
        }
    }

    gate.evidence("analysis/tech-lead/PARITY_MATRIX.md G5: placement does not read Requires (PARTIAL)");
    gate.evidence("crates/wc3-sim/src/lib.rs place_building_impl / enqueue_production");
    let (pass, total) = (gate.pass, gate.total);
    gate.finish(format!("{pass}/{total} tech-tree gates enforced"))
}

// ---------------------------------------------------------------------------
// cast.spells
// ---------------------------------------------------------------------------

fn caster_for(spell: Spell) -> Option<&'static UnitDef> {
    ROSTER
        .iter()
        .find(|def| !def.is_building() && def.spells.contains(&spell))
}

fn gate_cast() -> Gate {
    let mut gate = Gate::new("cast.spells", "cast");
    let spells = Spell::ALL.to_vec();
    gate.total = spells.len();
    gate.note("implemented_spells", spells.len());

    for (index, spell) in spells.iter().enumerate() {
        let Some(caster_def) = caster_for(*spell) else {
            gate.gap += 1;
            gate.failures.push(format!(
                "{}: no roster unit owns this spell",
                spell.def().id
            ));
            continue;
        };
        let def = spell.def();
        let mut sim = Sim::new(0xCA57 + index as u64, 64, 64);
        sim.passive_income = false;
        sim.resources[0] = 100_000;
        sim.resources[1] = 100_000;

        let Some(caster) = sim.spawn_def(0, caster_def.id, cell_center(20, 20)) else {
            gate.fail(format!("{}: caster {} failed to spawn", def.id, caster_def.id));
            continue;
        };
        {
            let unit = sim.unit_mut(caster).expect("caster");
            unit.mana = unit.max_mana;
        }
        if sim.unit(caster).map(|unit| unit.max_mana).unwrap_or(0) < def.mana {
            gate.fail(format!(
                "{}: caster {} max mana {} < cost {}",
                def.id,
                caster_def.id,
                sim.unit(caster).map(|u| u.max_mana).unwrap_or(0),
                def.mana
            ));
            continue;
        }

        let is_heal = matches!(spell, Spell::HolyLight | Spell::Heal);
        let ally_def = first_unit_def();
        let enemy_def = first_unit_def();
        let ally = sim.spawn_def(0, ally_def.id, cell_center(20, 20) + Vec2::new(180.0, 0.0));
        let enemy = sim.spawn_def(1, enemy_def.id, cell_center(20, 20) + Vec2::new(300.0, 0.0));
        if ally.is_none() || enemy.is_none() {
            gate.fail(format!("{}: target spawn failed", def.id));
            continue;
        }
        let (ally, enemy) = (ally.unwrap(), enemy.unwrap());
        if is_heal {
            if let Some(unit) = sim.unit_mut(ally) {
                unit.hp = (unit.max_hp / 2).max(1);
            }
        }
        let ally_hp_before = sim.unit(ally).map(|unit| unit.hp).unwrap_or(0);
        let enemy_hp_before = sim.unit(enemy).map(|unit| unit.hp).unwrap_or(0);

        let target = match def.target {
            TargetKind::None => SpellTarget::None,
            TargetKind::Unit => SpellTarget::Unit(if is_heal { ally } else { enemy }),
            TargetKind::Point => {
                let at = sim.unit(enemy).map(|unit| unit.pos).unwrap_or(cell_center(20, 20));
                SpellTarget::Point(at)
            }
        };

        let mana_before = sim.unit(caster).map(|unit| unit.mana).unwrap_or(0);
        let cast = sim.cast(caster, *spell, target);
        if let Err(reason) = cast {
            gate.fail(format!("{}: cast -> {reason}", def.id));
            continue;
        }
        let mana_after = sim.unit(caster).map(|unit| unit.mana).unwrap_or(0);
        let cooldown = sim
            .unit(caster)
            .map(|unit| unit.spell_cd[spell.index()])
            .unwrap_or(0);
        if mana_before - mana_after != def.mana {
            gate.fail(format!(
                "{}: mana charged {}, spell costs {}",
                def.id,
                mana_before - mana_after,
                def.mana
            ));
            continue;
        }
        if cooldown == 0 && def.cooldown_ticks > 0 {
            gate.fail(format!("{}: cooldown not set", def.id));
            continue;
        }

        // A SpellCast ack is not an effect: require damage/heal or explicit state.
        let mut effect = sim
            .events
            .iter()
            .any(|event| matches!(event, Event::SpellHit { .. } | Event::SpellHeal { .. }));
        let horizon = (def.waves as u32 * def.wave_interval).max(60) + 30;
        for _ in 0..horizon {
            sim.step(&[]);
            if sim
                .events
                .iter()
                .any(|event| matches!(event, Event::SpellHit { .. } | Event::SpellHeal { .. }))
            {
                effect = true;
            }
        }
        match spell {
            Spell::HolyLight | Spell::Heal => {
                let healed = sim.unit(ally).map(|unit| unit.hp).unwrap_or(0);
                effect = effect || healed > ally_hp_before;
            }
            Spell::DivineShield => {
                let active = sim
                    .unit(caster)
                    .map(|unit| unit.invulnerable_until > sim.tick)
                    .unwrap_or(false);
                effect = effect || active;
            }
            Spell::Blizzard => {
                // Waves were queued by cast; damage lands over `waves` ticks.
                let hurt = sim.unit(enemy).map(|unit| unit.hp).unwrap_or(0) < enemy_hp_before;
                effect = effect || hurt;
            }
            _ => {
                let hurt = sim.unit(enemy).map(|unit| unit.hp).unwrap_or(0) < enemy_hp_before;
                let rooted = sim
                    .unit(enemy)
                    .map(|unit| unit.rooted_until > sim.tick)
                    .unwrap_or(false);
                effect = effect || hurt || rooted;
            }
        }
        if effect {
            gate.pass += 1;
        } else {
            gate.fail(format!(
                "{}: cast accepted but no damage/heal/state effect observed",
                def.id
            ));
        }
    }
    let (pass, total) = (gate.pass, gate.total);
    gate.finish(format!(
        "{pass}/{total} implemented spells cast with exact mana and an observed effect"
    ))
}

// ---------------------------------------------------------------------------
// cast.coverage
// ---------------------------------------------------------------------------

fn gate_ability_coverage() -> Gate {
    let mut gate = Gate::new("cast.coverage", "cast");
    let mut all: HashSet<&'static str> = HashSet::new();
    for def in ROSTER.iter() {
        for id in def.ability_ids {
            all.insert(id);
        }
    }
    // Data-backed numbers come from AbilityData.slk through gen_abilities.py;
    // every roster ability id has a GenericAbility row.  Active rows are cast
    // through `Sim::cast_data_ability`.
    let data: HashSet<&'static str> = wc3_sim::GENERIC_ABILITIES
        .iter()
        .map(|ability| ability.id)
        .collect();
    let castable = wc3_sim::GENERIC_ABILITIES
        .iter()
        .filter(|ability| ability.castable)
        .count();
    let passive = data.len().saturating_sub(castable);
    let missing: Vec<&str> = all
        .iter()
        .copied()
        .filter(|id| !data.contains(id))
        .collect();
    gate.total = all.len();
    gate.pass = all.len() - missing.len();
    gate.gap = missing.len();
    for id in missing.iter().take(40) {
        gate.failures.push(format!("ability {id} has no AbilityData row"));
    }
    gate.note("data_backed", data.len().to_string());
    gate.note("active_castable", castable.to_string());
    gate.note("passive_or_aura", passive.to_string());
    gate.note("hand_written", Spell::ALL.len().to_string());
    gate.evidence("crates/wc3-sim/src/abilities_gen.rs from analysis/war3x_data/Units/AbilityData.slk");
    gate.evidence("crates/wc3-sim/src/abilities.rs Sim::cast_data_ability");
    let (pass, total) = (gate.pass, gate.total);
    gate.finish(format!(
        "{pass}/{total} roster ability ids data-backed ({castable} active castable, {passive} passive/aura)"
    ))
}

fn gate_hero_coverage() -> Gate {
    // The 12 melee heroes from the matrix (C4/G15).
    const HEROES: [&str; 12] = [
        "Hpal", "Hamg", "Hmkg", "Ofar", "Obla", "Otch", "Udea", "Ulic", "Udre", "Ekee",
        "Emoo", "Edem",
    ];
    let mut gate = Gate::new("hero.coverage", "hero");
    gate.total = HEROES.len();
    for id in HEROES {
        if let Some(def) = def_of(id) {
            if def.stats.max_mana > 0 && def.stats.average_damage() > 0 {
                gate.pass += 1;
            } else {
                gate.fail(format!("{id}: hero def lacks mana/damage"));
            }
        } else {
            gate.gap += 1;
            gate.failures
                .push(format!("{id} ({}) is absent from ROSTER", hero_name(id)));
        }
    }
    let missing = gate.gap;
    gate.evidence("analysis/tech-lead/PARITY_MATRIX.md §2.2: 12 melee heroes, Emoo missing");
    let (pass, total) = (gate.pass, gate.total);
    gate.finish(format!("{pass}/{total} melee heroes present (missing={missing})"))
}

fn hero_name(id: &str) -> &'static str {
    match id {
        "Emoo" => "Priestess of the Moon",
        _ => "hero",
    }
}

fn gate_roster_coverage() -> Gate {
    // Exact missing melee IDs from PARITY_MATRIX §2.2.
    const EXPECTED: [&str; 4] = ["Emoo", "hmtt", "ugol", "uzg1"];
    let mut gate = Gate::new("roster.coverage", "roster");
    gate.total = EXPECTED.len();
    for id in EXPECTED {
        if def_of(id).is_some() {
            gate.pass += 1;
        } else {
            gate.gap += 1;
            gate.failures.push(format!("{id} absent from ROSTER"));
        }
    }
    gate.evidence("analysis/tech-lead/PARITY_MATRIX.md §2.2 exact missing IDs: Emoo hmtt ugol uzg1");
    let (pass, total) = (gate.pass, gate.total);
    gate.finish(format!("{pass}/{total} matrix-listed melee IDs present in ROSTER"))
}

// ---------------------------------------------------------------------------
// hero.stats / hero.leveling
// ---------------------------------------------------------------------------

fn hero_defs() -> Vec<&'static UnitDef> {
    ROSTER
        .iter()
        .filter(|def| def.stats.armor_type == ArmorType::Hero && !def.is_building())
        .collect()
}

fn gate_hero_stats() -> Gate {
    let mut gate = Gate::new("hero.stats", "hero");
    let heroes = hero_defs();
    gate.total = heroes.len();
    for (index, def) in heroes.iter().enumerate() {
        let mut sim = Sim::new(0x4E20 + index as u64, 32, 32);
        let Some(id) = sim.spawn_def(0, def.id, cell_center(10, 10)) else {
            gate.fail(format!("{}: spawn failed", def.id));
            continue;
        };
        let unit = sim.unit(id).expect("hero");
        let ok = unit.alive
            && unit.def().stats.armor_type == ArmorType::Hero
            && unit.max_hp == def.stats.hp
            && unit.max_mana == def.stats.max_mana
            && unit.max_mana > 0
            && def.stats.average_damage() > 0
            && def.stats.cost > 0;
        if ok {
            gate.pass += 1;
        } else {
            gate.fail(format!(
                "{}: hp={} mana={} avg_damage={} cost={}",
                def.id,
                unit.max_hp,
                unit.max_mana,
                def.stats.average_damage(),
                def.stats.cost
            ));
        }
    }
    let (pass, total) = (gate.pass, gate.total);
    gate.finish(format!(
        "{pass}/{total} Hero armor defs spawn with roster hp/mana/damage/cost"
    ))
}

fn gate_hero_leveling() -> Gate {
    let mut gate = Gate::new("hero.leveling", "hero");
    let heroes = hero_defs();
    gate.total = heroes.len();
    for (index, def) in heroes.iter().enumerate() {
        let mut sim = Sim::new(0x9E00 + index as u64, 32, 32);
        sim.passive_income = false;
        let Some(id) = sim.spawn_def(0, def.id, cell_center(10, 10)) else {
            gate.fail(format!("{}: spawn failed", def.id));
            continue;
        };
        let Some(start_level) = sim.hero_level(id) else {
            gate.fail(format!("{}: hero_level API absent", def.id));
            continue;
        };
        let Some((str0, agi0, int0)) = sim.hero_attributes(id) else {
            gate.fail(format!("{}: hero_attributes API absent", def.id));
            continue;
        };
        let unit = sim.unit(id).expect("hero");
        let (hp0, mana0, damage0) = (unit.max_hp, unit.max_mana, unit.damage);
        let need = wc3_sim::hero_xp_to_next(1);
        let Ok(level) = sim.add_hero_xp(id, need) else {
            gate.fail(format!("{}: add_hero_xp failed", def.id));
            continue;
        };
        let unit = sim.unit(id).expect("hero");
        let grew = sim
            .hero_attributes(id)
            .map(|(s, a, i)| s > str0 || a > agi0 || i > int0)
            .unwrap_or(false);
        let leveled = start_level == 1
            && level == 2
            && sim.hero_level(id) == Some(2)
            && sim.hero_xp(id) == Some(need)
            && grew
            && unit.max_hp > hp0
            && unit.damage > damage0
            && (mana0 == 0 || unit.max_mana > mana0);
        let set_ok = sim.set_hero_level(id, 6).is_ok()
            && sim.hero_level(id) == Some(6)
            && sim.hero_xp(id).unwrap_or(0) >= wc3_sim::hero_xp_for_level(6);
        let worker = sim
            .spawn_def(0, "hpea", cell_center(14, 10))
            .expect("worker");
        let rejects_non_hero = sim.add_hero_xp(worker, 100).is_err();
        if leveled && set_ok && rejects_non_hero {
            gate.pass += 1;
        } else {
            gate.fail(format!(
                "{}: level={:?} xp={:?} grew={grew} hp0={hp0} hp={} dmg0={damage0} dmg={} set6={set_ok} non_hero_reject={rejects_non_hero}",
                def.id,
                sim.hero_level(id),
                sim.hero_xp(id),
                sim.unit(id).map(|unit| unit.max_hp).unwrap_or(0),
                sim.unit(id).map(|unit| unit.damage).unwrap_or(0),
            ));
        }
    }
    let (pass, total) = (gate.pass, gate.total);
    gate.finish(format!(
        "{pass}/{total} heroes level through XP with attribute growth"
    ))
}

// ---------------------------------------------------------------------------
// items.data / items.runtime
// ---------------------------------------------------------------------------



fn item_tree(assets: &Path) -> Result<(usize, usize, usize, Vec<String>), String> {
    let data = GameData::load(&DirSource::new(assets)).map_err(|error| error.to_string())?;
    let mut total = 0usize;
    let mut pass = 0usize;
    let mut class_present = 0usize;
    let mut failures = Vec::new();
    for item in data.items() {
        total += 1;
        let id = item.id_str();
        if item.class().is_some() {
            class_present += 1;
        }
        let level = item.level().ok().flatten();
        let cost = item.gold_cost().ok().flatten();
        let usable = item.usable().ok();
        let ok = id.len() == 4 && level.is_some() && cost.is_some() && usable.is_some();
        if ok {
            pass += 1;
        } else if failures.len() < 20 {
            failures.push(format!(
                "{id}: level={level:?} cost={cost:?} usable={usable:?}"
            ));
        }
    }
    Ok((total, pass, class_present, failures))
}

fn gate_items_data(roc: &Path, tft: &Path) -> Gate {
    let mut gate = Gate::new("items.data", "items");
    let primary = if tft.is_dir() { tft } else { roc };
    match item_tree(primary) {
        Ok((total, pass, class_present, failures)) => {
            gate.total = total;
            gate.pass = pass;
            gate.fail = total - pass;
            gate.failures.extend(failures);
            gate.note("class_present", class_present);
        }
        Err(error) => {
            gate.total = 1;
            gate.fail = 1;
            gate.failures.push(format!("GameData::load -> {error}"));
        }
    }
    if let Ok((roc_total, roc_pass, roc_class, _)) = item_tree(roc) {
        gate.note("roc_items", roc_total);
        gate.note("roc_pass", roc_pass);
        gate.note("roc_class_present", roc_class);
    }
    gate.note("tft_items", gate.total);
    gate.evidence("analysis/war3x_data/Units/ItemData.slk (TFT, primary)");
    gate.evidence("analysis/war3_data/Units/ItemData.slk (RoC)");
    let (pass, total) = (gate.pass, gate.total);
    gate.finish(format!(
        "{pass}/{total} TFT item rows carry id/level/cost/usable"
    ))
}

fn gate_items_schema(roc: &Path, tft: &Path) -> Gate {
    let mut gate = Gate::new("items.schema", "items");
    for (label, tree) in [("roc", roc), ("tft", tft)] {
        gate.total += 1;
        if !tree.is_dir() {
            gate.fail(format!("{label}: tree absent {}", tree.display()));
            continue;
        }
        match item_tree(tree) {
            Ok((total, _, class_present, _)) => {
                if total > 0 && class_present == total {
                    gate.pass += 1;
                } else {
                    gate.fail(format!(
                        "{label}: class column exposed for {class_present}/{total} items"
                    ));
                }
            }
            Err(error) => gate.fail(format!("{label}: GameData::load -> {error}")),
        }
    }
    gate.evidence("crates/wc3-data/src/items.rs:99-101: class() reads the RoC column name `itemClass`");
    gate.evidence("analysis/war3x_data/Units/ItemData.slk: TFT column is `class`");
    let (pass, total) = (gate.pass, gate.total);
    gate.finish(format!("{pass}/{total} item trees expose the class column"))
}



fn gate_items_gaps() -> Gate {
    use wc3_sim::data::{item_def, ITEMS};
    let mut gate = Gate::new("items.gaps", "items");

    // 1. Every item with an `abilList` id has data-backed ability numbers.
    gate.total += 1;
    let ability_items = ITEMS
        .iter()
        .filter(|def| !def.ability_id.is_empty())
        .count();
    let covered = ITEMS
        .iter()
        .filter(|def| {
            !def.ability_id.is_empty() && wc3_sim::generic_ability(def.ability_id).is_some()
        })
        .count();
    if covered == ability_items {
        gate.pass += 1;
    } else {
        gate.fail(format!(
            "{}/{} ability items have AbilityData numbers",
            covered, ability_items
        ));
    }
    gate.note("ability_items", ability_items.to_string());
    gate.note("ability_covered", covered.to_string());
    gate.note("active_castable", wc3_sim::data_castable_ids().count().to_string());

    // 2. Shop purchase: Requires=gating, gold debit, stock and charges.
    gate.total += 1;
    let mut sim = Sim::new(0x1A00, 48, 48);
    sim.passive_income = false;
    sim.resources[0] = 5_000;
    let shop = sim
        .spawn_def(2, "ngme", cell_center(10, 10))
        .expect("goblin merchant");
    let buyer = sim
        .spawn_def(0, "hfoo", cell_center(11, 10))
        .expect("buyer");
    sim.step(&[]);
    let before = sim.resources[0];
    // Scroll of Town Portal requires TWN2 (hkee/ostr/etoa/unp1).
    let blocked = sim.buy_item(buyer, shop, "stwp").is_err();
    sim.spawn_def(0, "hkee", cell_center(14, 10));
    sim.step(&[]);
    let bought = sim.buy_item(buyer, shop, "stwp");
    let gold_ok = before - sim.resources[0] == 350;
    let charges_ok = bought
        .as_ref()
        .ok()
        .and_then(|id| sim.item(*id))
        .map(|item| item.charges == 1)
        .unwrap_or(false);
    let stock_ok = sim
        .shop(shop)
        .and_then(|shop| shop.slots.iter().find(|slot| slot.item == "stwp"))
        .map(|slot| slot.stock == 1)
        .unwrap_or(false);
    if blocked && bought.is_ok() && gold_ok && charges_ok && stock_ok {
        gate.pass += 1;
    } else {
        gate.fail(format!(
            "shop: blocked={blocked} bought={:?} gold_ok={gold_ok} charges={charges_ok} stock={stock_ok}",
            bought.err()
        ));
    }
    gate.note("shop", "ngme/stwp TWN2 gating + stock/charges");

    // 3. Requires/DependencyOr rows parsed from ItemFunc (18 in this install).
    gate.total += 1;
    let requirements = wc3_sim::data::ITEM_REQUIREMENTS.len();
    let stwp_requires_twn2 = wc3_sim::data::item_requirements("stwp")
        .any(|requirement| requirement.token == "TWN2" && requirement.alternatives.len() == 4);
    if requirements == 18 && stwp_requires_twn2 {
        gate.pass += 1;
    } else {
        gate.fail(format!(
            "requirements={requirements} stwp_twn2={stwp_requires_twn2}"
        ));
    }
    gate.note("requirements", requirements.to_string());

    // 4. Recipe path exists; retail has zero item-to-item Requires rows.
    gate.total += 1;
    let recipe_rows = wc3_sim::data::RECIPES.len();
    let engine = {
        let mut sim = Sim::new(0x1A01, 32, 32);
        let unit = sim
            .spawn_def(0, "hfoo", cell_center(10, 10))
            .expect("unit");
        matches!(sim.combine_items(unit, "ratf"), Err("no recipe data"))
    };
    if recipe_rows == 0 && engine {
        gate.pass += 1;
    } else {
        gate.fail(format!(
            "recipe_rows={recipe_rows} engine_guard={engine}"
        ));
    }
    gate.note("recipe_rows", recipe_rows.to_string());
    gate.note("recipes", "0 retail rows; Sim::combine_items engine present");
    gate.evidence("analysis/war3x_data/Units/ItemFunc.txt Requires/DependencyOr (18 gated items, 0 item->item)");
    gate.evidence("crates/wc3-sim/src/abilities_gen.rs + Sim::cast_data_ability");
    gate.evidence("crates/wc3-sim/src/lib.rs buy_item/use_item_on/combine_items");
    let (pass, total) = (gate.pass, gate.total);
    gate.finish(format!("{pass}/{total} item gameplay checks passed"))
}

fn fresh_sim(seed: u64) -> (Sim, UnitId) {
    let mut sim = Sim::new(seed, 32, 32);
    sim.passive_income = false;
    let unit = sim
        .spawn_def(0, "hfoo", cell_center(10, 10))
        .expect("footman def");
    (sim, unit)
}

fn gate_items_runtime() -> Gate {
    use wc3_sim::data::{item_def, ITEMS};

    let mut gate = Gate::new("items.runtime", "items");
    gate.note("item_defs", ITEMS.len());
    gate.note(
        "ability_items",
        ITEMS.iter().filter(|def| !def.ability_id.is_empty()).count(),
    );

    // 0. Generated table shape.
    gate.total += 1;
    let malformed = ITEMS
        .iter()
        .filter(|def| def.id.len() != 4 || def.name.is_empty() || def.level < 0 || def.cost < 0)
        .count();
    if malformed == 0 {
        gate.pass += 1;
    } else {
        gate.fail(format!("{malformed} generated item defs are malformed"));
    }

    let claws = item_def("ratf").expect("Claws of Attack in ITEMS");

    // 1. pickup applies the passive bonus.
    {
        gate.total += 1;
        let (mut sim, unit) = fresh_sim(0x17E0);
        let damage_before = sim.unit(unit).expect("unit").damage;
        let pos = sim.unit(unit).expect("unit").pos;
        let item = sim.spawn_item("ratf", pos, 0);
        let picked = item.map(|id| sim.pickup_item(unit, id)).transpose();
        let damage_after = sim.unit(unit).expect("unit").damage;
        let held = sim.unit(unit).expect("unit").items.len();
        if picked.is_ok()
            && damage_after == damage_before + claws.bonus_attack
            && held == 1
        {
            gate.pass += 1;
        } else {
            gate.fail(format!(
                "pickup: damage {damage_before}->{damage_after}, held={held}"
            ));
        }
    }

    // 2. drop removes the passive bonus.
    {
        gate.total += 1;
        let (mut sim, unit) = fresh_sim(0x17E1);
        let base = sim.unit(unit).expect("unit").damage;
        let pos = sim.unit(unit).expect("unit").pos;
        let Some(item) = sim.spawn_item("ratf", pos, 0) else {
            gate.fail("spawn_item(ratf) failed");
            let (pass, total) = (gate.pass, gate.total);
            return gate.finish(format!("{pass}/{total} item runtime checks passed"));
        };
        let _ = sim.pickup_item(unit, item);
        let dropped = sim.drop_item(unit, item);
        let after = sim.unit(unit).expect("unit").damage;
        if dropped.is_ok() && after == base {
            gate.pass += 1;
        } else {
            gate.fail(format!("drop: {dropped:?}, damage {base}->{after}"));
        }
    }

    // 3. six-slot inventory limit.
    {
        gate.total += 1;
        let (mut sim, unit) = fresh_sim(0x17E2);
        let pos = sim.unit(unit).expect("unit").pos;
        let mut ids = Vec::new();
        for _ in 0..7 {
            if let Some(id) = sim.spawn_item("ratf", pos, 0) {
                ids.push(id);
            }
        }
        let mut picked = 0usize;
        let mut overflow = None;
        for id in ids {
            match sim.pickup_item(unit, id) {
                Ok(()) => picked += 1,
                Err(reason) => {
                    overflow = Some(reason);
                    break;
                }
            }
        }
        if picked == 6 && overflow == Some("inventory full") {
            gate.pass += 1;
        } else {
            gate.fail(format!("inventory limit: picked={picked}, overflow={overflow:?}"));
        }
    }

    // 4. distance rule.
    {
        gate.total += 1;
        let (mut sim, unit) = fresh_sim(0x17E3);
        let far = cell_center(20, 20) + Vec2::new(600.0, 0.0);
        let Some(item) = sim.spawn_item("ratf", far, 0) else {
            gate.fail("spawn_item(ratf) failed");
            let (pass, total) = (gate.pass, gate.total);
            return gate.finish(format!("{pass}/{total} item runtime checks passed"));
        };
        match sim.pickup_item(unit, item) {
            Err("item too far") => gate.pass += 1,
            other => gate.fail(format!("distance rule: pickup={other:?}")),
        }
    }

    // 5. consumable use: heal, charge decrement, removal at zero.
    {
        gate.total += 1;
        let potion = ITEMS
            .iter()
            .find(|def| def.heal > 0 && def.charges > 0)
            .expect("a healing consumable in ITEMS");
        let (mut sim, unit) = fresh_sim(0x17E4);
        let pos = sim.unit(unit).expect("unit").pos;
        let Some(item) = sim.spawn_item(potion.id, pos, 2) else {
            gate.fail(format!("spawn_item({}) failed", potion.id));
            let (pass, total) = (gate.pass, gate.total);
            return gate.finish(format!("{pass}/{total} item runtime checks passed"));
        };
        let _ = sim.pickup_item(unit, item);
        {
            let unit_ref = sim.unit_mut(unit).expect("unit");
            unit_ref.hp = (unit_ref.max_hp / 3).max(1);
        }
        let hp_before = sim.unit(unit).expect("unit").hp;
        let first = sim.use_item(unit, item);
        let hp_after = sim.unit(unit).expect("unit").hp;
        let charges = sim.item(item).map(|entry| entry.charges);
        let second = sim.use_item(unit, item);
        let gone = sim.item(item).is_none()
            && !sim.unit(unit).expect("unit").items.contains(&item);
        if first.is_ok() && hp_after > hp_before && charges == Some(1) && second.is_ok() && gone {
            gate.pass += 1;
        } else {
            gate.fail(format!(
                "consumable {}: first={first:?} hp {hp_before}->{hp_after} charges={charges:?} gone={gone}",
                potion.id
            ));
        }
    }

    // 6. passive-only items are rejected by use_item.
    {
        gate.total += 1;
        let (mut sim, unit) = fresh_sim(0x17E5);
        let pos = sim.unit(unit).expect("unit").pos;
        let Some(item) = sim.spawn_item("ratf", pos, 0) else {
            gate.fail("spawn_item(ratf) failed");
            let (pass, total) = (gate.pass, gate.total);
            return gate.finish(format!("{pass}/{total} item runtime checks passed"));
        };
        let _ = sim.pickup_item(unit, item);
        match sim.use_item(unit, item) {
            Err("item has no implemented effect") => gate.pass += 1,
            other => gate.fail(format!("passive use_item returned {other:?}")),
        }
    }

    gate.evidence("crates/wc3-sim/src/lib.rs:1814 spawn_item / :1835 pickup_item / :1869 drop_item / :1899 use_item");
    gate.evidence("crates/wc3-sim/src/items_gen.rs (273 generated defs)");
    let (pass, total) = (gate.pass, gate.total);
    gate.finish(format!(
        "{pass}/{total} item runtime checks passed ({} defs)",
        ITEMS.len()
    ))
}

// ---------------------------------------------------------------------------
// triggers.jass
// ---------------------------------------------------------------------------

const TRIGGER_SCRIPT: &str = r#"
globals
    trigger t
    integer n = 0
endglobals
function action takes nothing returns nothing
    set n = n + 1
endfunction
function main takes nothing returns nothing
    set t = CreateTrigger()
    call TriggerAddAction(t, function action)
    call TriggerRegisterTimerEvent(t, 0.5, true)
endfunction
"#;

fn map_files(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("w3m" | "w3x" | "w3n")
            ) {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort();
    out
}

fn gate_triggers(maps_root: &Path) -> Gate {
    let mut gate = Gate::new("triggers.jass", "triggers");
    // Synthetic trigger: parse, register, tick twice.
    match wc3_jass::parse(TRIGGER_SCRIPT) {
        Ok(program) => {
            let mut runtime = wc3_jass::JassRuntime::new(program, 1);
            if let Err(error) = runtime.run_function("main") {
                gate.fail(format!("synthetic trigger main -> {error}"));
            } else {
                runtime.tick(0.5);
                runtime.tick(0.5);
                let count = runtime.globals.get("n").cloned();
                if count == Some(wc3_jass::Value::Int(2)) && runtime.triggers.len() == 1 {
                    gate.pass += 1;
                } else {
                    gate.fail(format!(
                        "synthetic trigger fired count={count:?} triggers={}",
                        runtime.triggers.len()
                    ));
                }
            }
        }
        Err(error) => gate.fail(format!("synthetic trigger parse -> {error}")),
    }
    gate.total += 1;

    // Retail map scripts: parse-only (never executed).
    let maps = map_files(maps_root);
    let mut map_functions = 0usize;
    gate.note("maps", maps.len());
    for path in &maps {
        gate.total += 1;
        let name = path
            .strip_prefix(maps_root)
            .unwrap_or(path)
            .display()
            .to_string();
        match MapBundle::open(path) {
            Ok(bundle) => match bundle.script.as_deref() {
                Some(script) => match wc3_jass::parse(script) {
                    Ok(program) => {
                        gate.pass += 1;
                        map_functions += program.functions.len();
                    }
                    Err(error) => gate.fail(format!("{name}: parse -> {error}")),
                },
                None => gate.fail(format!("{name}: no war3map.j")),
            },
            Err(error) => gate.fail(format!("{name}: MapBundle::open -> {error}")),
        }
    }
    gate.note("map_functions", map_functions);
    gate.evidence("crates/wc3-jass/src/lib.rs:448-460 (bare return consumes the next line)");
    gate.evidence("runtime engine: analysis/agent-re/scan.json map_main_ok 44/44 (wc3-re::jass; separate component)");
    let (pass, total, map_count) = (gate.pass, gate.total, maps.len());
    gate.finish(format!(
        "{pass}/{total} JASS scripts parsed (1 synthetic trigger + {map_count} retail maps)"
    ))
}

/// `return` with no value is legal JASS. The parser currently calls
/// `skip_newlines()` before checking for the newline, so the next token
/// (`endfunction`/`set`/`endif`) is consumed as the return expression.
fn gate_trigger_return() -> Gate {
    let mut gate = Gate::new("triggers.return", "triggers");
    let scripts = [
        (
            "bare return at end",
            "function f takes nothing returns nothing\n    return\nendfunction\n",
        ),
        (
            "bare return before statement",
            "function f takes nothing returns nothing\n    return\n    set x = 1\nendfunction\n",
        ),
        (
            "bare return in if",
            "function f takes nothing returns nothing\n    if a then\n        return\n    endif\nendfunction\n",
        ),
    ];
    for (label, script) in scripts {
        match wc3_jass::parse(script) {
            Ok(_) => gate.pass += 1,
            Err(error) => gate.fail(format!("{label}: parse -> {error}")),
        }
        gate.total += 1;
    }
    gate.evidence("crates/wc3-jass/src/lib.rs:448-460: skip_newlines() precedes the Newline check");
    gate.evidence("wc3-jass is the standalone lexer/parser/interpreter; the game engine uses wc3-re::jass");
    gate.finish("bare return statements parse (no value consumed from the next line)")
}

/// `elseif` chains: later branches must not replace earlier ones.
fn gate_trigger_elseif() -> Gate {
    let mut gate = Gate::new("triggers.elseif", "triggers");
    gate.total = 1;
    let script = r#"
globals
    integer n = 0
endglobals
function main takes nothing returns nothing
    if false then
        set n = 1
    elseif true then
        set n = 2
    elseif true then
        set n = 3
    endif
endfunction
"#;
    match wc3_jass::parse(script) {
        Ok(program) => {
            let mut runtime = wc3_jass::JassRuntime::new(program, 1);
            match runtime.run_function("main") {
                Ok(_) => {
                    let value = runtime.globals.get("n").cloned();
                    if value == Some(wc3_jass::Value::Int(2)) {
                        gate.pass += 1;
                    } else {
                        gate.fail(format!(
                            "elseif chain n={value:?}, expected 2 (first true branch wins)"
                        ));
                    }
                }
                Err(error) => gate.fail(format!("elseif script run -> {error}")),
            }
        }
        Err(error) => gate.fail(format!("elseif script parse -> {error}")),
    }
    gate.evidence("crates/wc3-jass/src/lib.rs:431: else_branch is overwritten per elseif");
    gate.evidence("wc3-jass is the standalone lexer/parser/interpreter; the game engine uses wc3-re::jass");
    gate.finish("elseif chains keep the first true branch")
}

// ---------------------------------------------------------------------------
// top-6 matrix increments: lumber, creeps, map units
// ---------------------------------------------------------------------------

fn gate_lumber() -> Gate {
    let mut gate = Gate::new("economy.lumber", "economy");
    gate.total = 3;

    // 1. worker chops a tree and deposits lumber at the hall.
    {
        let mut sim = Sim::new(0x1AB0, 64, 64);
        sim.passive_income = false;
        sim.resources[0] = 100_000;
        sim.lumber[0] = 0;
        sim.spawn_def(0, "htow", cell_center(10, 10));
        let tree_pos = cell_center(14, 10);
        sim.spawn_destructable("ATtr", tree_pos);
        let Some(worker) = sim.spawn_def(0, "hpea", cell_center(12, 10)) else {
            gate.fail("worker spawn failed");
            let (pass, total) = (gate.pass, gate.total);
            return gate.finish(format!("{pass}/{total} lumber checks passed"));
        };
        sim.harvest_lumber(&[worker], tree_pos);
        let mut deposited = 0i32;
        for _ in 0..3_000 {
            sim.step(&[]);
            for event in &sim.events {
                if let Event::LumberDeposited { owner, amount } = event {
                    if *owner == 0 {
                        deposited += *amount;
                    }
                }
            }
            if deposited > 0 {
                break;
            }
        }
        if deposited > 0 && sim.lumber[0] >= 10 {
            gate.pass += 1;
        } else {
            gate.fail(format!(
                "harvest loop: deposited={deposited}, lumber={}",
                sim.lumber[0]
            ));
        }
    }

    // 2. a building with a lumber cost is denied at zero lumber.
    {
        let mut sim = Sim::new(0x1AB1, 32, 32);
        sim.passive_income = false;
        sim.resources[0] = 100_000;
        sim.lumber[0] = 0;
        let Some(worker) = sim.spawn_def(0, "hpea", cell_center(8, 8)) else {
            gate.fail("worker spawn failed");
            let (pass, total) = (gate.pass, gate.total);
            return gate.finish(format!("{pass}/{total} lumber checks passed"));
        };
        if let Some(kee) = def_of("hkee") {
            if let Err(reason) = spawn_prerequisites(&mut sim, 0, kee, (18, 18), 0) {
                gate.fail(reason);
                let (pass, total) = (gate.pass, gate.total);
                return gate.finish(format!("{pass}/{total} lumber checks passed"));
            }
        }
        let site = find_site(&sim, (11, 8), 4).unwrap_or(cell_center(11, 8));
        match sim.place_building(worker, "hkee", site) {
            Err("not enough lumber") => gate.pass += 1,
            other => gate.fail(format!("zero-lumber build returned {other:?}")),
        }
    }

    // 3. a successful build debits the exact lumber cost.
    {
        let mut sim = Sim::new(0x1AB2, 32, 32);
        sim.passive_income = false;
        sim.resources[0] = 100_000;
        sim.lumber[0] = 1_000;
        let Some(worker) = sim.spawn_def(0, "hpea", cell_center(8, 8)) else {
            gate.fail("worker spawn failed");
            let (pass, total) = (gate.pass, gate.total);
            return gate.finish(format!("{pass}/{total} lumber checks passed"));
        };
        if let Some(kee) = def_of("hkee") {
            if let Err(reason) = spawn_prerequisites(&mut sim, 0, kee, (18, 18), 0) {
                gate.fail(reason);
                let (pass, total) = (gate.pass, gate.total);
                return gate.finish(format!("{pass}/{total} lumber checks passed"));
            }
        }
        let site = find_site(&sim, (11, 8), 4).unwrap_or(cell_center(11, 8));
        let before = sim.lumber[0];
        let cost = def_of("hkee").map(|def| def.stats.lumber_cost).unwrap_or(0);
        match sim.place_building(worker, "hkee", site) {
            Ok(_) if cost > 0 && before - sim.lumber[0] == cost => gate.pass += 1,
            Ok(_) => gate.fail(format!(
                "build lumber debit {} != cost {cost}",
                before - sim.lumber[0]
            )),
            Err(reason) => gate.fail(format!("funded build -> {reason}")),
        }
    }

    gate.evidence("crates/wc3-sim/src/lib.rs HarvestLumber / LumberDeposited / deposit_lumber");
    gate.evidence("analysis/tech-lead/PARITY_MATRIX.md G2 (lumber economy)");
    let (pass, total) = (gate.pass, gate.total);
    gate.finish(format!("{pass}/{total} lumber economy checks passed"))
}

fn gate_creeps() -> Gate {
    let mut gate = Gate::new("creeps.camps", "creeps");
    gate.total = 3;
    let mut sim = Sim::new(0xC4E9, 64, 64);
    sim.passive_income = false;
    let anchor = cell_center(24, 24);
    let camp = sim.spawn_camp(&["nftr", "nban"], anchor, 500.0, 900.0);
    let Some(camp_id) = camp else {
        gate.fail("spawn_camp returned None (creep defs missing)");
        let (pass, total) = (gate.pass, gate.total);
        return gate.finish(format!("{pass}/{total} creep camp checks passed"));
    };
    let camp_units: Vec<UnitId> = sim
        .camps
        .iter()
        .find(|camp| camp.id == camp_id)
        .map(|camp| camp.units.clone())
        .unwrap_or_default();

    // 1. camp spawned the requested units under owner 2.
    if camp_units.len() == 2
        && camp_units.iter().all(|id| {
            sim.unit(*id)
                .map(|unit| unit.owner == 2 && unit.alive)
                .unwrap_or(false)
        })
    {
        gate.pass += 1;
    } else {
        gate.fail(format!("camp units {camp_units:?} (expected 2 owner-2 units)"));
    }

    // 2. aggression: a player unit inside the radius pulls the camp.
    let intruder = sim.spawn_def(0, "hfoo", anchor + Vec2::new(300.0, 0.0));
    let Some(intruder) = intruder else {
        gate.fail("intruder spawn failed");
        let (pass, total) = (gate.pass, gate.total);
        return gate.finish(format!("{pass}/{total} creep camp checks passed"));
    };
    for _ in 0..10 {
        sim.step(&[]);
    }
    let engaged = camp_units.iter().any(|id| {
        sim.unit(*id)
            .map(|unit| matches!(unit.order, Order::Attack(target) if target == intruder))
            .unwrap_or(false)
    });
    if engaged {
        gate.pass += 1;
    } else {
        gate.fail("camp did not acquire a target within 500 units");
    }

    // 3. leash: a target beyond the leash radius is dropped and the camp returns.
    if let Some(unit) = sim.unit_mut(intruder) {
        unit.pos = anchor + Vec2::new(2_000.0, 0.0);
        unit.order = Order::Idle;
    }
    let mut saw_return = false;
    for _ in 0..180 {
        sim.step(&[]);
        if camp_units.iter().any(|id| {
            sim.unit(*id)
                .map(|unit| matches!(unit.order, Order::Move(target) if target.distance(anchor) < 1.0))
                .unwrap_or(false)
        }) {
            saw_return = true;
        }
    }
    let max_distance = camp_units
        .iter()
        .filter_map(|id| sim.unit(*id).map(|unit| unit.pos.distance(anchor)))
        .fold(0.0f32, f32::max);
    if saw_return && max_distance <= 950.0 {
        gate.pass += 1;
    } else {
        gate.fail(format!(
            "leash: saw_return={saw_return} max_distance={max_distance:.0} (limit 950)"
        ));
    }

    gate.evidence("crates/wc3-sim/src/lib.rs spawn_camp / Camp / advance_creeps");
    gate.evidence("analysis/tech-lead/PARITY_MATRIX.md G18 (creep camps)");
    let (pass, total) = (gate.pass, gate.total);
    gate.finish(format!("{pass}/{total} creep camp checks passed"))
}

fn gate_map_units(maps_root: &Path) -> Gate {
    let mut gate = Gate::new("map.units.spawn", "maps");
    gate.total = 2;
    let map_path = maps_root.join("Campaign").join("Human04.w3m");
    match MapBundle::open(&map_path) {
        Ok(bundle) => {
            let units = bundle.units.as_ref().map(|units| units.len()).unwrap_or(0);
            if units > 0 {
                gate.pass += 1;
                gate.note("human04_units", units);
            } else {
                gate.fail(format!(
                    "war3mapUnits.doo yields 0 units; errors={:?}",
                    bundle.errors
                ));
            }
        }
        Err(error) => gate.fail(format!("Human04 open -> {error}")),
    }
    // The spawn API the game will call.
    let mut sim = Sim::new(0x3A90, 32, 32);
    let spawned = sim.spawn_map_units(&[(0, "hfoo", cell_center(6, 6))]);
    if spawned.len() == 1 {
        gate.pass += 1;
    } else {
        gate.fail("Sim::spawn_map_units did not spawn an entry");
    }
    gate.evidence("crates/wc3-sim/src/lib.rs spawn_map_units; crates/wc3-game/src/model_render.rs:563 consumes bundle.units");
    gate.evidence("analysis/tech-lead/PARITY_MATRIX.md G22 (map-placed units)");
    let (pass, total) = (gate.pass, gate.total);
    gate.finish(format!("{pass}/{total} map-unit checks passed"))
}

// ---------------------------------------------------------------------------
// audio.game
// ---------------------------------------------------------------------------

struct DirProvider {
    index: HashMap<String, PathBuf>,
    cache: HashMap<String, Arc<Sound>>,
}

fn path_key(path: &str) -> String {
    path.replace('/', "\\").to_ascii_lowercase()
}

fn index_tree(root: &Path) -> HashMap<String, PathBuf> {
    fn walk(base: &Path, dir: &Path, out: &mut HashMap<String, PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(base, &path, out);
            } else if let Ok(relative) = path.strip_prefix(base) {
                out.insert(path_key(&relative.to_string_lossy()), path);
            }
        }
    }
    let mut out = HashMap::new();
    walk(root, root, &mut out);
    out
}

impl SoundProvider for DirProvider {
    fn load_sound(&mut self, path: &str) -> Option<Arc<Sound>> {
        let key = path_key(path);
        if let Some(sound) = self.cache.get(&key) {
            return Some(sound.clone());
        }
        let file = self.index.get(&key)?.clone();
        let bytes = fs::read(file).ok()?;
        let sound = Arc::new(decode_wav(&bytes).ok()?);
        self.cache.insert(key, sound.clone());
        Some(sound)
    }

    fn load_bytes(&mut self, path: &str) -> Option<Arc<[u8]>> {
        let file = self.index.get(&path_key(path))?.clone();
        fs::read(file).ok().map(Arc::from)
    }

    fn exists(&self, path: &str) -> bool {
        self.index.contains_key(&path_key(path))
    }
}

fn gate_audio(assets: &Path) -> Gate {
    let mut gate = Gate::new("audio.game", "audio");
    gate.total = 6;
    let index = index_tree(assets);
    gate.note("indexed_files", index.len());

    let sound_dir = assets.join("UI").join("SoundInfo");
    let mut sounds = SoundInfoDb::new();
    let mut tables = 0usize;
    if let Ok(entries) = fs::read_dir(&sound_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("slk") {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("table");
            if let Ok(bytes) = fs::read(&path) {
                if sounds.add_table(name, &bytes).is_ok() {
                    tables += 1;
                }
            }
        }
    }
    gate.note("sound_tables", tables);
    gate.note("sound_defs", sounds.len());
    if tables == 0 || sounds.is_empty() {
        gate.fail("no SoundInfo tables loaded");
    } else {
        gate.pass += 1;
    }

    let unit_names = fs::read(assets.join("Units").join("unitUI.slk"))
        .ok()
        .and_then(|bytes| UnitNameMap::parse(&bytes).ok())
        .unwrap_or_default();
    gate.note("unit_names", unit_names.len());

    // A definition is playable only when one variant really decodes, not just
    // when its path exists in the tree (mp3/unsupported WAV variants exist).
    let playable = sounds.iter().find_map(|def| {
        for path in def.paths() {
            let Some(file) = index.get(&path_key(&path)) else {
                continue;
            };
            let Ok(bytes) = fs::read(file) else {
                continue;
            };
            if decode_wav(&bytes).is_ok() {
                return Some(def.name.clone());
            }
        }
        None
    });
    let provider = DirProvider {
        index,
        cache: HashMap::new(),
    };
    let mut game = GameAudio::new(
        AudioEngine::headless(AudioConfig::default()),
        sounds.clone(),
        unit_names.clone(),
        provider,
    );

    // 1. UI click resolves a real InterfaceClick file.
    match game.ui_click() {
        Some(_) => gate.pass += 1,
        None => gate.fail("ui_click returned no voice"),
    }

    // 2. A real unit acknowledgement.
    let mut ack = None;
    for id in ["hpea", "opeo", "uaco", "ewsp"] {
        if let Some(voice) = game.unit_ack(id, None) {
            ack = Some((id, voice));
            break;
        }
    }
    match ack {
        Some(_) => gate.pass += 1,
        None => gate.fail(format!(
            "no unit ack started (unit_names={})",
            unit_names.len()
        )),
    }

    // 3. Named sound lookup: use a definition whose archive file exists in the
    //    extracted tree, so the gate proves the full table -> path -> decode
    //    chain instead of hitting a stale DirectoryBase entry.

    match playable {
        Some(name) => match game.play_sound(&name, None) {
            Some(_) => gate.pass += 1,
            None => gate.fail(format!("play_sound({name}) returned no voice")),
        },
        None => gate.fail("no sound definition resolves to an existing archive file"),
    }

    // 4. Unknown sound names are rejected without starting a voice.
    let missing = game.play_sound("__parity_missing__", None);
    if missing.is_none() {
        gate.pass += 1;
    } else {
        gate.fail(format!("unknown sound started a voice: {missing:?}"));
    }

    // 5. Headless mixer actually renders the started voices.
    let config = AudioConfig::default();
    let mut buffer = vec![0.0f32; config.block_samples() * config.channels as usize];
    game.engine().render(&mut buffer);
    let stats = game.audio_stats();
    gate.note("voices_started", stats.voices_started);
    gate.note("frames_rendered", stats.frames_rendered);
    if stats.voices_started >= 2 && stats.frames_rendered > 0 {
        gate.pass += 1;
    } else {
        gate.fail(format!(
            "mixer stats voices_started={} frames={}",
            stats.voices_started, stats.frames_rendered
        ));
    }

    gate.evidence("analysis/war3_full3/UI/SoundInfo/*.slk");
    gate.evidence("analysis/war3_full3/Sound/**.wav (real archive files)");
    let (pass, total) = (gate.pass, gate.total);
    gate.finish(format!(
        "{pass}/{total} audio checks passed ({tables} tables, {} defs, {} unit names)",
        sounds.len(),
        unit_names.len()
    ))
}

// ---------------------------------------------------------------------------
// menu
// ---------------------------------------------------------------------------

const CORE_FDF: &[&str] = &[
    "UI\\FrameDef\\UI\\ConsoleUI.fdf",
    "UI\\FrameDef\\UI\\ResourceBar.fdf",
    "UI\\FrameDef\\UI\\SimpleInfoPanel.fdf",
    "UI\\FrameDef\\UI\\InfoPanelUnitDetail.fdf",
    "UI\\FrameDef\\UI\\UpperButtonBar.fdf",
    "UI\\FrameDef\\UI\\EscMenuTemplates.fdf",
    "UI\\FrameDef\\UI\\InfoPanelTemplates.fdf",
    "UI\\FrameDef\\GlobalStrings.fdf",
    "UI\\FrameDef\\DateStrings.fdf",
    "UI\\FrameDef\\NetworkStrings.fdf",
];

fn gate_menu(assets: &Path) -> Gate {
    let mut gate = Gate::new("menu.fdf", "menu");
    let index = index_tree(assets);
    gate.total = CORE_FDF.len();

    let mut resolved_docs = Vec::new();
    for doc in CORE_FDF {
        match index.get(&path_key(doc)) {
            Some(path) => match fs::read_to_string(path) {
                Ok(text) => match FdfDocument::parse(&text) {
                    Ok(document) => {
                        gate.pass += 1;
                        resolved_docs.push((doc, document));
                    }
                    Err(error) => gate.fail(format!("{doc}: parse -> {error}")),
                },
                Err(error) => gate.fail(format!("{doc}: read -> {error}")),
            },
            None => gate.fail(format!("{doc}: missing from extracted tree")),
        }
    }

    // Includes must resolve inside the archive tree.
    let mut includes = 0usize;
    let mut missing_includes = 0usize;
    for (doc, document) in &resolved_docs {
        for include in &document.includes {
            includes += 1;
            if !index.contains_key(&path_key(include)) {
                missing_includes += 1;
                if gate.failures.len() < 40 {
                    gate.failures.push(format!("{doc}: include {include} missing"));
                }
            }
        }
    }
    gate.total += includes;
    gate.pass += includes.saturating_sub(missing_includes);
    gate.fail += missing_includes;
    gate.note("includes", includes);
    gate.note("missing_includes", missing_includes);
    gate.note(
        "core_nodes",
        resolved_docs.iter().map(|(_, doc)| doc.nodes.len()).sum::<usize>(),
    );

    // Every FDF in the retail tree (menu screens + glue screens) must parse.
    let mut fdf_total = 0usize;
    let mut fdf_failed = 0usize;
    let mut stack = vec![assets.join("UI").join("FrameDef")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("fdf") {
                continue;
            }
            fdf_total += 1;
            let text = fs::read_to_string(&path).unwrap_or_default();
            if FdfDocument::parse(&text).is_err() {
                fdf_failed += 1;
                if gate.failures.len() < 40 {
                    gate.failures.push(format!(
                        "{}: parse failed",
                        path.strip_prefix(assets).unwrap_or(&path).display()
                    ));
                }
            }
        }
    }
    gate.note("fdf_documents", fdf_total);
    gate.note("fdf_failures", fdf_failed);

    // Build menu: data menu vs the sim's owner-derived command card.
    gate.total += 4;
    for race in [Race::Human, Race::Orc, Race::Undead, Race::NightElf] {
        let menu_defs = build_menu(race);
        let menu_ids: Vec<&str> = menu_defs.iter().map(|def| def.id).collect();
        let menu_ok = !menu_ids.is_empty()
            && menu_defs.iter().all(|def| {
                def.is_building() && def.stats.cost > 0 && def_of(def.id).is_some()
            });
        let mut sim = Sim::new(0x3E00 + race as u64, 32, 32);
        let hall_ok = main_hall_for(race)
            .and_then(|hall| sim.spawn_def(0, hall.id, cell_center(10, 10)).map(|_| ()))
            .is_some();
        let owner_menu: Vec<&str> = if hall_ok {
            sim.build_menu_for_owner(0)
                .iter()
                .map(|def| def.id)
                .collect()
        } else {
            Vec::new()
        };
        let matches = menu_ids == owner_menu;
        if menu_ok && matches {
            gate.pass += 1;
        } else {
            gate.fail(format!(
                "{race:?}: build_menu={:?} owner_menu={:?}",
                menu_ids, owner_menu
            ));
        }
    }
    let (pass, total) = (gate.pass, gate.total);
    gate.finish(format!(
        "{pass}/{total} menu checks passed ({} core FDF, {fdf_total} FDF total, 4 build menus)",
        CORE_FDF.len()
    ))
}

// ---------------------------------------------------------------------------
// JSON output
// ---------------------------------------------------------------------------

fn push_json_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
}

fn gate_json(gate: &Gate) -> String {
    let mut out = String::from("{");
    out.push_str("\"id\":");
    push_json_string(&mut out, gate.id);
    out.push_str(",\"area\":");
    push_json_string(&mut out, gate.area);
    out.push_str(",\"status\":");
    push_json_string(&mut out, gate.status());
    out.push_str(&format!(
        ",\"total\":{},\"pass\":{},\"fail\":{},\"gap\":{}",
        gate.total, gate.pass, gate.fail, gate.gap
    ));
    out.push_str(",\"detail\":");
    push_json_string(&mut out, &gate.detail);
    out.push_str(",\"failures\":[");
    for (index, failure) in gate.failures.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        push_json_string(&mut out, failure);
    }
    out.push_str("],\"metrics\":{");
    for (index, (key, value)) in gate.metrics.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        push_json_string(&mut out, key);
        out.push(':');
        push_json_string(&mut out, value);
    }
    out.push_str("},\"evidence\":[");
    for (index, item) in gate.evidence.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        push_json_string(&mut out, item);
    }
    out.push_str("]}");
    out
}

fn main() {
    let mut out = PathBuf::from("analysis/agent-qa/parity/report-raw.json");
    let mut assets = PathBuf::from("analysis/war3_full3");
    let mut maps = PathBuf::from("analysis/war3_full3/Maps");
    let mut tft = PathBuf::from("analysis/war3x_data");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = PathBuf::from(args.next().unwrap_or_default()),
            "--assets" => assets = PathBuf::from(args.next().unwrap_or_default()),
            "--maps" => maps = PathBuf::from(args.next().unwrap_or_default()),
            "--tft" => tft = PathBuf::from(args.next().unwrap_or_default()),
            "--parse-file" => {
                let path = PathBuf::from(args.next().unwrap_or_default());
                let text = fs::read_to_string(&path).unwrap_or_default();
                match wc3_jass::parse(&text) {
                    Ok(program) => println!(
                        "OK functions={} globals={}",
                        program.functions.len(),
                        program.globals.len()
                    ),
                    Err(error) => println!("ERR {error}"),
                }
                return;
            }
            "--dump-script" => {
                let path = PathBuf::from(args.next().unwrap_or_default());
                match MapBundle::open(&path) {
                    Ok(bundle) => match bundle.script {
                        Some(script) => print!("{script}"),
                        None => eprintln!("NO_SCRIPT {}", path.display()),
                    },
                    Err(error) => eprintln!("OPEN_ERROR {}: {error}", path.display()),
                }
                return;
            }
            other => {
                eprintln!("unknown argument {other}");
                std::process::exit(64);
            }
        }
    }

    std::panic::set_hook(Box::new(|info| {
        eprintln!("PARITY_PANIC {info}");
    }));

    let a1 = assets.clone();
    let a2 = assets.clone();
    let a3 = assets.clone();
    let a4 = assets.clone();
    let t1 = tft.clone();
    let m1 = maps.clone();
    let gates = vec![
        safe_gate("train.all_defs", "train", gate_train),
        safe_gate("train.menu_linkage", "train", gate_train_linkage),
        safe_gate("build.all_defs", "build", gate_build),
        safe_gate("tech.requires", "tech", gate_tech_requires),
        safe_gate("cast.spells", "cast", gate_cast),
        safe_gate("cast.coverage", "cast", gate_ability_coverage),
        safe_gate("hero.stats", "hero", gate_hero_stats),
        safe_gate("hero.coverage", "hero", gate_hero_coverage),
        safe_gate("hero.leveling", "hero", gate_hero_leveling),
        safe_gate("roster.coverage", "roster", gate_roster_coverage),
        safe_gate("items.data", "items", {
            let roc = a1.clone();
            let tft_path = t1.clone();
            move || gate_items_data(&roc, &tft_path)
        }),
        safe_gate("items.schema", "items", {
            let roc = a1.clone();
            let tft_path = t1.clone();
            move || gate_items_schema(&roc, &tft_path)
        }),
        safe_gate("items.runtime", "items", gate_items_runtime),
        safe_gate("items.gaps", "items", gate_items_gaps),
        safe_gate("economy.lumber", "economy", gate_lumber),
        safe_gate("creeps.camps", "creeps", gate_creeps),
        safe_gate("map.units.spawn", "maps", {
            let maps_path = m1.clone();
            move || gate_map_units(&maps_path)
        }),
        safe_gate("triggers.jass", "triggers", move || gate_triggers(&m1)),
        safe_gate("triggers.return", "triggers", gate_trigger_return),
        safe_gate("triggers.elseif", "triggers", gate_trigger_elseif),
        safe_gate("audio.game", "audio", move || gate_audio(&a2)),
        safe_gate("menu.fdf", "menu", move || gate_menu(&a3)),
        safe_gate("assets.tree", "assets", move || gate_assets(&a4)),
    ];

    let fail = gates.iter().filter(|gate| gate.status() == "FAIL").count();
    let gap = gates.iter().filter(|gate| gate.status() == "GAP").count();
    let status = if fail > 0 {
        "FAIL"
    } else if gap > 0 {
        "PARTIAL"
    } else {
        "PASS"
    };
    let generated = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);

    let mut json = String::from("{");
    json.push_str("\"schema\":\"wc3rust-parity-v1\",\"generated_unix\":");
    json.push_str(&generated.to_string());
    json.push_str(",\"status\":");
    push_json_string(&mut json, status);
    json.push_str(",\"assets\":");
    push_json_string(&mut json, &assets.display().to_string());
    json.push_str(",\"maps\":");
    push_json_string(&mut json, &maps.display().to_string());
    json.push_str(",\"gates\":[");
    for (index, gate) in gates.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&gate_json(gate));
    }
    json.push_str("]}\n");

    if let Some(parent) = out.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(error) = fs::write(&out, json) {
        eprintln!("write {} -> {error}", out.display());
        std::process::exit(74);
    }

    println!(
        "PARITY {status} gates={} fail={} gap={}",
        gates.len(),
        fail,
        gap
    );
    for gate in &gates {
        println!(
            "GATE {:<20} {:<7} pass={} fail={} gap={} total={}",
            gate.id,
            gate.status(),
            gate.pass,
            gate.fail,
            gate.gap,
            gate.total
        );
    }
    println!("REPORT {}", out.display());
    let missing_maps = maps.is_dir();
    if !missing_maps {
        eprintln!("maps directory missing: {}", maps.display());
    }

    std::process::exit(match status {
        "PASS" => 0,
        "PARTIAL" => 2,
        _ => 1,
    });
}

// ---------------------------------------------------------------------------
// assets.tree — the extraction coverage the parity run depends on
// ---------------------------------------------------------------------------

fn gate_assets(assets: &Path) -> Gate {
    let mut gate = Gate::new("assets.tree", "assets");
    gate.total = 1;
    let required = [
        "Units/UnitData.slk",
        "Units/unitUI.slk",
        "Units/AbilityData.slk",
        "Units/ItemData.slk",
        "UI/SoundInfo/UnitAckSounds.slk",
        "UI/SoundInfo/UISounds.slk",
        "UI/FrameDef/UI/ConsoleUI.fdf",
        "TerrainArt",
        "Sound",
    ];
    let mut missing = Vec::new();
    for path in required {
        if !assets.join(path).exists() {
            missing.push(path);
        }
    }
    if missing.is_empty() {
        gate.pass = 1;
    } else {
        gate.fail(format!("missing extracted assets: {missing:?}"));
    }
    gate.note("assets_root", assets.display().to_string());
    gate.finish(if missing.is_empty() {
        "required parity asset roots present"
    } else {
        "asset roots missing"
    })
}
