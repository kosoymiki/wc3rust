//! Deterministic opponent AI: workers, supply, army, expansion, attack waves
//! and retreats.  The controller only issues orders through the public Sim
//! API, so it exercises exactly the same systems a human player uses.

use glam::Vec2;

use crate::{Order, Spell, SpellTarget, UnitId, data, def_at, def_of};

/// `H2_AI_DEBUG=1` switches on the expansion trace.  The flag is read once
/// per process: the AI runs every tick and the trace is a diagnostic
/// contract, not a per-decision env lookup.
fn ai_debug() -> bool {
    static FLAG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FLAG.get_or_init(|| std::env::var_os("H2_AI_DEBUG").is_some())
}

/// WC3 melee keeps five workers on a gold mine; the sixth is surplus and
/// idles near the hall.
pub const MINE_SATURATION: usize = 5;

/// Distance from the hall toward the enemy base where a wave gathers before it
/// attacks.  WC3 melee armies rally outside the base instead of trickling in
/// one unit at a time.
const REGROUP_OFFSET: f32 = 600.0;
/// A unit inside this radius of the rally counts as regrouped.
const REGROUP_RADIUS: f32 = 420.0;
/// The wave leaves once this share of the army (permille) reached the rally.
const REGROUP_READY_PERMILLE: usize = 750;
/// Focus fire looks for the lowest-HP enemy inside this radius of the army.
const FOCUS_RADIUS: f32 = 900.0;
/// Expansion candidates must sit at least this far from the main hall, so the
/// AI never "expands" onto the mine its main base already works.
const EXPAND_MIN_DISTANCE: f32 = 900.0;
/// Enemy proximity inside this radius of a candidate mine is a safety penalty.
const EXPAND_THREAT_RADIUS: f32 = 1_800.0;
/// Weight of the enemy-distance penalty relative to the hall distance.
const EXPAND_THREAT_WEIGHT: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiMode {
    Build,
    Attack,
    Retreat,
}

#[derive(Debug, Clone, Copy)]
pub struct AiConfig {
    /// Workers the AI keeps alive (excluding the starting worker).
    pub workers: usize,
    /// Supply the army must reach before the first wave.
    pub army_supply: i32,
    /// Earliest tick for an attack wave.
    pub first_attack_tick: u64,
    /// Ticks between attack waves.
    pub wave_interval: u32,
    /// Retreat when average army health falls below this permille.
    pub retreat_permille: i32,
    /// Ordered composition cycle (def id, weight kept for data-driven tuning).
    pub composition: &'static [(&'static str, u32)],
    /// Preferred worker type id.
    pub worker: &'static str,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            workers: 5,
            army_supply: 12,
            first_attack_tick: 1500,
            wave_interval: 1200,
            retreat_permille: 350,
            composition: &[("hfoo", 3), ("hrif", 2), ("hmpr", 1)],
            worker: "hpea",
        }
    }
}

