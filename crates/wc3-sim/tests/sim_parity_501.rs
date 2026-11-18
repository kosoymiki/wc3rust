//! Mechanic tests for the sim-parity lane (ledger 501), 2026-11-18.
//!
//! Every test here reproduces a player-visible gap found by the census in
//! `analysis/agent-sim/SIM-GAPS-2026-11-18.md` and pins the fix.  Test names
//! carry the mechanic id; numbers come from the shipped SLK rows
//! (`UnitData.slk` costs/build times, `*UnitFunc.txt` `Upgrade=` chains), never
//! from a literal chosen by the test.
//!
//! Mechanic ids covered:
//! * `sim.building.upgrade-hall-tier`      — hall line upgrades in place
//! * `sim.building.upgrade-chain-all-races`— every race's `Upgrade=` chain
//! * `sim.building.upgrade-predecessor`    — order enforced by the data chain
//! * `sim.building.upgrade-save-load`      — in-flight upgrade survives a save
//! * `sim.orders.patrol-command`           — patrol through the command stream
//! * `sim.orders.research-command`         — research through the command stream
//! * `sim.victory.elimination-rule`        — defeat = no buildings left

use wc3_sim::{Command, Event, Order, OrderSpec, Sim, UnitId, Vec2, data, def_of};

fn hall(sim: &mut Sim, owner: u8, def_id: &str, pos: Vec2) -> UnitId {
    sim.spawn_def(owner, def_id, pos).expect("hall")
}

/// Fast sim with one hall per side and a bank: upgrades and research need
/// gold and lumber from the real rows, so the bank is only a test affordance.
fn sandbox() -> Sim {
    let mut sim = Sim::new(0x501, 32, 32);
    sim.passive_income = false;
    sim.resources = [50_000, 50_000];
    sim.lumber = [50_000, 50_000];
    hall(&mut sim, 0, "htow", Vec2::new(300.0, 300.0));
    hall(&mut sim, 1, "ogre", Vec2::new(700.0, 700.0));
    sim.step(&[]);
    sim
}

fn tick_until_done(sim: &mut Sim, building: UnitId, limit: u32) -> u32 {
    for tick in 0..limit {
        sim.step(&[]);
        if sim.unit(building).is_some_and(|unit| unit.upgrade.is_none()) {
            return tick + 1;
        }
    }
    panic!("upgrade of {building:?} did not finish in {limit} ticks");
}

// ---------------------------------------------------------------------------
// Building upgrades
// ---------------------------------------------------------------------------

#[test]
fn sim_building_upgrade_hall_tier_changes_def_in_place() {
    let mut sim = sandbox();
    let hall = sim.units_of(0).next().expect("hall").id;
    let keeper = def_of("hkee").expect("Keep row");
    let gold_before = sim.resources[0];
    let lumber_before = sim.lumber[0];

    assert_eq!(
        sim.upgrade_denial(hall, "hkee"),
        None,
        "a Town Hall must accept its own `Upgrade=` row"
    );
    sim.upgrade_building(hall, "hkee").expect("start upgrade");

    // Cost is the target row, not a literal.
    assert_eq!(sim.resources[0], gold_before - keeper.stats.cost);
    assert_eq!(sim.lumber[0], lumber_before - keeper.stats.lumber_cost);
    assert!(
        sim.unit(hall).expect("hall").upgrade.is_some(),
        "job must be visible on the unit"
    );

    let ticks = tick_until_done(&mut sim, hall, keeper.stats.build_ticks + 5);
    assert_eq!(
        ticks, keeper.stats.build_ticks,
        "the job takes the target's `build_ticks`"
    );
    let unit = sim.unit(hall).expect("hall");
    assert_eq!(unit.def().id, "hkee", "def swapped in place");
    assert_eq!(unit.id, hall, "identity preserved");
    assert_eq!(unit.max_hp, keeper.stats.hp);
    assert!(
        sim.events
            .iter()
            .any(|event| matches!(event, Event::UpgradeComplete { building, to } if *building == hall && *to == "hkee")),
        "UpgradeComplete must fire for audio/HUD: {:?}",
        sim.events
    );
}

