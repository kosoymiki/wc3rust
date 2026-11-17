//! mechanic `economy.upkeep`: WC3 upkeep tax at the 50/80 food breakpoints.

use wc3_sim::{Sim, UpkeepTier, Vec2};

fn food_exact(amount: i32) -> Sim {
    let mut sim = Sim::new(500 + amount as u64, 64, 64);
    sim.passive_income = false;
    sim.ruleset = wc3_sim::Ruleset::Tft;
    for index in 0..amount {
        let pos = Vec2::new(
            200.0 + (index % 8) as f32 * 40.0,
            200.0 + (index / 8) as f32 * 40.0,
        );
        sim.spawn_def(0, "hpea", pos).expect("peasant");
    }
    assert_eq!(sim.supply_used(0), amount);
    sim
}

#[test]
fn upkeep_tax_boundaries_50_80() {
    let cases = [
        (49, UpkeepTier::None, 100),
        (50, UpkeepTier::None, 100),
        (51, UpkeepTier::Low, 70),
        (79, UpkeepTier::Low, 70),
        (80, UpkeepTier::Low, 70),
        (81, UpkeepTier::High, 40),
    ];
    for (food, tier, paid) in cases {
        let mut sim = food_exact(food);
        assert_eq!(sim.upkeep(0), tier, "tier at {food} food");
        assert_eq!(sim.upkeep_label(0), tier.label(), "label at {food} food");
        assert_eq!(sim.upkeep_permille(0), tier.income_permille());
        assert_eq!(sim.deposit_gold(0, 100), paid, "gold at {food} food");
        assert_eq!(sim.deposit_lumber(0, 100), paid, "lumber at {food} food");
        assert_eq!(sim.resources[0], 1000 + paid);
        assert_eq!(sim.lumber[0], 1000 + paid);
    }
}
