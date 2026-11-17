//! Resource shelf model (mechanic group `economy.*` + the HUD display path).
//!
//! All values come from the canonical `Sim` state: gold/lumber banks, supply
//! used/cap, upkeep tier (TFT 50/80 thresholds), idle workers and the damage
//! events that feed the "unit under attack" alert. No generic placeholders are
//! produced when the sim has the value.

use glam::Vec2;
use wc3_sim::{Event, Ruleset, Sim, UpkeepTier, MAX_SUPPLY, UPKEEP_HIGH_FOOD, UPKEEP_LOW_FOOD};

#[derive(Clone, Debug, PartialEq)]
pub struct ResourceView {
    pub gold: i32,
    pub lumber: i32,
    pub supply_used: i32,
    pub supply_cap: i32,
    pub upkeep: UpkeepTier,
    pub idle_workers: usize,
    /// Positions of owned units that took damage this tick (deduplicated).
    pub under_attack: Vec<Vec2>,
}

/// Upkeep tier for a supply count: 0-50 none, 51-80 low, 81+ high.
pub fn upkeep_tier_for(supply_used: i32) -> UpkeepTier {
    if supply_used <= UPKEEP_LOW_FOOD {
        UpkeepTier::None
    } else if supply_used <= UPKEEP_HIGH_FOOD {
        UpkeepTier::Low
    } else {
        UpkeepTier::High
    }
}

/// Supply ceiling per ruleset (RoC 90, TFT 100), matching `Sim::supply_cap`.
pub fn supply_cap_for(ruleset: Ruleset) -> i32 {
    match ruleset {
        Ruleset::Roc => 90,
        Ruleset::Tft => MAX_SUPPLY,
    }
}

/// Idle-worker button label when at least one worker is idle.
pub fn idle_worker_action(idle: usize) -> Option<&'static str> {
    (idle > 0).then_some("SELECT IDLE WORKER")
}

/// Owned units that took damage in the current event batch.
pub fn under_attack_positions(sim: &Sim, player: u8) -> Vec<Vec2> {
    let mut out: Vec<Vec2> = Vec::new();
    for event in &sim.events {
        let Event::Damage { to, .. } = event else {
            continue;
        };
        let Some(unit) = sim.unit(*to) else {
            continue;
        };
        if unit.owner != player || !unit.alive {
            continue;
        }
        if !out.iter().any(|pos| pos.distance_squared(unit.pos) < 1.0) {
            out.push(unit.pos);
        }
    }
    out
}

/// Snapshot for the resource shelf + idle button + alert badge.
pub fn resource_view(sim: &Sim, player: u8) -> ResourceView {
    let index = player as usize;
    ResourceView {
        gold: sim.resources.get(index).copied().unwrap_or(0),
        lumber: sim.lumber.get(index).copied().unwrap_or(0),
        supply_used: sim.supply_used(player),
        supply_cap: sim.supply_cap(player),
        upkeep: sim.upkeep(player),
        idle_workers: sim.idle_workers(player).len(),
        under_attack: under_attack_positions(sim, player),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wc3_sim::{OrderSpec, Vec2};

    fn sim_with_units() -> Sim {
        let mut sim = Sim::new(0xE0E0, 64, 64);
        sim.passive_income = false;
        sim
    }

    #[test]
    fn economy_upkeep_ui_tiers_boundaries() {
        for (food, tier) in [
            (0, UpkeepTier::None),
            (49, UpkeepTier::None),
            (50, UpkeepTier::None),
            (51, UpkeepTier::Low),
            (79, UpkeepTier::Low),
            (80, UpkeepTier::Low),
            (81, UpkeepTier::High),
            (100, UpkeepTier::High),
        ] {
            assert_eq!(upkeep_tier_for(food), tier, "food {food}");
        }
        assert_eq!(UpkeepTier::None.label(), "NO UPKEEP");
        assert_eq!(UpkeepTier::Low.label(), "LOW UPKEEP");
        assert_eq!(UpkeepTier::High.label(), "HIGH UPKEEP");
    }

    #[test]
    fn economy_supply_ui_caps_by_ruleset() {
        assert_eq!(supply_cap_for(Ruleset::Roc), 90);
        assert_eq!(supply_cap_for(Ruleset::Tft), 100);
        let mut sim = sim_with_units();
        sim.set_ruleset(Ruleset::Roc);
        assert!(sim.supply_cap(0) <= supply_cap_for(Ruleset::Roc));
        sim.set_ruleset(Ruleset::Tft);
        assert!(sim.supply_cap(0) <= supply_cap_for(Ruleset::Tft));
    }

    #[test]
    fn ui_resources_view_reads_sim_values() {
        let mut sim = sim_with_units();
        sim.resources[0] = 733;
        sim.lumber[0] = 241;
        let view = resource_view(&sim, 0);
        assert_eq!(view.gold, 733);
        assert_eq!(view.lumber, 241);
        assert_eq!(view.supply_cap, sim.supply_cap(0));
        assert!(view.supply_used >= 0 && view.supply_used <= view.supply_cap);
        assert_eq!(view.upkeep, sim.upkeep(0));
        assert_eq!(view.idle_workers, 0);
        assert!(view.under_attack.is_empty());
    }

    #[test]
    fn ui_resources_idle_worker_button_follows_orders() {
        let mut sim = sim_with_units();
        let peasant = sim
            .spawn_def(0, "hpea", Vec2::new(600.0, 600.0))
            .expect("peasant");
        assert_eq!(resource_view(&sim, 0).idle_workers, 1);
        assert_eq!(idle_worker_action(1), Some("SELECT IDLE WORKER"));
        sim.issue(&[peasant], OrderSpec::Move(Vec2::new(1400.0, 600.0)));
        sim.step(&[]);
        assert_eq!(resource_view(&sim, 0).idle_workers, 0);
        assert_eq!(idle_worker_action(0), None);
    }

    #[test]
    fn ui_resources_under_attack_alert_from_damage_events() {
        let mut sim = sim_with_units();
        let defender = sim
            .spawn_def(0, "hfoo", Vec2::new(600.0, 600.0))
            .expect("defender");
        let attacker = sim
            .spawn_def(1, "hfoo", Vec2::new(700.0, 600.0))
            .expect("attacker");
        sim.issue(&[attacker], OrderSpec::Attack(defender));
        let mut alerted = Vec::new();
        for _ in 0..600 {
            sim.step(&[]);
            let alerts = under_attack_positions(&sim, 0);
            if !alerts.is_empty() {
                alerted = alerts;
                break;
            }
            sim.events.clear();
        }
        assert!(!alerted.is_empty(), "damage event produced an alert");
        let defender_pos = sim.unit(defender).expect("defender alive").pos;
        assert!(
            alerted
                .iter()
                .any(|pos| pos.distance_squared(defender_pos) < 1.0),
            "alert points at the damaged unit: {alerted:?} vs {defender_pos:?}"
        );
        // The attacker's owner sees its own damaged unit (retaliation), never
        // the defender's position as its alert.
        for pos in under_attack_positions(&sim, 1) {
            assert!(pos.distance_squared(defender_pos) >= 1.0);
        }
    }
}