#[test]
fn sim_building_upgrade_chain_all_races_reaches_the_final_tier() {
    // `[htow] Upgrade=hkee`, `[hkee] Upgrade=hcas`, and the orc/undead/nightelf
    // equivalents from `*UnitFunc.txt`.
    let chains: [(&str, [&str; 2]); 4] = [
        ("htow", ["hkee", "hcas"]),
        ("ogre", ["ostr", "ofrt"]),
        ("unpl", ["unp1", "unp2"]),
        ("etol", ["etoa", "etoe"]),
    ];
    for (base, [tier2, tier3]) in chains {
        let mut sim = Sim::new(0x501, 32, 32);
        sim.passive_income = false;
        sim.resources = [50_000, 50_000];
        sim.lumber = [50_000, 50_000];
        hall(&mut sim, 0, base, Vec2::new(300.0, 300.0));
        hall(&mut sim, 1, "htow", Vec2::new(900.0, 900.0));
        sim.step(&[]);
        let id = sim.units_of(0).next().expect("hall").id;

        assert_eq!(
            data::upgrade_targets_of(base),
            vec![tier2],
            "{base} has exactly one `Upgrade=` target"
        );
        sim.upgrade_building(id, tier2)
            .unwrap_or_else(|err| panic!("{base} -> {tier2}: {err}"));
        tick_until_done(&mut sim, id, def_of(tier2).unwrap().stats.build_ticks + 5);
        assert_eq!(sim.unit(id).unwrap().def().id, tier2);

        sim.upgrade_building(id, tier3)
            .unwrap_or_else(|err| panic!("{tier2} -> {tier3}: {err}"));
        tick_until_done(&mut sim, id, def_of(tier3).unwrap().stats.build_ticks + 5);
        assert_eq!(
            sim.unit(id).unwrap().def().id,
            tier3,
            "{base} line must reach {tier3}"
        );
    }
}

#[test]
fn sim_building_upgrade_predecessor_is_enforced_by_the_data_chain() {
    let mut sim = sandbox();
    let hall = sim.units_of(0).next().expect("hall").id;
    assert_eq!(
        sim.upgrade_denial(hall, "hcas"),
        Some("not an upgrade of this building"),
        "Castle needs a Keep first ([hkee] Upgrade=hcas)"
    );
    sim.upgrade_building(hall, "hcas").expect_err("skipping a tier");
    // Same rule for a tower: the base form upgrades, a finished tower does not
    // turn into a hall.
    let tower = sim
        .spawn_def(0, "hwtw", Vec2::new(500.0, 300.0))
        .expect("scout tower");
    assert_eq!(
        sim.upgrade_denial(tower, "hkee"),
        Some("not an upgrade of this building")
    );
    assert_eq!(
        data::upgrade_targets_of("hwtw"),
        vec!["hgtw", "hctw", "hatw"],
        "`[hwtw] Upgrade=hgtw,hctw,hatw` in HumanUnitFunc.txt"
    );
}

#[test]
fn sim_building_upgrade_preserves_damage_ratio() {
    let mut sim = sandbox();
    let hall = sim.units_of(0).next().expect("hall").id;
    let max_before = sim.unit(hall).unwrap().max_hp;
    sim.set_unit_hp(hall, max_before / 2);
    sim.upgrade_building(hall, "hkee").expect("start upgrade");
    tick_until_done(&mut sim, hall, def_of("hkee").unwrap().stats.build_ticks + 5);
    let unit = sim.unit(hall).unwrap();
    let expected = (unit.max_hp as f32 * 0.5).round() as i32;
    assert!(
        (unit.hp - expected).abs() <= 1,
        "half-dead Town Hall must become a half-dead Keep: hp={} expected≈{expected}",
        unit.hp
    );
}

#[test]
fn sim_building_upgrade_survives_a_save_load_round_trip() {
    let mut sim = sandbox();
    let hall = sim.units_of(0).next().expect("hall").id;
    sim.upgrade_building(hall, "hkee").expect("start upgrade");
    sim.step(&[]);
    let bytes = sim.to_save_bytes();
    let mut loaded = Sim::from_save(&bytes).expect("save round trip");
    assert!(
        loaded.unit(hall).expect("hall").upgrade.is_some(),
        "in-flight upgrade must be restored"
    );
    assert_eq!(loaded.state_hash(), sim.state_hash(), "identical state");
    // Both halves finish on the same tick and end on the same def.
    let remain = sim.unit(hall).unwrap().upgrade.unwrap().remaining;
    for _ in 0..remain + 2 {
        sim.step(&[]);
        loaded.step(&[]);
    }
    assert_eq!(sim.unit(hall).unwrap().def().id, "hkee");
    assert_eq!(
        loaded.unit(hall).unwrap().def().id,
        "hkee",
        "the loaded copy must complete the same upgrade"
    );
    assert_eq!(loaded.state_hash(), sim.state_hash());
}

// ---------------------------------------------------------------------------
// Command stream: patrol + research
// ---------------------------------------------------------------------------

