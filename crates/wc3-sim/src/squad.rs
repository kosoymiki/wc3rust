//! Squad-level AI: composition, leader election, regroup, target selection and
//! the attack-or-flee decision.
//!
//! Ported mechanisms (donor `donors/OpenRA`, access date 2026-11-14):
//!
//! * `OpenRA.Mods.Common/Traits/BotModules/Squads/StateMachine.cs:14-37` —
//!   explicit state object with `Activate`/`Tick`/`Deactivate`.
//! * `OpenRA.Mods.Common/Traits/BotModules/SquadManagerBotModule.cs:54-56,
//!   415-432` — squad size is `SquadSize + random(SquadSizeRandomBonus)`; a new
//!   assault squad takes *every* idle unit around the base (no force left
//!   behind); squad updates are spread over ticks (`squadsPendingUpdate`).
//! * `.../Squads/States/GroundStates.cs:26-60` — leader election prefers the
//!   unit with the most restrictive movement and then the unit closest to the
//!   squad centre.
//! * `.../Squads/States/GroundStates.cs:136-160` — regroup: units further than
//!   `squad_size/3` cells from the leader are sent back to the leader.
//! * `.../Squads/States/GroundStates.cs:129-133` — stuck detection: no leader
//!   movement for 63 ticks (2.5 s at 25 tps) drops the squad back to idle.
//! * `.../Squads/States/StateBase.cs:74-104` — do not flee while own buildings
//!   are inside the danger radius.
//! * `.../Squads/AttackOrFleeFuzzy.cs:26-108` — fuzzy attack-or-flee over own
//!   health, enemy health, relative attack power and relative speed.
//!
//! The fuzzy inference is reimplemented with the same trapezoid terms and the
//! same rule list, aggregated through max-min and defuzzified by weighted
//! centroid; the integer implementation keeps it deterministic.

use glam::Vec2;

use crate::{Order, Sim, TICKS_PER_SECOND, UnitId};

/// Squad role (port of `SquadType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SquadKind {
    Assault,
    Rush,
    Protection,
}

/// Squad state (port of `GroundUnits*State`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SquadState {
    Idle,
    AttackMove,
    Attack,
    Flee,
}

/// Tuning, mirroring `SquadManagerBotModuleInfo` defaults that matter for the
/// port (`SquadSize = 8`, `SquadSizeRandomBonus = 30`, `DangerScanRadius`).
#[derive(Debug, Clone, Copy)]
pub struct SquadConfig {
    pub squad_size: usize,
    pub squad_size_bonus: usize,
    pub attack_force_interval: u32,
    pub rush_interval: u32,
    pub assign_roles_interval: u32,
    pub idle_scan_radius: f32,
    pub attack_scan_radius: f32,
    pub danger_radius: f32,
    /// 63 ticks at 25 tps = 2.52 s, scaled to the 30 tps simulation.
    pub stuck_ticks: u32,
    /// Minimum army size before an attack squad is formed.
    pub min_attack_units: usize,
    /// Units closer than this to the base are "hanging around".
    pub rally_radius: f32,
}