impl AiConfig {
    /// Race-specific defaults used by `Sim::enable_ai_for_race`.
    pub fn for_race(race: data::Race) -> Self {
        match race {
            data::Race::Human => Self::default(),
            data::Race::Orc => Self {
                composition: &[("ogru", 3), ("ohun", 2), ("odoc", 1)],
                worker: "opeo",
                ..Self::default()
            },
            data::Race::Undead => Self {
                composition: &[("ugho", 3), ("ucry", 2), ("unec", 1)],
                worker: "uaco",
                ..Self::default()
            },
            data::Race::NightElf => Self {
                composition: &[("earc", 3), ("esen", 2), ("edry", 1)],
                worker: "ewsp",
                ..Self::default()
            },
            data::Race::Neutral => Self::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AiState {
    pub owner: u8,
    pub config: AiConfig,
    pub mode: AiMode,
    pub waves: u32,
    pub next_wave: u64,
    pub next_train: u64,
    pub next_build: u64,
    pub next_expand: u64,
    pub home: Vec2,
    pub train_cycle: usize,
    pub expanded: bool,
    pub expand_mine: Option<Vec2>,
    pub expand_worker: Option<UnitId>,
    pub last_target: Option<UnitId>,
    /// `(site, remaining when last seen, tick)` — progress resets the clock, so a
    /// long-but-healthy build is never cancelled; a site that stops gaining
    /// progress (builder walled out by its own footprint) is.
    pub site_since: Option<(UnitId, u32, u64)>,
}

impl AiState {
    pub fn new(owner: u8, home: Vec2, config: AiConfig) -> Self {
        Self {
            owner,
            config,
            mode: AiMode::Build,
            waves: 0,
            next_wave: config.first_attack_tick,
            next_train: 30,
            next_build: 60,
            next_expand: 2400,
            home,
            train_cycle: 0,
            expanded: false,
            expand_mine: None,
            expand_worker: None,
            last_target: None,
            site_since: None,
        }
    }
}

impl crate::Sim {
    /// Enables the built-in AI for `owner` using race defaults.
    pub fn enable_ai_for_race(&mut self, owner: u8, race: data::Race) {
        self.enable_ai(owner, AiConfig::for_race(race));
    }

    pub(crate) fn advance_ai(&mut self) {
        if self.ai.is_empty() {
            return;
        }
        for index in 0..self.ai.len() {
            let mut state = self.ai[index].clone();
            self.run_ai(&mut state);
            self.ai[index] = state;
        }
    }

    fn run_ai(&mut self, state: &mut AiState) {
        let owner = state.owner;
        let tick = self.tick;
        // The home anchor outlives the hall: when the main hall is destroyed
        // `first_hall` returns `None` from that tick, and the AI used to freeze
        // forever.  It now keeps the last known base position and rebuilds.
        if let Some(hall) = self.first_hall(owner) {
            state.home = hall;
        }
        let home = state.home;
        if tick % 60 == 0 && self.first_hall(owner).is_none() {
            self.ai_rebuild_hall(state);
        }
        if tick % 30 == 0 {
            self.ai_workers(state, home);
            self.ai_plan(state, home);
        }
        if tick % 45 == 0 {
            self.ai_army(state, home);
            self.ai_hero(state, home);
        }
        if tick % 120 == 0 {
            self.ai_repair(state, home);
        }
        if tick % 60 == 0 {
            self.ai_watchdog(state);
        }
        if tick % 20 == 0 {
            self.ai_spells(state);
        }
        if tick >= state.next_expand && !state.expanded {
            if self.ai_expand(state, home) {
                state.expanded = true;
            } else if state.next_expand <= tick {
                state.next_expand = tick + 180;
            }
        }
        self.ai_attack(state, home);
    }

    fn first_hall(&self, owner: u8) -> Option<Vec2> {
        self.units
            .iter()
            .find(|unit| {
                unit.alive
                    && unit.owner == owner
                    && unit.is_building()
                    && unit.construct.is_none()
                    && unit.def().is_main_hall()
            })
            .map(|unit| unit.pos)
    }

    fn first_hall_id(&self, owner: u8) -> Option<UnitId> {
        self.units
            .iter()
            .find(|unit| {
                unit.alive
                    && unit.owner == owner
                    && unit.is_building()
                    && unit.construct.is_none()
                    && unit.def().is_main_hall()
            })
            .map(|unit| unit.id)
    }

    fn ai_workers(&mut self, state: &mut AiState, hall: Vec2) {
        let owner = state.owner;
        let tick = self.tick;
        let worker_def = def_of(state.config.worker);
        let worker_cost = worker_def.map(|def| def.stats.cost).unwrap_or(0);
        let worker_queued: usize = self
            .units
            .iter()
            .filter(|unit| unit.alive && unit.owner == owner && unit.is_building())
            .map(|unit| {
                unit.production
                    .iter()
                    .filter(|item| {
                        def_at(item.def)
                            .map(|def| def.stats.is_worker)
                            .unwrap_or(false)
                    })
                    .count()
            })
            .sum();
        let workers = self
            .units
            .iter()
            .filter(|unit| unit.alive && unit.owner == owner && unit.def().stats.is_worker)
            .count()
            + worker_queued;
        // Lumber demand comes from the queue the build planner wants to spend
        // next, not from a fixed split: every pending unit/building adds its
        // `lumber_cost`, and the chopping crew scales with it.
        let planned_lumber = self
            .ai_next_build_def(owner, state)
            .map(|(def, _)| def.stats.lumber_cost)
            .unwrap_or(0);
        let queued_lumber: i32 = self
            .units
            .iter()
            .filter(|unit| unit.alive && unit.owner == owner && unit.is_building())
            .flat_map(|unit| unit.production.iter())
            .filter_map(|item| def_at(item.def))
            .map(|def| def.stats.lumber_cost)
            .sum();
        let lumber_target = (2 + ((queued_lumber + planned_lumber) / 80) as usize).min(6);
        // `config.workers` staffs the gold line (five per mine); the lumber
        // crew is trained on top of it, so a five-worker AI still chops.
        let worker_target = state.config.workers + lumber_target;
        if workers < worker_target
            && tick >= state.next_train
            && self.resources[owner as usize] >= worker_cost + 80
            && self.supply_used(owner) + 1 <= self.supply_cap(owner)
        {
            if let Some(hall_id) = self.first_hall_id(owner) {
                if self.train_def(hall_id, state.config.worker).is_ok() {
                    state.next_train = tick + 90;
                    if ai_debug() {
                        eprintln!(
                            "[ai] t={} worker: train {}/{} lumber_target={}",
                            tick,
                            workers + 1,
                            worker_target,
                            lumber_target
                        );
                    }
                }
            }
        }
        // Put idle workers on the closest non-empty mine.
        let idle: Vec<(UnitId, Vec2)> = self
            .idle_workers(owner)
            .into_iter()
            .filter(|id| Some(*id) != state.expand_worker)
            .filter_map(|id| self.unit(id).map(|unit| (id, unit.pos)))
            .collect();
        // WC3 melee saturation: five workers per live gold mine, the rest chop
        // lumber.  Without a lumber crew the AI can never afford a farm,
        // barracks or tower and its build order dead-ends.
        let mine_count = self
            .gold_mines
            .iter()
            .filter(|mine| mine.remaining > 0)
            .count()
            .max(1);
        let mut gold_workers = self
            .units
            .iter()
            .filter(|unit| {
                unit.alive
                    && unit.owner == owner
                    && unit.def().stats.is_worker
                    && matches!(unit.order, Order::Harvest { .. })
            })
            .count();
        let mut lumber_workers = self
            .units
            .iter()
            .filter(|unit| {
                unit.alive
                    && unit.owner == owner
                    && unit.def().stats.is_worker
                    && matches!(unit.order, Order::HarvestLumber { .. })
            })
            .count();
        // Never overfill a mine at the expense of the wood line, and never let
        // the wood line take the last miner: the split is bounded by the crew.
        let gold_target = (MINE_SATURATION * mine_count)
            .min(worker_target.saturating_sub(lumber_target))
            .max(1);
        // Rebalance a mis-assigned crew (a build order or a lost mine can leave
        // the gold line empty while the forest crew is over target).  WC3's AI
        // keeps the mine saturated; checked on a slow cadence so a unit's path
        // is not reset every tick.
        if tick % 150 == 0 && gold_workers < gold_target && lumber_workers > lumber_target {
            let mut surplus: Vec<(UnitId, Vec2)> = self
                .units
                .iter()
                .filter(|unit| {
                    unit.alive
                        && unit.owner == owner
                        && unit.def().stats.is_worker
                        && matches!(unit.order, Order::HarvestLumber { .. })
                })
                .map(|unit| (unit.id, unit.pos))
                .collect();
            surplus.sort_by(|a, b| {
                a.1.distance(hall)
                    .partial_cmp(&b.1.distance(hall))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let convert = (gold_target - gold_workers).min(lumber_workers - lumber_target);
            for (id, pos) in surplus.into_iter().take(convert) {
                if let Some(mine) = self.nearest_gold_mine(pos) {
                    self.issue(&[id], crate::OrderSpec::Harvest(mine));
                    gold_workers += 1;
                    lumber_workers -= 1;
                }
            }
        }
        for (id, pos) in idle {
            if gold_workers < gold_target {
                if let Some(mine) = self.nearest_gold_mine(pos) {
                    self.issue(&[id], crate::OrderSpec::Harvest(mine));
                    gold_workers += 1;
                    continue;
                }
            }
            if lumber_workers < lumber_target {
                if let Some(tree) = self.nearest_tree_to(pos) {
                    self.issue(&[id], crate::OrderSpec::HarvestLumber(tree));
                    lumber_workers += 1;
                    continue;
                }
            }
            // Saturated crew: WC3 keeps the peon near the hall instead of
            // pushing a sixth worker onto the same mine.
            if pos.distance(hall) > 400.0 {
                self.issue(&[id], crate::OrderSpec::Move(hall));
            }
        }
    }

    /// Nearest readable node for the AI crew: wood first (trees inside the AI's
    /// reach), never a tree on the other side of the map.
    fn nearest_tree_to(&self, pos: Vec2) -> Option<Vec2> {
        self.destructables
            .iter()
            .filter(|tree| tree.alive)
            .map(|tree| (tree.pos, tree.pos.distance(pos)))
            .filter(|(_, distance)| *distance <= 1500.0)
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(pos, _)| pos)
    }

    /// Data-driven build order, recomputed from live state every cycle:
    /// `hall -> workers -> supply -> army producer -> tech chain -> altar`.
    ///
    /// Every step is derived from the roster tables -- `trained_at` names the
    /// producer for each composition unit, `requires` walks the prerequisite
    /// chain, `supply_building_for`/`altar_for` pick the race's buildings -- so
    /// there is no hard-coded per-race string list.  A destroyed building
    /// simply makes its step missing again and the next cycle rebuilds it.
    fn ai_plan(&mut self, state: &mut AiState, home: Vec2) {
        let owner = state.owner;
        let tick = self.tick;
        if tick < state.next_build {
            return;
        }
        // One site at a time: wait for the current construction to finish so
        // the builder is not pulled between sites (and prerequisites count).
        if self.has_construction(owner) {
            self.ai_resume_build(owner, state.expand_worker);
            state.next_build = tick + 60;
            return;
        }
        let Some((def, step)) = self.ai_next_build_def(owner, state) else {
            state.next_build = tick + 120;
            return;
        };
        if self.resources[owner as usize] < def.stats.cost
            || self.lumber[owner as usize] < def.stats.lumber_cost
        {
            if ai_debug() {
                let race = self.owner_race(owner, &state.config);
                eprintln!(
                    "[ai] t={} race={} plan={} def={} wait gold={}/{} lumber={}/{}",
                    tick,
                    race.name(),
                    step,
                    def.id,
                    self.resources[owner as usize],
                    def.stats.cost,
                    self.lumber[owner as usize],
                    def.stats.lumber_cost
                );
            }
            state.next_build = tick + 120;
            return;
        }
        let Some(site) = self.find_build_site(home, def.footprint, 800.0) else {
            state.next_build = tick + 180;
            return;
        };
        if self.ai_place(state, def.id, site) {
            if ai_debug() {
                let race = self.owner_race(owner, &state.config);
                eprintln!(
                    "[ai] t={} race={} plan={} def={} at=({:.0},{:.0}) gold={}",
                    tick,
                    race.name(),
                    step,
                    def.id,
                    site.x,
                    site.y,
                    self.resources[owner as usize]
                );
            }
            state.next_build = tick + 240;
        } else {
            state.next_build = tick + 60;
        }
    }

    /// Next building the plan wants, without touching the world.
    fn ai_next_build_def(
        &self,
        owner: u8,
        state: &AiState,
    ) -> Option<(&'static data::UnitDef, &'static str)> {
        let hall_id = self.main_hall(owner)?;
        let race = self.unit(hall_id)?.def().race;
        // Supply before the cap: a farm/burrow/ziggurat/moon well is the first
        // thing that blocks every other step, exactly like WC3's melee AI.  The
        // opening supply building is placed even while headroom remains; later
        // ones are only added when the cap is within four supply.
        if let Some(food) = data::supply_building_for(race) {
            let food_exists = self.has_building_or_site_id(owner, food.id);
            if !food_exists || self.supply_cap(owner) - self.supply_used(owner) <= 4 {
                return Some((food, "supply"));
            }
        }
        for entry in state.config.composition {
            let Some(unit_def) = def_of(entry.0) else {
                continue;
            };
            // Producer first (barracks, crypt, ancient of war, ...).
            if let Some(&producer_id) = unit_def.trained_at.first()
                && !self.has_building_or_site(owner, unit_def)
                && let Some(step) = self.ai_next_missing(owner, producer_id)
            {
                return Some((step, "producer"));
            }
            // Then the unit's own tech chain (blacksmith, war mill, ...).
            for required in unit_def.requires {
                if self.has_building(owner, required) {
                    continue;
                }
                if let Some(step) = self.ai_next_missing(owner, required) {
                    return Some((step, "tech"));
                }
            }
        }
        // Hero tech last: the altar has no combat value until the army exists.
        if let Some(altar) = data::altar_for(race)
            && !self.has_building_or_site_id(owner, altar.id)
        {
            return Some((altar, "altar"));
        }
        None
    }

    /// True when a building of `def_id` stands complete or as a site.
    fn has_building_or_site_id(&self, owner: u8, def_id: &str) -> bool {
        self.units.iter().any(|unit| {
            unit.alive && unit.owner == owner && unit.is_building() && unit.def().id == def_id
        })
    }

    /// Race of the owner's hall, falling back to the configured worker's race
    /// when the hall is gone (rebuild path).
    fn owner_race(&self, owner: u8, config: &AiConfig) -> data::Race {
        self.main_hall(owner)
            .and_then(|id| self.unit(id))
            .map(|unit| unit.def().race)
            .or_else(|| def_of(config.worker).map(|def| def.race))
            .unwrap_or(data::Race::Human)
    }

    /// Rebuilds the main hall after it was destroyed.
    fn ai_rebuild_hall(&mut self, state: &mut AiState) {
        let owner = state.owner;
        if self.has_construction(owner) {
            return;
        }
        let race = self.owner_race(owner, &state.config);
        let Some(hall_def) = data::main_hall_for(race) else {
            return;
        };
        if self.resources[owner as usize] < hall_def.stats.cost
            || self.lumber[owner as usize] < hall_def.stats.lumber_cost
        {
            return;
        }
        let Some(worker) = self.nearest_free_worker(owner, state.home, state.expand_worker) else {
            return;
        };
        let Some(site) =
            self.find_build_site_ignoring(state.home, hall_def.footprint, 900.0, Some(worker))
        else {
            return;
        };
        if self.place_building_ordered(worker, hall_def.id, site).is_ok() && ai_debug() {
            eprintln!(
                "[ai] t={} race={} plan=rebuild-hall def={} at=({:.0},{:.0})",
                self.tick,
                race.name(),
                hall_def.id,
                site.x,
                site.y
            );
        }
    }

    /// Trains the race hero from a finished altar once the army tech exists.
    fn ai_hero(&mut self, state: &mut AiState, _home: Vec2) {
        let owner = state.owner;
        if self
            .units
            .iter()
            .any(|unit| unit.alive && unit.owner == owner && unit.def().hero)
        {
            return;
        }
        let race = self.owner_race(owner, &state.config);
        let Some(altar_def) = data::altar_for(race) else {
            return;
        };
        let Some(altar) = self
            .units
            .iter()
            .find(|unit| {
                unit.alive
                    && unit.owner == owner
                    && unit.is_building()
                    && unit.construct.is_none()
                    && unit.def().id == altar_def.id
            })
            .map(|unit| unit.id)
        else {
            return;
        };
        let Some(hero) = data::ROSTER.iter().find(|def| {
            def.race == race && def.hero && def.trained_at.contains(&altar_def.id)
        }) else {
            return;
        };
        if self.resources[owner as usize] < hero.stats.cost
            || self.lumber[owner as usize] < hero.stats.lumber_cost
        {
            return;
        }
        if self.train_def(altar, hero.id).is_ok() && ai_debug() {
            eprintln!("[ai] t={} race={} hero: train {}", self.tick, race.name(), hero.id);
        }
    }

    /// Sends one free worker to repair the most damaged completed building.
    /// Repairs run between construction sites only, so the builder is never
    /// pulled off a site (the site watchdog would otherwise cancel it).
    fn ai_repair(&mut self, state: &mut AiState, _home: Vec2) {
        let owner = state.owner;
        // Repairs do not wait for the build queue: the crew is large enough to
        // spare one worker, and waiting for a gap between sites starved the
        // step on a busy AI (the builder is excluded by `nearest_free_worker`).
        let Some((target, target_pos)) = self
            .units
            .iter()
            .filter(|unit| {
                unit.alive
                    && unit.owner == owner
                    && unit.is_building()
                    && unit.construct.is_none()
                    && unit.hp * 4 < unit.max_hp * 3
            })
            .min_by(|a, b| {
                (a.hp * 100 / a.max_hp.max(1))
                    .cmp(&(b.hp * 100 / b.max_hp.max(1)))
                    .then(a.id.cmp(&b.id))
            })
            .map(|unit| (unit.id, unit.pos))
        else {
            return;
        };
        if self.units.iter().any(|unit| {
            unit.alive
                && unit.owner == owner
                && matches!(unit.order, Order::Repair(id) if id == target)
        }) {
            return;
        }
        if self.resources[owner as usize] < 25 || self.lumber[owner as usize] < 25 {
            return;
        }
        if let Some(worker) = self.nearest_free_worker(owner, target_pos, state.expand_worker) {
            self.issue(&[worker], crate::OrderSpec::Repair(target));
            if ai_debug() {
                eprintln!(
                    "[ai] t={} repair: worker {:?} -> {:?}",
                    self.tick, worker, target
                );
            }
        }
    }

    /// Nearest worker that is not a building and is not reserved for the
    /// expansion site.
    fn nearest_free_worker(
        &self,
        owner: u8,
        pos: Vec2,
        except: Option<UnitId>,
    ) -> Option<UnitId> {
        self.units
            .iter()
            .filter(|unit| {
                unit.alive
                    && unit.owner == owner
                    && !unit.is_building()
                    && unit.def().stats.is_worker
                    && Some(unit.id) != except
                    && !matches!(unit.order, Order::Build(_))
            })
            .map(|unit| (unit.id, unit.pos.distance(pos)))
            .min_by(|a, b| {
                a.1.partial_cmp(&b.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.0.cmp(&b.0))
            })
            .map(|(id, _)| id)
    }

    fn ai_army(&mut self, state: &mut AiState, hall: Vec2) {
        let owner = state.owner;
        let tick = self.tick;
        // Build the next composition item when supply headroom exists.
        if self.supply_used(owner) + 3 > self.supply_cap(owner) {
            return;
        }
        for _ in 0..state.config.composition.len().max(1) {
            let entry =
                state.config.composition[state.train_cycle % state.config.composition.len().max(1)];
            state.train_cycle = state.train_cycle.wrapping_add(1);
            let Some(def) = def_of(entry.0) else {
                continue;
            };
            if self.resources[owner as usize] < def.stats.cost {
                return;
            }
            if let Some(building) = self.producer_for(owner, def.id) {
                if self.train_def(building, def.id).is_ok() {
                    state.next_train = tick + 45;
                    return;
                }
            }
        }
        let _ = hall;
    }

    fn producer_for(&self, owner: u8, unit_id: &str) -> Option<UnitId> {
        let def = def_of(unit_id)?;
        self.units
            .iter()
            .find(|unit| {
                unit.alive
                    && unit.owner == owner
                    && unit.is_building()
                    && unit.construct.is_none()
                    && def.trained_at.contains(&unit.def().id)
            })
            .map(|unit| unit.id)
    }

    /// Re-issues the build order when a construction site lost its builder.
    ///
    /// A worker that is already channeling a site keeps its order: the AI only
    /// reclaims *free* workers, and it picks the one nearest the site (shortest
    /// walk, deterministic id tiebreak) instead of the first worker in the
    /// list.
    fn ai_resume_build(&mut self, owner: u8, except: Option<UnitId>) {
        let Some(site) = self
            .units
            .iter()
            .find(|unit| {
                unit.alive && unit.owner == owner && unit.is_building() && unit.construct.is_some()
            })
            .map(|unit| (unit.id, unit.pos))
        else {
            return;
        };
        let has_builder = self.units.iter().any(|unit| {
            unit.alive
                && unit.owner == owner
                && !unit.is_building()
                && unit.def().stats.is_worker
                && matches!(unit.order, crate::Order::Build(target) if target == site.0)
        });
        if has_builder {
            return;
        }
        let free_worker = self
            .units
            .iter()
            .filter(|unit| {
                unit.alive
                    && unit.owner == owner
                    && !unit.is_building()
                    && unit.def().stats.is_worker
                    && !matches!(unit.order, crate::Order::Build(_))
                    && Some(unit.id) != except
            })
            .min_by(|a, b| {
                a.pos
                    .distance(site.1)
                    .partial_cmp(&b.pos.distance(site.1))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.id.cmp(&b.id))
            })
            .map(|unit| unit.id);
        if let Some(worker) = free_worker {
            self.issue(&[worker], crate::OrderSpec::Build(site.0));
        }
    }

    fn has_construction(&self, owner: u8) -> bool {
        self.units.iter().any(|unit| {
            unit.alive && unit.owner == owner && unit.is_building() && unit.construct.is_some()
        })
    }

    fn has_building(&self, owner: u8, def_id: &str) -> bool {
        self.units.iter().any(|unit| {
            unit.alive
                && unit.owner == owner
                && unit.is_building()
                && unit.def().id == def_id
                && unit.construct.is_none()
        })
    }

    /// Deepest missing prerequisite for `def_id` (build this one next).
    fn ai_next_missing(&self, owner: u8, def_id: &'static str) -> Option<&'static data::UnitDef> {
        let def = def_of(def_id)?;
        for required in def.requires {
            if !self.has_building(owner, required) {
                return self
                    .ai_next_missing(owner, required)
                    .or_else(|| def_of(required));
            }
        }
        def_of(def_id)
    }

    /// Places a building, first walking the nearest worker to the site when it
    /// is out of placement range.  Returns true only when the site exists.
    fn ai_place(&mut self, state: &mut AiState, def_id: &'static str, site: Vec2) -> bool {
        let owner = state.owner;
        let Some((worker, distance)) = self
            .units
            .iter()
            .filter(|unit| {
                unit.alive
                    && unit.owner == owner
                    && !unit.is_building()
                    && unit.def().stats.is_worker
                    && Some(unit.id) != state.expand_worker
            })
            .map(|unit| (unit.id, unit.pos.distance(site)))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        else {
            return false;
        };
        if distance <= 380.0 {
            return self.place_building(worker, def_id, site).is_ok();
        }
        if self.tick >= state.next_build {
            self.issue(&[worker], crate::OrderSpec::Move(site));
            state.next_build = self.tick + 45;
        }
        false
    }

    /// True when a producer for `unit_def` exists either complete or under
    /// construction; used to stop the AI from spamming sites while one is
    /// being built.
    fn has_building_or_site(&self, owner: u8, unit_def: &'static data::UnitDef) -> bool {
        self.units.iter().any(|unit| {
            unit.alive
                && unit.owner == owner
                && unit.is_building()
                && unit_def.trained_at.contains(&unit.def().id)
        })
    }

    /// Cancel a construction that has been standing for `BUILD_STALL_TICKS`.
    ///
    /// With real footprints (a Great Hall is 6x6 cells) a misplaced site can wall
    /// its own builder out; the AI then keeps `has_construction` true forever and
    /// never expands or builds again. WC3's AI cancels blocked constructions.
    fn ai_watchdog(&mut self, state: &mut AiState) {
        const BUILD_STALL_TICKS: u64 = 1_200;
        let owner = state.owner;
        let site = self
            .units
            .iter()
            .find(|unit| unit.alive && unit.owner == owner && unit.construct.is_some())
            .and_then(|unit| unit.construct.as_ref().map(|c| (unit.id, c.remaining)));
        match (site, state.site_since) {
            (Some((id, remaining)), Some((tracked, last_remaining, _since)))
                if tracked == id && remaining < last_remaining =>
            {
                // Progress: restart the stall clock.
                state.site_since = Some((id, remaining, self.tick));
            }
            (Some((id, remaining)), Some((tracked, last_remaining, since)))
                if tracked == id && remaining == last_remaining =>
            {
                if self.tick.saturating_sub(since) > BUILD_STALL_TICKS {
                    self.cancel_construction(id);
                    state.site_since = None;
                }
            }
            (Some((id, remaining)), _) => state.site_since = Some((id, remaining, self.tick)),
            (None, _) => state.site_since = None,
        }
    }

    fn ai_expand(&mut self, state: &mut AiState, hall: Vec2) -> bool {
        let owner = state.owner;
        if self.has_construction(owner) {
            self.ai_resume_build(owner, state.expand_worker);
            return false;
        }
        let hall_def = self
            .main_hall(owner)
            .and_then(|id| self.unit(id))
            .map(|unit| unit.def())
            .unwrap_or_else(|| def_of("htow").expect("town hall"));
        // Expansion is a reward for a working economy: the main base must have
        // its five-per-mine crew before a second hall is considered.  A config
        // that asks for fewer workers saturates at that smaller target.
        let main_mines = self
            .gold_mines
            .iter()
            .filter(|mine| mine.remaining > 0 && mine.pos.distance(hall) <= 1_400.0)
            .count()
            .max(1);
        let crew_target = state
            .config
            .workers
            .min(MINE_SATURATION * main_mines)
            .max(1);
        let crew = self
            .units
            .iter()
            .filter(|unit| unit.alive && unit.owner == owner && unit.def().stats.is_worker)
            .count();
        if crew < crew_target {
            if ai_debug() {
                eprintln!(
                    "[ai] t={} expand: main base not saturated ({crew}/{crew_target})",
                    self.tick
                );
            }
            return false;
        }
        if self.resources[owner as usize] < hall_def.stats.cost + 250
            || self.lumber[owner as usize] < hall_def.stats.lumber_cost
        {
            return false;
        }
        let mine = match state.expand_mine {
            Some(mine) => mine,
            None => {
                // Expansion = the *safest* free mine: hall distance plus the
                // proximity of enemy units/buildings, so a guarded or contested
                // expansion loses to a farther but quiet one.  Workers are sent
                // to it instead of requiring one to be standing there already.
                let Some(mine) = self.safest_expansion_mine(owner, hall) else {
                    if ai_debug() {
                        eprintln!("[ai] t={} expand: no candidate mine", self.tick);
                    }
                    return false;
                };
                let Some(worker) = self
                    .units
                    .iter()
                    .filter(|unit| unit.alive && unit.owner == owner && unit.def().stats.is_worker)
                    .min_by(|a, b| {
                        a.pos
                            .distance(mine)
                            .partial_cmp(&b.pos.distance(mine))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|unit| unit.id)
                else {
                    return false;
                };
                state.expand_mine = Some(mine);
                state.expand_worker = Some(worker);
                state.next_expand = self.tick + 60;
                // Put the hall on the base side of the mine: the builder walks
                // out from the base and channels without the mine's own
                // blocking (or the gold node) pushing it outside the channel
                // ring, which stalled the site until the watchdog cancelled it.
                let anchor = expansion_anchor(hall, mine);
                if let Some(site) =
                    self.find_build_site_ignoring(anchor, hall_def.footprint, 900.0, Some(worker))
                {
                    self.issue(&[worker], crate::OrderSpec::Move(site));
                }
                return false;
            }
        };
        let anchor = expansion_anchor(hall, mine);
        let site =
            self.find_build_site_ignoring(anchor, hall_def.footprint, 900.0, state.expand_worker);
        let Some(site) = site else {
            if ai_debug() {
                eprintln!(
                    "[ai] t={} expand: no site near mine ({:.0},{:.0}) fp={} gold={}",
                    self.tick, mine.x, mine.y, hall_def.footprint, self.resources[owner as usize]
                );
            }
            state.next_expand = self.tick + 300;
            return false;
        };
        let Some(worker) = state.expand_worker else {
            state.expand_mine = None;
            return false;
        };
        let Some(unit) = self.unit(worker) else {
            state.expand_mine = None;
            state.expand_worker = None;
            return false;
        };
        if ai_debug() {
            eprintln!(
                "[ai] t={} expand: site=({:.0},{:.0}) worker_alive={} denial={:?}",
                self.tick,
                site.x,
                site.y,
                unit.alive,
                self.place_denial(owner, hall_def.id, site)
            );
        }
        if !unit.alive {
            state.expand_mine = None;
            state.expand_worker = None;
            return false;
        }
        // The site sits at the mine, whose own blocking keeps the worker ~400
        // units away: use the ordered variant (the worker walks in and channels)
        // instead of the near-only check that could never pass.
        // Place immediately: the sim's ordered variant hands the worker an
        // `Order::Build` and the worker walks to the site and channels. Waiting
        // for proximity made the AI dance around the mine (and the stall
        // watchdog then cancelled a perfectly good site).
        let attempt = self.place_building_ordered(worker, hall_def.id, site);
        if ai_debug() {
            eprintln!(
                "[ai] t={} expand: place result={:?}",
                self.tick,
                attempt.as_ref().err()
            );
        }
        match attempt {
            Ok(_) => {
                state.expand_mine = None;
                state.expand_worker = None;
                true
            }
            Err(_) => {
                // The cached site is unusable: forget the mine so the search
                // runs again with the current ground state.
                state.expand_mine = None;
                state.next_expand = self.tick + 120;
                false
            }
        }
    }

    /// Safest free mine for an expansion: hall distance dominates, enemy
    /// proximity inside `EXPAND_THREAT_RADIUS` is a weighted penalty.  Ties
    /// break on position so replays stay deterministic.
    fn safest_expansion_mine(&self, owner: u8, hall: Vec2) -> Option<Vec2> {
        let enemies: Vec<Vec2> = self
            .units
            .iter()
            .filter(|unit| unit.alive && unit.owner != owner)
            .map(|unit| unit.pos)
            .collect();
        let mut scored: Vec<(f32, Vec2)> = Vec::new();
        for mine in self.gold_mines.iter().filter(|mine| mine.remaining > 0) {
            let distance = mine.pos.distance(hall);
            if distance < EXPAND_MIN_DISTANCE {
                continue;
            }
            let nearest_enemy = enemies
                .iter()
                .map(|pos| pos.distance(mine.pos))
                .fold(f32::INFINITY, f32::min);
            let penalty = if nearest_enemy.is_finite() {
                (EXPAND_THREAT_RADIUS - nearest_enemy).max(0.0) * EXPAND_THREAT_WEIGHT
            } else {
                0.0
            };
            scored.push((distance + penalty, mine.pos));
        }
        scored
            .into_iter()
            .min_by(|a, b| {
                a.0.partial_cmp(&b.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.1.x.partial_cmp(&b.1.x).unwrap_or(std::cmp::Ordering::Equal))
                    .then(a.1.y.partial_cmp(&b.1.y).unwrap_or(std::cmp::Ordering::Equal))
            })
            .map(|(_, pos)| pos)
    }

    fn ai_attack(&mut self, state: &mut AiState, hall: Vec2) {
        let owner = state.owner;
        let tick = self.tick;
        let army: Vec<(UnitId, Vec2, i32, i32)> = self
            .units
            .iter()
            .filter(|unit| {
                unit.alive
                    && unit.owner == owner
                    && !unit.is_building()
                    && !unit.def().stats.is_worker
            })
            .map(|unit| (unit.id, unit.pos, unit.hp, unit.max_hp))
            .collect();
        if army.is_empty() {
            if state.mode == AiMode::Attack {
                state.mode = AiMode::Build;
                state.next_wave = tick + 600;
            }
            return;
        }
        let health: i64 = army.iter().map(|(_, _, hp, _)| *hp as i64).sum();
        let max_health: i64 = army.iter().map(|(_, _, _, max)| (*max as i64).max(1)).sum();
        let health_permille = (health * 1000 / max_health.max(1)) as i32;
        // Target selection is measured from the army, not from the base: an army
        // walking at the enemy hall ignores the units it passes, which stalls a
        // match after the buildings are gone.  The nearest enemy to the army is
        // the same "closest threat" rule the WC3 melee AI uses.
        let army_centre = army
            .iter()
            .fold(Vec2::ZERO, |sum, (_, pos, _, _)| sum + *pos)
            / army.len().max(1) as f32;
        let target = self
            .nearest_enemy_building(owner, army_centre)
            .or_else(|| self.nearest_enemy_unit(owner, army_centre))
            .map(|id| self.unit(id).map(|unit| unit.pos))
            .flatten();
        // Enemy base for the next offensive push: measured from the hall, so a
        // new wave walks at the enemy town instead of the first thing the army
        // happened to see on the way.
        let offensive_target = self
            .nearest_enemy_building(owner, hall)
            .or_else(|| self.nearest_enemy_unit(owner, hall))
            .and_then(|id| self.unit(id).map(|unit| unit.pos));

        match state.mode {
            AiMode::Attack => {
                if health_permille < state.config.retreat_permille {
                    let ids: Vec<UnitId> = army.iter().map(|(id, _, _, _)| *id).collect();
                    self.issue_group(&ids, hall, crate::Formation::Box, false);
                    state.mode = AiMode::Retreat;
                    state.next_wave = tick + 240;
                    return;
                }
                // Cleanup phase: the enemy has no buildings left, so the war is
                // decided by hunting the remaining units.  Each unit is sent at
                // its own nearest enemy (no formation, no shared path) because a
                // group attack-move to one straggler leaves the others alive and
                // the match never ends.
                if self.nearest_enemy_building(owner, army_centre).is_none() {
                    if tick >= state.next_wave {
                        for (id, pos, _, _) in &army {
                            if let Some(enemy) = self.nearest_enemy_unit(owner, *pos) {
                                self.issue(&[*id], crate::OrderSpec::Attack(enemy));
                            }
                        }
                        state.waves += 1;
                        state.next_wave = tick + 150;
                    }
                    return;
                }
                if tick >= state.next_wave {
                    if let Some(target) = target {
                        let ids: Vec<UnitId> = army.iter().map(|(id, _, _, _)| *id).collect();
                        self.issue_group(&ids, target, crate::Formation::Box, true);
                        state.waves += 1;
                    }
                    state.next_wave = tick + state.config.wave_interval as u64;
                }
                // Focus fire: the whole army switches to the lowest-HP enemy
                // near the fight instead of each unit picking its own target.
                // Issued on a slow cadence so paths are not reset every tick.
                if tick % 60 == 0
                    && let Some(focus) = self.lowest_hp_enemy_near(owner, army_centre, FOCUS_RADIUS)
                {
                    let focus_pos = self
                        .unit(focus)
                        .map(|unit| unit.pos)
                        .unwrap_or(army_centre);
                    let mut issued = 0usize;
                    for (id, pos, _, _) in &army {
                        if pos.distance(focus_pos) <= FOCUS_RADIUS + 600.0 {
                            self.issue(&[*id], crate::OrderSpec::Attack(focus));
                            issued += 1;
                        }
                    }
                    if issued > 0 {
                        state.last_target = Some(focus);
                        if ai_debug() {
                            let hp = self.unit(focus).map(|unit| unit.hp).unwrap_or(0);
                            eprintln!(
                                "[ai] t={} focus: {:?} hp={} units={}",
                                tick, focus, hp, issued
                            );
                        }
                    }
                }
            }
            AiMode::Retreat => {
                if health_permille >= 700 || tick >= state.next_wave {
                    state.mode = AiMode::Build;
                    state.next_wave = tick + (state.config.wave_interval / 2) as u64;
                }
            }
            AiMode::Build => {
                let army_supply: i32 = army
                    .iter()
                    .filter_map(|(id, _, _, _)| self.unit(*id))
                    .map(|unit| unit.def().stats.supply)
                    .sum();
                if tick >= state.next_wave && army_supply >= state.config.army_supply {
                    if let Some(target_pos) = offensive_target {
                        // Regroup first: the army gathers at a rally point on
                        // the way out of the base, then attack-moves as one
                        // group.  A trickle of single units into the enemy
                        // base loses to any defender.
                        let regroup = regroup_point(hall, target_pos);
                        let near = army
                            .iter()
                            .filter(|(_, pos, _, _)| pos.distance(regroup) <= REGROUP_RADIUS)
                            .count();
                        let ready = near * 1000
                            >= REGROUP_READY_PERMILLE * army.len().max(1);
                        if !ready {
                            let mut moving = 0usize;
                            for (id, pos, _, _) in &army {
                                if pos.distance(regroup) <= REGROUP_RADIUS {
                                    continue;
                                }
                                let heading = self
                                    .unit(*id)
                                    .and_then(|unit| unit.goal)
                                    .map(|goal| goal.distance(regroup) <= 96.0)
                                    .unwrap_or(false);
                                if !heading {
                                    self.issue(&[*id], crate::OrderSpec::Move(regroup));
                                    moving += 1;
                                }
                            }
                            if ai_debug() {
                                eprintln!(
                                    "[ai] t={} regroup: near={}/{} moving={} point=({:.0},{:.0})",
                                    tick,
                                    near,
                                    army.len(),
                                    moving,
                                    regroup.x,
                                    regroup.y
                                );
                            }
                            state.next_wave = tick + 30;
                            return;
                        }
                        let ids: Vec<UnitId> = army.iter().map(|(id, _, _, _)| *id).collect();
                        self.issue_group(&ids, target_pos, crate::Formation::Box, true);
                        state.mode = AiMode::Attack;
                        state.waves += 1;
                        self.events.push(crate::Event::AiWave {
                            owner,
                            wave: state.waves,
                        });
                        if ai_debug() {
                            eprintln!(
                                "[ai] t={} wave {} attack-move -> ({:.0},{:.0})",
                                tick, state.waves, target_pos.x, target_pos.y
                            );
                        }
                        state.next_wave = tick + state.config.wave_interval as u64;
                    }
                }
            }
        }

        // Home defence: react to enemies close to the base whenever the main
        // army is not already committed to an attack.  The order is re-issued
        // only every 45 ticks (or when the attacker changes): re-issuing every
        // tick resets the unit paths and starves the per-tick path budget, which
        // made a defending army stand still.
        if state.mode != AiMode::Attack
            && tick % 45 == 0
            && let Some(enemy) = self.nearest_enemy_unit(owner, hall)
        {
            if let Some(enemy_unit) = self.unit(enemy) {
                if enemy_unit.pos.distance(hall) <= 1100.0 {
                    let ids: Vec<UnitId> = army.iter().map(|(id, _, _, _)| *id).collect();
                    self.issue_group(&ids, enemy_unit.pos, crate::Formation::Box, true);
                    state.last_target = Some(enemy);
                }
            }
        }
    }

    fn ai_spells(&mut self, state: &mut AiState) {
        let owner = state.owner;
        let casters: Vec<(UnitId, Vec2, Vec3Stats)> = self
            .units
            .iter()
            .filter(|unit| {
                unit.alive
                    && unit.owner == owner
                    && !unit.is_building()
                    && !unit.def().spells.is_empty()
            })
            .map(|unit| {
                (
                    unit.id,
                    unit.pos,
                    Vec3Stats {
                        hp: unit.hp,
                        max_hp: unit.max_hp,
                        mana: unit.mana,
                    },
                )
            })
            .collect();
        for (caster, pos, stats) in casters {
            for &spell in self
                .unit(caster)
                .map(|unit| unit.def().spells)
                .unwrap_or(&[])
            {
                let def = spell.def();
                if stats.mana < def.mana {
                    continue;
                }
                match spell {
                    Spell::HolyLight | Spell::Heal | Spell::DeathCoil => {
                        // Heal the most damaged ally in range.
                        let target = self
                            .units
                            .iter()
                            .filter(|unit| {
                                unit.alive
                                    && unit.owner == owner
                                    && unit.hp < unit.max_hp
                                    && unit.pos.distance(pos) <= def.range
                            })
                            .min_by_key(|unit| unit.hp * 100 / unit.max_hp.max(1))
                            .map(|unit| unit.id);
                        if let Some(target) = target {
                            let _ = self.cast(caster, spell, SpellTarget::Unit(target));
                            break;
                        }
                    }
                    Spell::DivineShield => {
                        if stats.hp * 100 / stats.max_hp.max(1) < 50 {
                            let _ = self.cast(caster, spell, SpellTarget::None);
                            break;
                        }
                    }
                    Spell::ChainLightning
                    | Spell::EntanglingRoots
                    | Spell::StormBolt
                    | Spell::FrostNova => {
                        let target = self.nearest_enemy_unit(owner, pos);
                        if let Some(target) = target {
                            if let Some(unit) = self.unit(target) {
                                if unit.pos.distance(pos) <= def.range {
                                    let _ = self.cast(caster, spell, SpellTarget::Unit(target));
                                    break;
                                }
                            }
                        }
                    }
                    Spell::ThunderClap => {
                        let ready = self.units.iter().any(|unit| {
                            unit.alive
                                && unit.owner != owner
                                && unit.pos.distance(pos) <= def.radius + unit.radius()
                        });
                        if ready {
                            let _ = self.cast(caster, spell, SpellTarget::None);
                            break;
                        }
                    }
                    Spell::Blizzard | Spell::CarrionSwarm => {
                        let target = self
                            .units
                            .iter()
                            .filter(|unit| {
                                unit.alive
                                    && unit.owner != owner
                                    && unit.pos.distance(pos) <= def.range
                            })
                            .min_by(|a, b| {
                                a.pos
                                    .distance(pos)
                                    .partial_cmp(&b.pos.distance(pos))
                                    .unwrap_or(std::cmp::Ordering::Equal)
                            })
                            .map(|unit| unit.pos);
                        if let Some(target) = target {
                            let _ = self.cast(caster, spell, SpellTarget::Point(target));
                            break;
                        }
                    }
                    // Data-mapped hero abilities are driven by the hero/UI
                    // path by id, not by the typed auto-cast table.
                    Spell::DataDamage { .. }
                    | Spell::DataHeal { .. }
                    | Spell::DataBuff { .. }
                    | Spell::DataSummon { .. }
                    | Spell::DataAura { .. }
                    | Spell::DataAuraPassive { .. }
                    | Spell::DataTransformation { .. }
                    | Spell::DataOnly { .. } => {}
                }
            }
        }
        let _ = state;
    }

    fn nearest_enemy_building(&self, owner: u8, pos: Vec2) -> Option<UnitId> {
        self.units
            .iter()
            .filter(|unit| unit.alive && unit.owner != owner && unit.is_building())
            .min_by(|a, b| {
                a.pos
                    .distance(pos)
                    .partial_cmp(&b.pos.distance(pos))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.id.cmp(&b.id))
            })
            .map(|unit| unit.id)
    }

    fn nearest_enemy_unit(&self, owner: u8, pos: Vec2) -> Option<UnitId> {
        self.units
            .iter()
            .filter(|unit| unit.alive && unit.owner != owner && !unit.is_building())
            .min_by(|a, b| {
                a.pos
                    .distance(pos)
                    .partial_cmp(&b.pos.distance(pos))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.id.cmp(&b.id))
            })
            .map(|unit| unit.id)
    }

    /// Lowest-HP living enemy unit within `radius` of `pos` (focus fire).
    fn lowest_hp_enemy_near(&self, owner: u8, pos: Vec2, radius: f32) -> Option<UnitId> {
        self.units
            .iter()
            .filter(|unit| {
                unit.alive
                    && unit.owner != owner
                    && !unit.is_building()
                    && unit.pos.distance(pos) <= radius
            })
            .min_by(|a, b| {
                a.hp
                    .cmp(&b.hp)
                    .then(
                        a.pos
                            .distance(pos)
                            .partial_cmp(&b.pos.distance(pos))
                            .unwrap_or(std::cmp::Ordering::Equal),
                    )
                    .then(a.id.cmp(&b.id))
            })
            .map(|unit| unit.id)
    }

    /// Deterministic placement search: rings of candidate cells around
    /// `center`, keeping the whole footprint on walkable, unoccupied cells.
    pub fn find_build_site(&self, center: Vec2, footprint: u8, max_distance: f32) -> Option<Vec2> {
        self.find_build_site_ignoring(center, footprint, max_distance, None)
    }

    /// True when no living non-building unit is inside the future footprint
    /// (keeps AI structures from boxing workers in).
    fn site_clear_of_units(&self, center: Vec2, side: i32) -> bool {
        let cell = self.grid.cell_size;
        // Half the footprint plus a unit diameter of clearance: the sim refuses
        // a site with any living unit inside its cells, so the AI must not pick
        // one that its own builder is standing in.
        let radius = side as f32 * cell * 0.5 + 96.0;
        !self
            .units
            .iter()
            .any(|unit| unit.alive && !unit.is_building() && unit.pos.distance(center) < radius)
    }

    pub fn find_build_site_ignoring(
        &self,
        center: Vec2,
        footprint: u8,
        max_distance: f32,
        ignore: Option<UnitId>,
    ) -> Option<Vec2> {
        // A site may not cover a gold mine (the placement predicate refuses it).
        // `footprint` is authored at the 32-unit pathing resolution; the AI grid
        // may be coarser, so convert before walking cells.
        let world_side = footprint.max(1) as f32 * crate::PATHING_CELL_SIZE;
        let side = ((world_side / self.grid.cell_size.max(1.0)).ceil() as i32).max(1);
        let cell_size = self.grid.cell_size.max(1.0);
        let base = self.grid.world_to_cell(center);
        let max_ring = ((max_distance / cell_size).ceil() as i32).max(1);
        for ring in 0..=max_ring {
            let mut candidates: Vec<(i32, i32)> = Vec::new();
            for dy in -ring..=ring {
                for dx in -ring..=ring {
                    if dx.abs().max(dy.abs()) != ring {
                        continue;
                    }
                    candidates.push((base.0 as i32 + dx, base.1 as i32 + dy));
                }
            }
            // Deterministic order: sort by (y, x).
            candidates.sort_by_key(|&(x, y)| (y, x));
            for (cx, cy) in candidates {
                let start_x = cx - side / 2;
                let start_y = cy - side / 2;
                let mut ok = true;
                for y in start_y..start_y + side {
                    for x in start_x..start_x + side {
                        if x < 0
                            || y < 0
                            || x >= self.grid.width as i32
                            || y >= self.grid.height as i32
                            || !self.grid.walkable(x as u32, y as u32)
                        {
                            ok = false;
                            break;
                        }
                    }
                    if !ok {
                        break;
                    }
                }
                if !ok {
                    continue;
                }
                // Same rule as `Sim::place_denial`: no gold mine inside the cells.
                if self.gold_mines.iter().any(|mine| {
                    let (mx, my) = self.grid.world_to_cell(mine.pos);
                    (mx as i32) >= start_x
                        && (mx as i32) < start_x + side
                        && (my as i32) >= start_y
                        && (my as i32) < start_y + side
                }) {
                    continue;
                }
                let pos = Vec2::new((cx as f32 + 0.5) * cell_size, (cy as f32 + 0.5) * cell_size);
                let occupied = self.units.iter().any(|unit| {
                    if !unit.alive || Some(unit.id) == ignore {
                        return false;
                    }
                    let (ux, uy) = self.grid.world_to_cell(unit.pos);
                    (ux as i32) >= start_x
                        && (ux as i32) < start_x + side
                        && (uy as i32) >= start_y
                        && (uy as i32) < start_y + side
                });
                if !occupied {
                    return Some(pos);
                }
            }
        }
        None
    }

    /// Army units (non-workers, non-buildings) for tests and HUD.
    pub fn army_units(&self, owner: u8) -> Vec<UnitId> {
        self.units
            .iter()
            .filter(|unit| {
                unit.alive
                    && unit.owner == owner
                    && !unit.is_building()
                    && !unit.def().stats.is_worker
            })
            .map(|unit| unit.id)
            .collect()
    }
}

#[derive(Clone, Copy)]
struct Vec3Stats {
    hp: i32,
    max_hp: i32,
    mana: i32,
}

/// Worker type id for a race, used by callers that only know the race.
pub fn worker_for_race(race: data::Race) -> &'static str {
    match race {
        data::Race::Human => "hpea",
        data::Race::Orc => "opeo",
        data::Race::Undead => "uaco",
        data::Race::NightElf => "ewsp",
        data::Race::Neutral => "hpea",
    }
}

/// Main hall type id for a race.
pub fn hall_for_race(race: data::Race) -> &'static str {
    match race {
        data::Race::Human => "htow",
        data::Race::Orc => "ogre",
        data::Race::Undead => "unpl",
        data::Race::NightElf => "etol",
        data::Race::Neutral => "htow",
    }
}

impl crate::Sim {
    /// Mirrors the AI composition cycle for a race when tests need the def id
    /// the AI would train next.
    pub fn next_ai_def(&self, owner: u8) -> Option<&'static str> {
        let state = self.ai.iter().find(|state| state.owner == owner)?;
        let composition = state.config.composition;
        if composition.is_empty() {
            None
        } else {
            Some(composition[state.train_cycle % composition.len()].0)
        }
    }