#[test]
fn sim_orders_patrol_command_reaches_the_order_slot() {
    // No enemy on this map: a patrol test must measure the patrol order, not
    // automatic acquisition.
    let mut sim = Sim::new(0x501, 32, 32);
    sim.passive_income = false;
    hall(&mut sim, 0, "htow", Vec2::new(300.0, 300.0));
    let fighter = sim
        .spawn_def(0, "hfoo", Vec2::new(400.0, 300.0))
        .expect("footman");
    sim.step(&[]);
    let a = Vec2::new(200.0, 200.0);
    let b = Vec2::new(600.0, 200.0);
    sim.step(&[Command::Patrol {
        units: vec![fighter],
        a,
        b,
    }]);
    match sim.unit(fighter).expect("footman").order {
        Order::Patrol {
            a: pa,
            b: pb,
            forward,
        } => {
            assert!(pa.distance(a) < 1.0 && pb.distance(b) < 1.0);
            assert!(forward, "a fresh patrol heads for its patrol point");
        }
        other => panic!("patrol command must set Order::Patrol, got {other:?}"),
    }
    // The unit must actually walk to the patrol point (`b` is the goal of a
    // fresh patrol; `a` is the endpoint it returns to).
    let start = sim.unit(fighter).unwrap().pos;
    for _ in 0..30 {
        sim.step(&[]);
    }
    let now = sim.unit(fighter).unwrap().pos;
    assert!(
        now.distance(b) < start.distance(b),
        "patrol must walk to its patrol point: {start:?} -> {now:?}"
    );
}

#[test]
fn sim_orders_research_command_starts_the_upgrade() {
    let mut sim = sandbox();
    let hall = sim.units_of(0).next().expect("hall").id;
    // `[htow] Researches=Rhpm` (Masonry) from HumanUnitFunc.txt.
    let research_id = sim.unit(hall).expect("hall").def().researches[0];
    sim.step(&[Command::Research {
        building: hall,
        upgrade: research_id,
    }]);
    assert!(
        sim.unit(hall).unwrap().research.iter().any(|r| r.id == research_id),
        "{research_id} must be in the building's research list"
    );
    let ticks = sim.unit(hall).unwrap().research[0].remaining;
    assert!(ticks > 0);
    let mut completed = false;
    for _ in 0..ticks + 2 {
        sim.step(&[]);
        // `Sim::step` clears the event buffer at the start of every tick, so the
        // completion event only exists on the tick it fires.
        completed |= sim
            .events
            .iter()
            .any(|event| matches!(event, Event::UpgradeResearched { owner: 0, id } if *id == research_id));
    }
    assert_eq!(sim.upgrade_level(0, research_id), 1);
    assert!(completed, "UpgradeResearched must fire");
}

// ---------------------------------------------------------------------------
// Victory rule
// ---------------------------------------------------------------------------

#[test]
fn sim_victory_elimination_needs_zero_buildings_not_the_hall() {
    let mut sim = Sim::new(0x501, 32, 32);
    sim.passive_income = false;
    let player_hall = hall(&mut sim, 0, "htow", Vec2::new(300.0, 300.0));
    let barracks = sim
        .spawn_def(0, "hbar", Vec2::new(500.0, 300.0))
        .expect("barracks");
    hall(&mut sim, 1, "ogre", Vec2::new(900.0, 900.0));
    sim.step(&[]);

    // Losing the town hall is not defeat while a barracks stands: the seat can
    // rebuild, which is the WC3 melee rule.
    sim.kill_unit(player_hall);
    sim.step(&[]);
    assert!(
        !sim.events
            .iter()
            .any(|event| matches!(event, Event::Victory(_))),
        "a seat with a barracks is not eliminated: {:?}",
        sim.events
    );

    // With the last building gone the other side wins.
    sim.kill_unit(barracks);
    sim.step(&[]);
    assert!(
        sim.events
            .iter()
            .any(|event| matches!(event, Event::Victory(1))),
        "elimination needs zero buildings left: {:?}",
        sim.events
    );
}

#[test]
fn sim_building_upgrade_denies_when_the_bank_is_short() {
    let mut sim = sandbox();
    let hall = sim.units_of(0).next().expect("hall").id;
    sim.resources[0] = 0;
    assert_eq!(sim.upgrade_denial(hall, "hkee"), Some("not enough gold"));
    let err = sim.upgrade_building(hall, "hkee").expect_err("denied");
    assert_eq!(err, "not enough gold");
    assert!(sim.unit(hall).unwrap().upgrade.is_none(), "nothing started");
    assert!(sim
        .events
        .iter()
        .any(|event| matches!(event, Event::Error("not enough gold"))));
}

#[test]
fn sim_building_upgrade_ignores_stop_and_move_orders_on_the_job() {
    // A stop order must not cancel an in-flight upgrade: WC3 upgrades are a
    // building job, not an order a unit can drop.
    let mut sim = sandbox();
    let hall = sim.units_of(0).next().expect("hall").id;
    sim.upgrade_building(hall, "hkee").expect("start upgrade");
    sim.step(&[Command::Stop { units: vec![hall] }]);
    assert!(
        sim.unit(hall).unwrap().upgrade.is_some(),
        "Stop must not cancel a building upgrade"
    );
    // Nor does an explicit Move queue replace it.
    sim.issue(&[hall], OrderSpec::Move(Vec2::new(600.0, 600.0)));
    sim.step(&[]);
    assert!(sim.unit(hall).unwrap().upgrade.is_some());
}