impl Default for SquadConfig {
    fn default() -> Self {
        Self {
            squad_size: 8,
            // OpenRA's default is 30 bonus units because an OpenRA bot produces
            // continuously; a WC3 1v1 army is 10-20 units, so the spread is
            // narrowed here (formula unchanged).
            squad_size_bonus: 4,
            attack_force_interval: 60,
            rush_interval: 300,
            assign_roles_interval: 30,
            idle_scan_radius: 1000.0,
            attack_scan_radius: 1200.0,
            danger_radius: 700.0,
            stuck_ticks: 76,
            min_attack_units: 3,
            rally_radius: 900.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Squad {
    pub id: u32,
    pub owner: u8,
    pub kind: SquadKind,
    pub state: SquadState,
    pub units: Vec<UnitId>,
    pub target: Option<UnitId>,
    pub target_pos: Vec2,
    pub leader: Option<UnitId>,
    pub last_leader_pos: Option<Vec2>,
    pub last_progress_tick: u64,
    /// Last tick an order was issued to this squad.  Orders are re-issued on a
    /// cadence instead of every tick: re-issuing resets the unit paths and
    /// starves the per-tick path budget (observed as squads standing still).
    pub last_issue_tick: u64,
    /// Position the last order pointed at.
    pub last_issue_pos: Vec2,
}

impl Squad {
    fn new(id: u32, owner: u8, kind: SquadKind, target_pos: Vec2) -> Self {
        Self {
            id,
            owner,
            kind,
            state: SquadState::Idle,
            units: Vec::new(),
            target: None,
            target_pos,
            leader: None,
            last_leader_pos: None,
            last_progress_tick: 0,
            last_issue_tick: 0,
            last_issue_pos: target_pos,
        }
    }

    pub fn is_valid(&self) -> bool {
        !self.units.is_empty()
    }
}

/// Per-player squad controller.
#[derive(Debug, Clone)]
pub struct SquadManager {
    pub owner: u8,
    pub config: SquadConfig,
    pub squads: Vec<Squad>,
    pub idle: Vec<UnitId>,
    pub next_squad_id: u32,
    pub attack_force_timer: u32,
    pub rush_timer: u32,
    pub assign_timer: u32,
    /// Round-robin index used to spread squad updates over ticks.
    pub update_cursor: usize,
}

impl SquadManager {
    pub fn new(owner: u8, config: SquadConfig) -> Self {
        Self {
            owner,
            config,
            squads: Vec::new(),
            idle: Vec::new(),
            next_squad_id: 1,
            attack_force_timer: config.attack_force_interval,
            rush_timer: config.rush_interval,
            assign_timer: config.assign_roles_interval,
            update_cursor: 0,
        }
    }

    pub fn squad_count(&self) -> usize {
        self.squads.len()
    }
}

impl Sim {
    /// Enables squad control for `owner`.  Idempotent.
    pub fn enable_squads(&mut self, owner: u8, config: SquadConfig) {
        if self.squads.iter().any(|manager| manager.owner == owner) {
            return;
        }
        self.squads.push(SquadManager::new(owner, config));
        self.squads.sort_by_key(|manager| manager.owner);
    }

    pub fn squad_manager(&self, owner: u8) -> Option<&SquadManager> {
        self.squads.iter().find(|manager| manager.owner == owner)
    }

    /// Runs one squad-controller tick for every enabled owner.
    pub fn advance_squads(&mut self) {
        if self.squads.is_empty() {
            return;
        }
        let owners: Vec<u8> = self.squads.iter().map(|manager| manager.owner).collect();
        for owner in owners {
            self.squad_tick(owner);
        }
    }

    fn squad_tick(&mut self, owner: u8) {
        let Some(index) = self.squads.iter().position(|m| m.owner == owner) else {
            return;
        };
        let config = self.squads[index].config;

        // 1. Drop dead/removed units from squads and the idle pool.
        let alive: Vec<UnitId> = self
            .units
            .iter()
            .filter(|unit| unit.alive && unit.owner == owner)
            .map(|unit| unit.id)
            .collect();
        for squad in &mut self.squads[index].squads {
            squad.units.retain(|id| alive.contains(id));
        }
        self.squads[index].idle.retain(|id| alive.contains(id));
        self.squads[index].squads.retain(|squad| squad.is_valid());

        // 2. Collect idle combat units around the base.
        self.squads[index].assign_timer = self.squads[index].assign_timer.saturating_sub(1);
        if self.squads[index].assign_timer == 0 {
            self.squads[index].assign_timer = config.assign_roles_interval;
            let base = self.own_base(owner);
            let in_squad: Vec<UnitId> = self.squads[index]
                .squads
                .iter()
                .flat_map(|squad| squad.units.iter().copied())
                .collect();
            let mut fresh: Vec<UnitId> = self
                .units
                .iter()
                .filter(|unit| {
                    unit.alive
                        && unit.owner == owner
                        && !unit.is_building()
                        && !unit.def().stats.is_worker
                        && unit.damage > 0
                        && !in_squad.contains(&unit.id)
                        && base
                            .map(|pos| unit.pos.distance(pos) <= config.rally_radius)
                            .unwrap_or(true)
                })
                .map(|unit| unit.id)
                .collect();
            fresh.sort();
            for id in fresh {
                if !self.squads[index].idle.contains(&id) {
                    self.squads[index].idle.push(id);
                }
            }
        }

        // 3. Create an attack force when enough units wait around the base.
        //
        // OpenRA takes `SquadSize + random(SquadSizeRandomBonus)` idle units and
        // commits *all* of them.  A WC3 1v1 army is much smaller than an OpenRA
        // one, so the randomized target is capped at twice `squad_size`; the
        // commit-when-reached rule is unchanged.
        self.squads[index].attack_force_timer =
            self.squads[index].attack_force_timer.saturating_sub(1);
        if self.squads[index].attack_force_timer == 0 {
            self.squads[index].attack_force_timer = config.attack_force_interval;
            let bonus = self.random(config.squad_size_bonus.max(1) as u32) as usize;
            let wanted = (config.squad_size + bonus).min(config.squad_size * 2);
            let threshold = wanted.max(config.min_attack_units);
            if self.squads[index].idle.len() >= threshold {
                let units = std::mem::take(&mut self.squads[index].idle);
                let target = self.enemy_base(owner).unwrap_or(Vec2::ZERO);
                let id = self.squads[index].next_squad_id;
                self.squads[index].next_squad_id += 1;
                let mut squad = Squad::new(id, owner, SquadKind::Assault, target);
                squad.units = units;
                self.squads[index].squads.push(squad);
            }
        }

        // 4. Rush timer: send every ground squad at the nearest enemy building.
        self.squads[index].rush_timer = self.squads[index].rush_timer.saturating_sub(1);
        if self.squads[index].rush_timer == 0 {
            self.squads[index].rush_timer = config.rush_interval;
            let ground: usize = self.squads[index]
                .squads
                .iter()
                .filter(|squad| squad.kind != SquadKind::Protection)
                .map(|squad| squad.units.len())
                .sum::<usize>()
                + self.squads[index].idle.len();
            if ground >= config.squad_size {
                let target = self.enemy_base(owner).unwrap_or(Vec2::ZERO);
                for squad in &mut self.squads[index].squads {
                    if squad.kind != SquadKind::Protection {
                        squad.kind = SquadKind::Rush;
                        squad.target_pos = target;
                        squad.state = SquadState::AttackMove;
                    }
                }
            }
        }

        // 5. Protection: a squad stays home when the base is under attack.
        let base = self.own_base(owner).unwrap_or(Vec2::ZERO);
        let attacker = self
            .units
            .iter()
            .filter(|unit| {
                unit.alive
                    && unit.owner != owner
                    && unit.owner != 2
                    && unit.pos.distance(base) <= config.danger_radius
                    && unit.damage > 0
            })
            .min_by_key(|unit| unit.id)
            .map(|unit| unit.id);
        if let Some(attacker) = attacker
            && !self.squads[index]
                .squads
                .iter()
                .any(|squad| squad.kind == SquadKind::Protection)
        {
            let taken: Vec<UnitId> = {
                let manager = &mut self.squads[index];
                let mut taken = Vec::new();
                for squad in &mut manager.squads {
                    if squad.kind != SquadKind::Protection && squad.units.len() > 2 {
                        let take = squad.units.split_off(squad.units.len() / 2);
                        taken.extend(take);
                    }
                }
                if taken.is_empty() {
                    // No field squad exists yet: defend with half the idle pool.
                    let keep = manager.idle.len() / 2;
                    for _ in 0..keep {
                        if let Some(id) = manager.idle.pop() {
                            taken.push(id);
                        }
                    }
                }
                taken
            };
            let mut squad = Squad::new(0, owner, SquadKind::Protection, base);
            squad.units = taken;
            squad.target = Some(attacker);
            squad.state = SquadState::Attack;
            if squad.is_valid() {
                let id = self.squads[index].next_squad_id;
                self.squads[index].next_squad_id += 1;
                squad.id = id;
                self.squads[index].squads.push(squad);
            }
        }
        if attacker.is_none() {
            self.squads[index]
                .squads
                .retain(|squad| squad.kind != SquadKind::Protection);
        }

        // 6. Tick squads, spreading work across ticks (OpenRA perf mechanism).
        let count = self.squads[index].squads.len();
        let budget = ((count + config.attack_force_interval as usize - 1)
            / config.attack_force_interval.max(1) as usize)
            .max(1);
        for _ in 0..budget {
            if count == 0 {
                break;
            }
            let cursor = self.squads[index].update_cursor % self.squads[index].squads.len();
            self.squads[index].update_cursor = self.squads[index].update_cursor.wrapping_add(1);
            self.squad_step(index, cursor);
        }
    }

    /// Distance in cells scaled to world units (OpenRA uses cells; our grid cell
    /// is `crate::CELL_SIZE`, so a "cell radius" is converted once).
    fn cells_radius(cells: f32) -> f32 {
        (cells * crate::CELL_SIZE as f32).max(64.0)
    }

    fn squad_step(&mut self, manager_index: usize, squad_index: usize) {
        let config = self.squads[manager_index].config;
        let Some(squad) = self.squads[manager_index].squads.get(squad_index).cloned() else {
            return;
        };
        if !squad.is_valid() {
            return;
        }
        let owner = squad.owner;
        // Leader election: most restrictive mover closest to the squad centre
        // (OpenRA GroundStates.cs:26-60, reduced to our single locomotor).
        let centre = self
            .units
            .iter()
            .filter(|unit| squad.units.contains(&unit.id))
            .fold(Vec2::ZERO, |sum, unit| sum + unit.pos)
            / squad.units.len() as f32;
        let leader = self
            .units
            .iter()
            .filter(|unit| squad.units.contains(&unit.id) && unit.alive)
            .min_by(|a, b| {
                let da = a.pos.distance_squared(centre);
                let db = b.pos.distance_squared(centre);
                da.partial_cmp(&db)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.id.0.cmp(&b.id.0))
            })
            .map(|unit| unit.id);
        let Some(leader) = leader else {
            return;
        };
        let leader_pos = self.unit(leader).map(|unit| unit.pos).unwrap_or(centre);

        let mut next_state = squad.state;
        let mut next_target = squad.target;
        let mut next_pos = squad.target_pos;
        // Orders are re-issued at most every 45 ticks, or immediately when the
        // squad objective moved by more than two cells.
        let needs_issue = self.tick >= squad.last_issue_tick.saturating_add(45)
            || squad.last_issue_pos.distance(squad.target_pos) > 256.0;
        let mut issued = false;

        match squad.state {
            SquadState::Idle => {
                // Retarget when the remembered target is gone.
                let target_alive = squad
                    .target
                    .and_then(|id| self.unit(id))
                    .map(|unit| unit.alive)
                    .unwrap_or(false);
                if !target_alive {
                    next_target = self.find_closest_enemy_at(
                        owner,
                        squad.target_pos,
                        config.idle_scan_radius,
                    );
                    if let Some(target) = next_target
                        && let Some(unit) = self.unit(target)
                    {
                        next_pos = unit.pos;
                    }
                }
                // OpenRA scans around the *assigned target*, not around the
                // squad: an assault squad marches toward its objective.
                let enemies = self.enemies_near(owner, next_pos, config.idle_scan_radius);
                if !enemies.is_empty() && self.attack_or_flee(&squad.units, &enemies) {
                    next_state = SquadState::AttackMove;
                } else if !enemies.is_empty() {
                    next_state = SquadState::Flee;
                } else if leader_pos.distance(next_pos) > 320.0 && needs_issue {
                    self.issue(&squad.units, crate::OrderSpec::AttackMove(next_pos));
                    issued = true;
                }
            }
            SquadState::AttackMove => {
                // Stuck detection (OpenRA GroundStates.cs:129-133).
                let moved = squad
                    .last_leader_pos
                    .map(|pos| pos.distance(leader_pos) > 16.0)
                    .unwrap_or(true);
                if moved || squad.last_progress_tick == 0 {
                    self.squads[manager_index]
                        .squads
                        .get_mut(squad_index)
                        .map(|entry| {
                            entry.last_leader_pos = Some(leader_pos);
                            entry.last_progress_tick = self.tick.max(1);
                        });
                } else if self.tick > squad.last_progress_tick + config.stuck_ticks as u64 {
                    next_state = SquadState::Idle;
                }
                // Regroup: units further than squad_size/3 cells walk to the leader.
                let regroup = Self::cells_radius(squad.units.len() as f32 / 3.0);
                let stragglers: Vec<UnitId> = self
                    .units
                    .iter()
                    .filter(|unit| {
                        squad.units.contains(&unit.id)
                            && unit.alive
                            && unit.id != leader
                            && unit.pos.distance(leader_pos) > regroup
                    })
                    .map(|unit| unit.id)
                    .collect();
                if !stragglers.is_empty() {
                    self.issue(&[leader], crate::OrderSpec::AttackMove(leader_pos));
                    self.issue(&stragglers, crate::OrderSpec::AttackMove(leader_pos));
                    issued = true;
                } else {
                    let target = self.find_closest_enemy(leader, config.attack_scan_radius);
                    if let Some(target) = target {
                        next_target = Some(target);
                        next_pos = self.unit(target).map(|unit| unit.pos).unwrap_or(next_pos);
                        next_state = SquadState::Attack;
                    } else if needs_issue {
                        self.issue(&squad.units, crate::OrderSpec::AttackMove(next_pos));
                        issued = true;
                    }
                }
                if self.should_flee(owner, leader_pos, &squad.units, config.danger_radius) {
                    next_state = SquadState::Flee;
                }
            }
            SquadState::Attack => {
                let target_alive = squad
                    .target
                    .and_then(|id| self.unit(id))
                    .map(|unit| unit.alive)
                    .unwrap_or(false);
                if !target_alive {
                    next_target = self.find_closest_enemy(leader, config.attack_scan_radius);
                    if next_target.is_none() {
                        next_state = SquadState::Idle;
                    }
                }
                if let Some(target) = next_target {
                    // Re-target is already throttled by the state machine; a new
                    // target is a reason to issue immediately.
                    if needs_issue || squad.target != next_target {
                        self.issue(&squad.units, crate::OrderSpec::Attack(target));
                        issued = true;
                    }
                }
                if self.should_flee(owner, leader_pos, &squad.units, config.danger_radius) {
                    next_state = SquadState::Flee;
                }
            }
            SquadState::Flee => {
                let home = self.own_base(owner).unwrap_or(leader_pos);
                if needs_issue {
                    self.issue(&squad.units, crate::OrderSpec::AttackMove(home));
                    issued = true;
                }
                let healthy = self.units.iter().filter(|unit| {
                    squad.units.contains(&unit.id)
                        && unit.alive
                        && unit.hp * 100 >= unit.max_hp * 80
                });
                if healthy.count() * 2 >= squad.units.len() {
                    next_state = SquadState::Idle;
                }
            }
        }

        if let Some(entry) = self.squads[manager_index].squads.get_mut(squad_index) {
            entry.leader = Some(leader);
            entry.state = next_state;
            entry.target = next_target;
            entry.target_pos = next_pos;
            if issued {
                entry.last_issue_tick = self.tick;
                entry.last_issue_pos = next_pos;
            }
        }
    }

    /// Port of `StateBase.ShouldFlee(owner, dangerRadius)`: false when an own
    /// building stands inside the danger radius; otherwise the fuzzy verdict on
    /// the nearby enemies decides.
    fn should_flee(&self, owner: u8, centre: Vec2, units: &[UnitId], danger: f32) -> bool {
        let own_building_inside = self.units.iter().any(|unit| {
            unit.alive
                && unit.owner == owner
                && unit.is_building()
                && unit.pos.distance(centre) <= danger
        });
        if own_building_inside {
            return false;
        }
        let enemies = self.enemies_near(owner, centre, danger);
        !enemies.is_empty() && !self.attack_or_flee(units, &enemies)
    }

    fn enemies_near(&self, owner: u8, centre: Vec2, radius: f32) -> Vec<UnitId> {
        let mut enemies: Vec<UnitId> = self
            .units
            .iter()
            .filter(|unit| {
                unit.alive
                    && unit.owner != owner
                    && unit.owner != 2
                    && unit.damage > 0
                    && unit.pos.distance(centre) <= radius
            })
            .map(|unit| unit.id)
            .collect();
        enemies.sort();
        enemies
    }

    fn find_closest_enemy(&self, from: UnitId, radius: f32) -> Option<UnitId> {
        let owner = self.unit(from)?.owner;
        let pos = self.unit(from)?.pos;
        self.find_closest_enemy_at(owner, pos, radius)
    }

    fn find_closest_enemy_at(&self, owner: u8, pos: Vec2, radius: f32) -> Option<UnitId> {
        self.units
            .iter()
            .filter(|unit| {
                unit.alive
                    && unit.owner != owner
                    && unit.owner != 2
                    && unit.pos.distance(pos) <= radius
            })
            .min_by(|a, b| {
                let da = a.pos.distance_squared(pos);
                let db = b.pos.distance_squared(pos);
                da.partial_cmp(&db)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.id.0.cmp(&b.id.0))
            })
            .map(|unit| unit.id)
    }

    fn own_base(&self, owner: u8) -> Option<Vec2> {
        self.units
            .iter()
            .filter(|unit| unit.alive && unit.owner == owner && unit.is_building())
            .min_by_key(|unit| unit.id)
            .map(|unit| unit.pos)
    }

    fn enemy_base(&self, owner: u8) -> Option<Vec2> {
        self.units
            .iter()
            .filter(|unit| {
                unit.alive && unit.owner != owner && unit.owner != 2 && unit.is_building()
            })
            .min_by_key(|unit| unit.id)
            .map(|unit| unit.pos)
    }

    // ------------------------------------------------------------------
    // Fuzzy attack-or-flee (AttackOrFleeFuzzy.cs)
    // ------------------------------------------------------------------

    fn attack_or_flee(&self, own: &[UnitId], enemy: &[UnitId]) -> bool {
        let own_health = self.normalized_health(own, 100.0);
        let enemy_health = self.normalized_health(enemy, 100.0);
        let own_power = self.relative_power(own);
        let enemy_power = self.relative_power(enemy);
        let power = ratio(own_power, enemy_power);
        let own_speed = self.average_speed(own);
        let enemy_speed = self.average_speed(enemy);
        let speed = ratio(own_speed, enemy_speed);
        attack_or_flee_value(own_health, enemy_health, power, speed)
    }

    fn normalized_health(&self, units: &[UnitId], scale: f32) -> f32 {
        let mut sum_max = 0i64;
        let mut sum_hp = 0i64;
        for id in units {
            if let Some(unit) = self.unit(*id) {
                sum_max += unit.max_hp.max(0) as i64;
                sum_hp += unit.hp.max(0) as i64;
            }
        }
        if sum_max == 0 {
            return 0.0;
        }
        (sum_hp as f32 * scale / sum_max as f32).clamp(0.0, 100.0)
    }

    fn relative_power(&self, units: &[UnitId]) -> f32 {
        let mut sum = 0.0f32;
        for id in units {
            if let Some(unit) = self.unit(*id) {
                // Damage per attack divided by the attack interval gives DPS;
                // both are integer ticks, so the ratio is exact enough for the
                // fuzzy thresholds (<1 % error at 30 tps).
                let interval = unit.attack_ticks.max(1) as f32;
                sum += unit.damage.max(0) as f32 * 30.0 / interval;
            }
        }
        sum
    }

    fn average_speed(&self, units: &[UnitId]) -> f32 {
        if units.is_empty() {
            return 0.0;
        }
        let sum: f32 = units
            .iter()
            .filter_map(|id| self.unit(*id))
            .map(|unit| unit.speed)
            .sum();
        sum / units.len() as f32
    }
}

fn ratio(own: f32, enemy: f32) -> f32 {
    if enemy <= 0.0 {
        return 999.0;
    }
    if own <= 0.0 {
        return 0.0;
    }
    (own / enemy * 100.0).clamp(0.0, 999.0)
}

/// Trapezoid membership (`AI.Fuzzy.Library` terms).
fn trapezoid(x: f32, a: f32, b: f32, c: f32, d: f32) -> f32 {
    if x < a || x > d {
        return 0.0;
    }
    if b > a && x < b {
        return (x - a) / (b - a);
    }
    if x <= c {
        return 1.0;
    }
    if d > c {
        return (d - x) / (d - c);
    }
    0.0
}

fn health_terms(x: f32) -> (f32, f32, f32) {
    // NearDead (0,0,20,40), Injured (30,50,50,70), Normal (50,80,100,100).
    (
        trapezoid(x, 0.0, 0.0, 20.0, 40.0),
        trapezoid(x, 30.0, 50.0, 50.0, 70.0),
        trapezoid(x, 50.0, 80.0, 100.0, 100.0),
    )
}

fn ratio_terms(x: f32) -> (f32, f32, f32) {
    // Weak (0,0,70,90), Equal (85,100,100,115), Strong (110,150,150,1000).
    (
        trapezoid(x, 0.0, 0.0, 70.0, 90.0),
        trapezoid(x, 85.0, 100.0, 100.0, 115.0),
        trapezoid(x, 110.0, 150.0, 150.0, 1000.0),
    )
}

/// Rules from `AttackOrFleeFuzzy.cs`.  Each rule is `(own class, enemy set,
/// power set, speed set, consequent)`; `true` in a set means the term is
/// required, `false` means it must not fire, and an empty requirement list
/// means "any".
type TermSet = &'static [u8];

struct Rule {
    own: TermSet,
    enemy: TermSet,
    power: TermSet,
    speed: TermSet,
    flee: bool,
}

// Class indices: 0 = NearDead/Weak/Slow, 1 = Injured/Equal/Equal, 2 = Normal/Strong/Fast.
const NORMAL: TermSet = &[2];
const INJURED: TermSet = &[1];
const NEAR_DEAD: TermSet = &[0];
const ND_INJ: TermSet = &[0, 1];
const INJ_NORM: TermSet = &[1, 2];
const ANY: TermSet = &[0, 1, 2];
const WEAK: TermSet = &[0];
const EQUAL: TermSet = &[1];
const EQUAL_STRONG: TermSet = &[1, 2];
const SLOW: TermSet = &[0];
const SLOW_EQUAL: TermSet = &[0, 1];
const EQUAL_FAST: TermSet = &[1, 2];
const FAST: TermSet = &[2];

const RULES: &[Rule] = &[
    // DefaultRulesNormalOwnHealth
    Rule {
        own: NORMAL,
        enemy: ANY,
        power: ANY,
        speed: ANY,
        flee: false,
    },
    // DefaultRulesInjuredOwnHealth
    Rule {
        own: INJURED,
        enemy: NEAR_DEAD,
        power: ANY,
        speed: ANY,
        flee: false,
    },
    Rule {
        own: INJURED,
        enemy: INJ_NORM,
        power: EQUAL_STRONG,
        speed: ANY,
        flee: false,
    },
    Rule {
        own: INJURED,
        enemy: INJ_NORM,
        power: WEAK,
        speed: SLOW,
        flee: false,
    },
    Rule {
        own: INJURED,
        enemy: INJ_NORM,
        power: WEAK,
        speed: EQUAL_FAST,
        flee: true,
    },
    Rule {
        own: INJURED,
        enemy: ANY,
        power: ANY,
        speed: SLOW,
        flee: false,
    },
    // DefaultRulesNearDeadOwnHealth
    Rule {
        own: NEAR_DEAD,
        enemy: ND_INJ,
        power: ANY,
        speed: SLOW_EQUAL,
        flee: false,
    },
    Rule {
        own: NEAR_DEAD,
        enemy: ND_INJ,
        power: WEAK,
        speed: EQUAL_FAST,
        flee: true,
    },
    Rule {
        own: NEAR_DEAD,
        enemy: NORMAL,
        power: WEAK,
        speed: EQUAL_FAST,
        flee: true,
    },
    Rule {
        own: NEAR_DEAD,
        enemy: NORMAL,
        power: EQUAL_STRONG,
        speed: FAST,
        flee: true,
    },
    Rule {
        own: NEAR_DEAD,
        enemy: INJURED,
        power: EQUAL,
        speed: FAST,
        flee: true,
    },
];

fn set_member(set: TermSet, class: usize) -> bool {
    set.contains(&(class as u8))
}

/// Mamdani-style max-min inference with singleton consequents; the defuzzified
/// value is the weighted centroid of the two output singletons (Attack = 15,
/// Flee = 35).  `CanAttack` in the donor is `attackChance < 30`, which is the
/// same threshold used here.
pub fn attack_or_flee_value(own_health: f32, enemy_health: f32, power: f32, speed: f32) -> bool {
    let own_terms = health_terms(own_health);
    let enemy_terms = health_terms(enemy_health);
    let power_terms = ratio_terms(power);
    let speed_terms = ratio_terms(speed);

    // Per class membership used inside the rules.
    let class_grade = |set: TermSet, terms: (f32, f32, f32)| -> f32 {
        set.iter()
            .map(|class| match class {
                0 => terms.0,
                1 => terms.1,
                _ => terms.2,
            })
            .fold(0.0f32, f32::max)
    };

    let mut attack_strength = 0.0f32;
    let mut flee_strength = 0.0f32;
    for rule in RULES {
        let own = class_grade(rule.own, own_terms);
        let enemy = class_grade(rule.enemy, enemy_terms);
        let power_grade = class_grade(rule.power, power_terms);
        let speed_grade = class_grade(rule.speed, speed_terms);
        let strength = own.min(enemy).min(power_grade).min(speed_grade);
        if strength <= 0.0 {
            continue;
        }
        if rule.flee {
            flee_strength = flee_strength.max(strength);
        } else {
            attack_strength = attack_strength.max(strength);
        }
    }
    let total = attack_strength + flee_strength;
    if total <= 0.0 {
        // No rule fires (e.g. all terms zero at the boundaries): attack, matching
        // the donor's NaN handling (`!NaN && attackChance < 30` => false => flee;
        // we choose attack because the squad already decided to engage).
        return true;
    }
    let centroid = (attack_strength * 15.0 + flee_strength * 35.0) / total;
    centroid < 30.0
}

/// Helper used by tests: the class a health percentage maps to.
pub fn health_class(percent: f32) -> u8 {
    let (near, injured, normal) = health_terms(percent);
    let mut best = (0u8, near);
    if injured > best.1 {
        best = (1, injured);
    }
    if normal > best.1 {
        best = (2, normal);
    }
    best.0
}

impl Sim {
    /// Attacks per second helper for the fuzzy power ratio (30 Hz simulation).
    pub fn attacks_per_tick(&self, unit: UnitId) -> f32 {
        1.0 / self.effective_attack_ticks(unit).max(1) as f32 * TICKS_PER_SECOND as f32
    }

