//! Creep camp support from `war3mapUnits.doo` neutral-aggressive placements.
//!
//! Reads player-12 (`PLAYER_NEUTRAL_AGGRESSIVE`) placed units, clusters them
//! into camps by proximity, and resolves each camp against JASS world units
//! already spawned by [`Engine::spawn_map_entities`](crate::engine::Engine).
//! The returned [`CreepCamp`] carries JASS handles plus the sim-facing def
//! (anchor, aggro, leash, members) that `wc3-sim::Sim::camps` consumes.
//!
//! No units are created here: `war3mapUnits.doo` placement spawning into the
//! JASS [`World`](crate::jass::vm::World) is owned by `Engine`. This module
//! only *recognizes* the neutral-aggressive subset as camps, so sim can adopt
//! the same JASS handles without duplicating units.

use std::collections::BTreeSet;

use crate::engine::Engine;
use crate::error::{ReError, Result};
use crate::fourcc_str;
use wc3_formats::map::PlacedUnit;

/// `PLAYER_NEUTRAL_AGGRESSIVE` from `Scripts\common.j`.
pub const PLAYER_NEUTRAL_AGGRESSIVE: u32 = 12;

/// Default creep-camp aggro radius (WC3 default 500 map units).
pub const DEFAULT_AGGRO: f32 = 500.0;
/// Default creep-camp leash radius (WC3 default 500 map units).
pub const DEFAULT_LEASH: f32 = 500.0;
/// Radius within which two neutral-aggressive placements belong to one camp.
pub const DEFAULT_CLUSTER_RADIUS: f32 = 300.0;

/// One member of a creep camp: rawcode and world position.
#[derive(Debug, Clone)]
pub struct CampMember {
    pub rawcode: String,
    pub pos: [f32; 2],
}

/// Sim-facing definition of one creep camp.
#[derive(Debug, Clone)]
pub struct CampDef {
    pub anchor: [f32; 2],
    pub members: Vec<CampMember>,
    pub aggro: f32,
    pub leash: f32,
}

/// One spawned/resolved creep camp: JASS handles + sim-facing def.
#[derive(Debug, Clone)]
pub struct CreepCamp {
    pub anchor: [f32; 2],
    pub units: Vec<u32>, // JASS handles of the camp's units
    pub aggro: f32,
    pub leash: f32,
}

/// Cluster neutral-aggressive placements into camp definitions.
///
/// Greedy: each unclustered placement seeds a camp anchored at its position;
/// every unclustered neutral-aggressive placement within `cluster_radius` of
/// the anchor joins it. Only `player == PLAYER_NEUTRAL_AGGRESSIVE` placements
/// are considered.
pub fn camp_defs(placements: &[PlacedUnit], cluster_radius: f32) -> Vec<CampDef> {
    let neutral: Vec<&PlacedUnit> = placements
        .iter()
        .filter(|u| u.player == PLAYER_NEUTRAL_AGGRESSIVE)
        .collect();
    let mut assigned = BTreeSet::new();
    let mut camps = Vec::new();
    for (index, seed) in neutral.iter().enumerate() {
        if assigned.contains(&index) {
            continue;
        }
        assigned.insert(index);
        let anchor = [seed.position.x, seed.position.y];
        let mut members = Vec::new();
        members.push(CampMember {
            rawcode: fourcc_str(&seed.type_id),
            pos: anchor,
        });
        for (j, other) in neutral.iter().enumerate().skip(index + 1) {
            if assigned.contains(&j) {
                continue;
            }
            let dx = other.position.x - anchor[0];
            let dy = other.position.y - anchor[1];
            if dx.hypot(dy) <= cluster_radius {
                assigned.insert(j);
                members.push(CampMember {
                    rawcode: fourcc_str(&other.type_id),
                    pos: [other.position.x, other.position.y],
                });
            }
        }
        camps.push(CampDef {
            anchor,
            members,
            aggro: DEFAULT_AGGRO,
            leash: DEFAULT_LEASH,
        });
    }
    camps
}

/// Resolve camp definitions against JASS world units spawned by `Engine`.
///
/// Matches each `CampDef` member to an existing JASS unit by rawcode +
/// position (within 4.0 map units, same tolerance as `spawn_map_entities`).
/// Units with no matching JASS unit are skipped and recorded in `unmatched`.
pub fn resolve_camps(
    runtime: &crate::jass::vm::Runtime,
    camps: &[CampDef],
) -> (Vec<CreepCamp>, Vec<String>) {
    let mut resolved = Vec::new();
    let mut unmatched = Vec::new();
    for camp in camps {
        let mut units = Vec::new();
        for member in &camp.members {
            let found = runtime.world.units.iter().find(|(_, unit)| {
                unit.id.eq_ignore_ascii_case(&member.rawcode)
                    && (unit.x - member.pos[0] as f64).abs() < 4.0
                    && (unit.y - member.pos[1] as f64).abs() < 4.0
            });
            match found {
                Some((handle, _)) => units.push(*handle),
                None => unmatched.push(format!("{} {:?}", member.rawcode, member.pos)),
            }
        }
        resolved.push(CreepCamp {
            anchor: camp.anchor,
            units,
            aggro: camp.aggro,
            leash: camp.leash,
        });
    }
    (resolved, unmatched)
}

