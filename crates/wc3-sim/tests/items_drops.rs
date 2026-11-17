//! mechanic `items.drops`: neutral creep deaths use the retail drop table.

use wc3_sim::{AttackType, Sim, Vec2, data};

#[test]
fn items_drops_creep_loot() {
    let mut sim = Sim::new(0xD20, 64, 64);
    sim.passive_income = false;
    let hero = sim
        .spawn_def(0, "Hpal", Vec2::new(600.0, 600.0))
        .expect("hero");
    let mut creeps = Vec::new();
    for index in 0..40 {
        let pos = Vec2::new(
            800.0 + (index % 8) as f32 * 60.0,
            800.0 + (index / 8) as f32 * 60.0,
        );
        if let Some(creep) = sim.spawn_def(2, "nftr", pos) {
            creeps.push(creep);
        }
    }
    for creep in creeps {
        let _ = sim.apply_damage(hero, creep, 100_000, AttackType::Magic);
    }
    assert!(!sim.items.is_empty(), "creep drop table produced loot");
    for item in &sim.items {
        let def = data::ITEMS[item.def_index as usize];
        assert!(def.droppable, "dropped item is droppable");
        assert!((1..=10).contains(&def.level), "retail drop level");
    }
}