    /// True when the AI has issued at least one attack wave.
    pub fn ai_waves(&self, owner: u8) -> u32 {
        self.ai
            .iter()
            .find(|state| state.owner == owner)
            .map(|state| state.waves)
            .unwrap_or(0)
    }
}

/// Expansion hall anchor: 320 units from the mine toward the main base.
///
/// The site search then starts between the base and the mine, so the worker's
/// approach from the base is unobstructed and the mine's blocked cells cannot
/// push the builder outside the channel ring.
fn expansion_anchor(hall: Vec2, mine: Vec2) -> Vec2 {
    let direction = (hall - mine).normalize_or_zero();
    if direction == Vec2::ZERO {
        mine
    } else {
        mine + direction * 320.0
    }
}

/// Rally point on the line from the hall toward the enemy base.  `REGROUP_OFFSET`
/// keeps the army outside the base while it gathers; a degenerate direction
/// (enemy standing on the hall) falls back to the hall itself.
fn regroup_point(hall: Vec2, target: Vec2) -> Vec2 {
    let direction = (target - hall).normalize_or_zero();
    if direction == Vec2::ZERO {
        hall
    } else {
        hall + direction * REGROUP_OFFSET
    }
}

/// Deterministic builder approach direction: stable per unit, no RNG.
fn gazette_dir(unit: UnitId) -> Vec2 {
    const DIRS: [Vec2; 4] = [
        Vec2::new(1.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(-1.0, 0.0),
        Vec2::new(0.0, -1.0),
    ];
    DIRS[(unit.0 as usize) % DIRS.len()]
}