impl Engine {
    /// Analyze `war3mapUnits.doo` placements from the last `enter_map` and
    /// resolve neutral-aggressive creep camps against the JASS world.
    ///
    /// Returns `Ok(Vec<CreepCamp>)`. Callers (e.g. `wc3-game`'s bridge) adopt
    /// the JASS handles into `wc3-sim::Sim::camps`.
    pub fn creep_camps(&mut self) -> Result<Vec<CreepCamp>> {
        let placements = self
            .map()
            .ok_or_else(|| ReError::Unsupported("no map loaded for creep camps".into()))?
            .units
            .as_ref()
            .ok_or_else(|| ReError::Unsupported("no war3mapUnits.doo placements".into()))?
            .clone();
        let defs = camp_defs(&placements, DEFAULT_CLUSTER_RADIUS);
        let (camps, unmatched) = {
            let runtime = self
                .runtime()
                .ok_or_else(|| ReError::Unsupported("no JASS runtime for creep camps".into()))?;
            resolve_camps(runtime, &defs)
        };
        if !unmatched.is_empty() {
            self.script_errors.extend(unmatched);
        }
        Ok(camps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jass::vm::Runtime;

    fn placed(rawcode: &str, x: f32, y: f32, player: u32) -> PlacedUnit {
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(rawcode.as_bytes());
        PlacedUnit {
            type_id: bytes,
            variation: 0,
            position: glam::Vec3::new(x, y, 0.0),
            rotation: 0.0,
            scale: glam::Vec3::ONE,
            player,
            hp: 100,
            mp: 0,
            hero_level: 1,
        }
    }

    #[test]
    fn camp_defs_clusters_neutral_aggressive() {
        let placements = vec![
            placed("ngna", 100.0, 100.0, PLAYER_NEUTRAL_AGGRESSIVE),
            placed("nftr", 110.0, 105.0, PLAYER_NEUTRAL_AGGRESSIVE), // near ngna
            placed("nenf", 1000.0, 1000.0, PLAYER_NEUTRAL_AGGRESSIVE), // separate camp
            placed("hfoo", 200.0, 200.0, 1), // player 1: ignored
        ];
        let camps = camp_defs(&placements, DEFAULT_CLUSTER_RADIUS);
        assert_eq!(camps.len(), 2, "two camps: clustered + lone");
        let first = camps
            .iter()
            .find(|c| c.members.len() == 2)
            .expect("clustered camp");
        let codes: BTreeSet<_> = first.members.iter().map(|m| m.rawcode.as_str()).collect();
        assert_eq!(codes, BTreeSet::from(["ngna", "nftr"]));
        let second = camps
            .iter()
            .find(|c| c.members.len() == 1)
            .expect("lone camp");
        assert_eq!(second.members[0].rawcode, "nenf");
        assert!(camps.iter().all(|c| c.aggro == DEFAULT_AGGRO));
        assert!(camps.iter().all(|c| c.leash == DEFAULT_LEASH));
    }

    #[test]
    fn resolve_camps_matches_jass_units() {
        let mut runtime = Runtime::new(crate::jass::Program::default(), 1);
        let ngna = runtime.alloc();
        let nftr = runtime.alloc();
        runtime.world.units.insert(
            ngna,
            crate::jass::vm::Unit {
                id: "ngna".into(),
                owner: PLAYER_NEUTRAL_AGGRESSIVE as i32,
                x: 100.0,
                y: 100.0,
                ..crate::jass::vm::Unit::default()
            },
        );
        runtime.world.units.insert(
            nftr,
            crate::jass::vm::Unit {
                id: "nftr".into(),
                owner: PLAYER_NEUTRAL_AGGRESSIVE as i32,
                x: 110.0,
                y: 105.0,
                ..crate::jass::vm::Unit::default()
            },
        );
        let defs = vec![CampDef {
            anchor: [100.0, 100.0],
            members: vec![
                CampMember {
                    rawcode: "ngna".into(),
                    pos: [100.0, 100.0],
                },
                CampMember {
                    rawcode: "nftr".into(),
                    pos: [110.0, 105.0],
                },
            ],
            aggro: DEFAULT_AGGRO,
            leash: DEFAULT_LEASH,
        }];
        let (camps, unmatched) = resolve_camps(&runtime, &defs);
        assert_eq!(camps.len(), 1);
        assert_eq!(camps[0].units.len(), 2, "both JASS units matched");
        assert!(camps[0].units.contains(&ngna) && camps[0].units.contains(&nftr));
        assert!(unmatched.is_empty(), "unmatched: {unmatched:?}");
    }

    #[test]
    fn player_one_units_ignored() {
        let placements = vec![placed("hfoo", 100.0, 100.0, 1)];
        assert!(camp_defs(&placements, DEFAULT_CLUSTER_RADIUS).is_empty());
    }
}