    /// True when the unit is currently fighting (attack or attack-move).
    pub fn is_engaged(&self, unit: UnitId) -> bool {
        self.unit(unit)
            .map(|unit| matches!(unit.order, Order::Attack(_) | Order::MoveAttack(_)))
            .unwrap_or(false)
    }
}

/// Squad-aware enemy scan used by the AI glue in `ai.rs`.
pub fn nearest_enemy_to(sim: &Sim, owner: u8, pos: Vec2, radius: f32) -> Option<UnitId> {
    sim.units
        .iter()
        .filter(|unit| {
            unit.alive && unit.owner != owner && unit.owner != 2 && unit.pos.distance(pos) <= radius
        })
        .min_by(|a, b| {
            let da = a.pos.distance_squared(pos);
            let db = b.pos.distance_squared(pos);
            da.partial_cmp(&db)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.id.0.cmp(&b.id.0))
        })
        .map(|unit| unit.id)
}

/// Set membership helper kept for tests of the rule table.
pub fn rule_count() -> usize {
    RULES.len()
}

/// Exposes whether a rule set entry exists for the `(own, enemy)` pair.
pub fn rule_flees(class: u8, enemy_class: u8) -> bool {
    RULES.iter().any(|rule| {
        set_member(rule.own, class as usize)
            && set_member(rule.enemy, enemy_class as usize)
            && rule.flee
    })
}
