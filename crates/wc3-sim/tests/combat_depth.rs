//! WC3 combat depth — attack/armour table, damage rolls, miss policy, bounty,
//! splash falloff and a Justice map verification.
//!
//! Mechanics covered (ledger 401):
//! * `combat.attack-armor-table` — the shipped `MiscGame.txt` `DamageBonus*`
//!   matrix and the `0.06*armor/(1+0.06*|armor|)` reduction, wired through
//!   `UnitDef` attack/armour types.
//! * `combat.damage-roll` — per-hit damage rolls in `[dmgplus+dice,
//!   dmgplus+dice*sides]`, seeded determinism, targetability and the
//!   ground↔air miss policy.
//! * `combat.bounty-xp` — `UnitBalance.slk` kill bounty paid once plus hero XP
//!   for units dying near a hero.
//! * `combat.splash-falloff` — `Farea1`/`Harea1`/`Qarea1` +
//!   `Hfact1`/`Qfact1` falloff, splash target masks (friendly fire vs
//!   `enemy`-only) and the projectile impact path.
//! * `combat.justice-1v1-table` — fixed 1v1 fights and a 10-unit skirmish on
//!   the shipped `(2)Justice.w3x` grid, asserting the measured per-hit numbers.
//!
//! Shipped numbers are computed from the `UnitWeapons.slk` dice and the
//! `UnitBalance.slk` armour values; the test prints the observed per-hit set so
//! a regression is visible as data, not just as a failed assert.

use std::path::Path;

use wc3_formats::map::map_mpq_base;
use wc3_formats::wpm::PathingMap;
use wc3_mpq::MpqArchive;
use wc3_sim::data::{self, DAMAGE_MULT_PERMILLE};
use wc3_sim::{ArmorType, AttackType, Command, Event, Sim, UnitId, Vec2, def_of};

const JUSTICE_MAP: &str = "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x";

/// Expected per-hit damage range for `(attacker, defender)` from the shipped
/// dice and armour: `base in [dmgplus+dice, dmgplus+dice*sides]`, then the
/// attack/armour multiplier, then the armour rating.
fn expected_hit_range(attacker: &str, defender: &str) -> (i32, i32) {
    let attack = def_of(attacker).expect("attacker def").stats;
    let defend = def_of(defender).expect("defender def").stats;
    let dice = attack.damage_dice as i32;
    let sides = attack.damage_sides as i32;
    let min = data::typed_damage(
        attack.attack_type,
        defend.armor_type,
        defend.armor,
        attack.damage + dice,
    );
    let max = data::typed_damage(
        attack.attack_type,
        defend.armor_type,
        defend.armor,
        attack.damage + dice * sides,
    );
    (min.min(max), max.max(min))
}

/// Steps the sim and returns every positive `Damage` event the `from` unit
/// dealt to `to`.
fn hits_between(sim: &mut Sim, from: UnitId, to: UnitId, ticks: u32) -> Vec<i32> {
    let mut hits = Vec::new();
    for _ in 0..ticks {
        sim.step(&[]);
        for event in &sim.events {
            if let Event::Damage {
                from: source,
                to: victim,
                amount,
            } = event
                && *source == from
                && *victim == to
                && *amount > 0
            {
                hits.push(*amount);
            }
        }
    }
    hits
}

/// World position of a pathing cell centre (buildings snap to these).
fn cell(sim: &Sim, x: u32, y: u32) -> Vec2 {
    sim.grid.cell_to_world(x, y)
}

fn flat_sim(seed: u64) -> Sim {
    let mut sim = Sim::new(seed, 48, 48);
    sim.passive_income = false;
    sim
}

/// Flat grid re-pathed at the 32-unit WC3 pathing cell so splash radii can be
/// laid out on exact cell multiples (buildings snap to cell centres).
fn flat_fine(seed: u64) -> Sim {
    let mut sim = Sim::new(seed, 64, 64);
    sim.passive_income = false;
    sim.set_pathing(64, 64, 32.0, vec![true; 64 * 64]);
    sim
}

/// Stages a 1v1 on a flat grid: `attacker` at (600, 600) with `gap` world units
/// to `defender`, both ordered to fight.
fn stage_duel(
    seed: u64,
    attacker: &str,
    defender: &str,
    gap: f32,
) -> (Sim, UnitId, UnitId) {
    let mut sim = flat_sim(seed);
    let a = sim
        .spawn_def_exact(0, attacker, Vec2::new(600.0, 600.0))
        .expect("attacker");
    let d = sim
        .spawn_def_exact(1, defender, Vec2::new(600.0 + gap, 600.0))
        .expect("defender");
    sim.step(&[Command::Attack {
        units: vec![a],
        target: d,
    }]);
    (sim, a, d)
}

// ----------------------------------------------------------------------
// combat.attack-armor-table
// ----------------------------------------------------------------------

#[test]
fn combat_attack_armor_table_matches_shipped_matrix() {
    // `Units/MiscGame.txt:106-112` verbatim, columns SMALL, MEDIUM, LARGE,
    // FORT, NORMAL, HERO, DIVINE, NONE (permille).
    let shipped: [[i32; 8]; 7] = [
        [1000, 1500, 1000, 700, 1000, 1000, 50, 1000],   // DamageBonusNormal
        [2000, 750, 1000, 350, 1000, 500, 50, 1500],     // DamageBonusPierce
        [1000, 500, 1000, 1500, 1000, 500, 50, 1500],    // DamageBonusSiege
        [1250, 750, 2000, 350, 1000, 500, 50, 1000],     // DamageBonusMagic
        [1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000], // DamageBonusChaos
        [1000, 1000, 1000, 1000, 1000, 750, 50, 1000],   // DamageBonusSpells
        [1000, 1000, 1000, 500, 1000, 1000, 50, 1000],   // DamageBonusHero
    ];
    assert_eq!(DAMAGE_MULT_PERMILLE.len(), shipped.len());
    for (row, expected) in shipped.iter().enumerate() {
        assert_eq!(&DAMAGE_MULT_PERMILLE[row], expected, "attack row {row}");
    }

    // Every attack row against every armour class must equal
    // `base * mult / 1000` with the armour formula applied on top.
    for attack in [
        AttackType::Normal,
        AttackType::Pierce,
        AttackType::Siege,
        AttackType::Magic,
        AttackType::Chaos,
        AttackType::Spells,
        AttackType::Hero,
    ] {
        for armor_type in [
            ArmorType::Light,
            ArmorType::Medium,
            ArmorType::Heavy,
            ArmorType::Fortified,
            ArmorType::Unarmored,
            ArmorType::Hero,
        ] {
            let mult = DAMAGE_MULT_PERMILLE[attack.index()][armor_type.index()];
            let typed = 100 * mult / 1000;
            // The armor block is gated on the damage object's physical flag, not
            // on the attack class: spells keep the table row and skip armor
            // (`analysis/agent-re/sim/PROBES.md` §2, ledger 404/412).
            let expected = if data::skips_armor(attack) {
                typed
            } else {
                data::armor_reduction(4, typed)
            };
            assert_eq!(
                data::typed_damage(attack, armor_type, 4, 100),
                expected,
                "{attack:?} vs {armor_type:?}"
            );
        }
    }

    // WC3 armour formula (`DefenseArmor=0.06`): 100 / (100 + 6*armor), and
    // negative armour amplifies via 2 - 0.94^|armor|.
    assert_eq!(data::armor_reduction(0, 100), 100);
    assert_eq!(data::armor_reduction(5, 100), 76);
    assert_eq!(data::armor_reduction(10, 100), 62);
    assert_eq!(data::armor_reduction(-1, 100), 106);
    assert_eq!(data::armor_reduction(-10, 100), 146);
    assert!(data::typed_damage(AttackType::Normal, ArmorType::Medium, 0, 100) > 0);

    // Unit defs carry the shipped attack/armour type ids.
    let cases = [
        ("hfoo", AttackType::Normal, ArmorType::Medium),
        ("ogru", AttackType::Normal, ArmorType::Medium),
        ("hrif", AttackType::Pierce, ArmorType::Light),
        ("otau", AttackType::Normal, ArmorType::Heavy),
        ("hmtm", AttackType::Siege, ArmorType::Medium),
        ("hgtw", AttackType::Pierce, ArmorType::Fortified),
    ];
    for (id, attack, armor) in cases {
        let stats = def_of(id).expect("def").stats;
        assert_eq!(stats.attack_type, attack, "{id} attack type");
        assert_eq!(stats.armor_type, armor, "{id} armour type");
    }

    // The three required table cases, analytically.
    let footman_grunt = expected_hit_range("hfoo", "ogru");
    let rifleman_tauren = expected_hit_range("hrif", "otau");
    let mortar_tower = expected_hit_range("hmtm", "hgtw");
    println!(
        "table ranges: hfoo->ogru {footman_grunt:?}, hrif->otau {rifleman_tauren:?}, \
         hmtm->hgtw {mortar_tower:?}"
    );
    assert_eq!(footman_grunt, (16, 17));
    assert_eq!(rifleman_tauren, (15, 20));
    assert_eq!(mortar_tower, (60, 73));

    // Negative armour on the defender must raise the number above the
    // unmodified table value.
    let mut sim = flat_sim(0xA11);
    let a = sim
        .spawn_def_exact(1, "hfoo", Vec2::new(600.0, 600.0))
        .unwrap();
    let d = sim
        .spawn_def_exact(0, "hfoo", Vec2::new(700.0, 600.0))
        .unwrap();
    sim.unit_mut(d).unwrap().armor = -5;
    // Normal vs Medium = 150 typed; -5 armour amplifies by 1.266.
    assert_eq!(sim.apply_damage(a, d, 100, AttackType::Normal), 189);
}

// ----------------------------------------------------------------------
// combat.damage-roll
// ----------------------------------------------------------------------

#[test]
fn combat_damage_roll_covers_the_dice_range_and_is_seeded() {
    let (mut sim, footman, grunt) = stage_duel(0x2B01, "hfoo", "ogru", 60.0);
    // Keep the duel alive long enough to sample the roll.
    sim.unit_mut(footman).unwrap().hp = 100_000;
    sim.unit_mut(footman).unwrap().max_hp = 100_000;
    sim.unit_mut(grunt).unwrap().hp = 100_000;
    sim.unit_mut(grunt).unwrap().max_hp = 100_000;
    let hits = hits_between(&mut sim, footman, grunt, 1_200);
    assert!(hits.len() >= 20, "sampled {} footman hits", hits.len());
    let min = *hits.iter().min().unwrap();
    let max = *hits.iter().max().unwrap();
    println!("footman->grunt per-hit: {min}..{max} over {} hits", hits.len());
    assert!(
        hits.iter().all(|hit| (16..=17).contains(hit)),
        "every hit inside the table range 16..=17: {hits:?}"
    );
    assert_eq!((min, max), (16, 17), "the roll covers both ends of 1d2");

    // Same seed, same staging, same damage stream.
    let (mut again, footman2, grunt2) = stage_duel(0x2B01, "hfoo", "ogru", 60.0);
    again.unit_mut(footman2).unwrap().hp = 100_000;
    again.unit_mut(footman2).unwrap().max_hp = 100_000;
    again.unit_mut(grunt2).unwrap().hp = 100_000;
    again.unit_mut(grunt2).unwrap().max_hp = 100_000;
    let replays = hits_between(&mut again, footman2, grunt2, 1_200);
    assert_eq!(hits, replays, "seeded RNG reproduces the roll");

    // A different seed must move the stream.
    let (mut other, footman3, grunt3) = stage_duel(0x2B02, "hfoo", "ogru", 60.0);
    other.unit_mut(footman3).unwrap().hp = 100_000;
    other.unit_mut(footman3).unwrap().max_hp = 100_000;
    other.unit_mut(grunt3).unwrap().hp = 100_000;
    other.unit_mut(grunt3).unwrap().max_hp = 100_000;
    let different = hits_between(&mut other, footman3, grunt3, 1_200);
    assert_ne!(hits, different, "a different seed changes the rolls");
}

#[test]
fn combat_damage_roll_targetability_and_miss_policy() {
    // A ground-only weapon never damages an air unit (footman vs gryphon).
    let (mut ground, footman, gryphon) = stage_duel(0x2B10, "hfoo", "hgry", 60.0);
    ground.unit_mut(gryphon).unwrap().hp = 100_000;
    ground.unit_mut(gryphon).unwrap().max_hp = 100_000;
    let blocked = hits_between(&mut ground, footman, gryphon, 400);
    assert!(blocked.is_empty(), "ground-only weapon hit an air unit: {blocked:?}");
    assert_eq!(
        ground.unit(gryphon).unwrap().hp, 100_000,
        "air unit untouched by a ground-only weapon"
    );

    // Ground -> air: 5% miss.  Air -> ground: 10% miss.  Both sides keep
    // firing on hold with a 1-tick cooldown, so ~400 shots sample the policy.
    let (mut duel, rifleman, flyer) = stage_duel(0x2B11, "hrif", "hgry", 300.0);
    for unit in [rifleman, flyer] {
        let unit = duel.unit_mut(unit).unwrap();
        unit.hp = 100_000;
        unit.max_hp = 100_000;
        unit.attack_ticks = 1;
        unit.cooldown = 0;
    }
    duel.step(&[Command::Hold {
        units: vec![rifleman, flyer],
    }]);
    duel.step(&[]);
    let mut shots = 0usize;
    let mut landed = 0usize;
    let mut flyer_shots = 0usize;
    let mut flyer_landed = 0usize;
    for _ in 0..500 {
        duel.step(&[]);
        for event in &duel.events {
            match event {
                Event::ProjectileHit { from, amount, .. } if *from == rifleman => {
                    shots += 1;
                    if *amount > 0 {
                        landed += 1;
                    }
                }
                Event::ProjectileHit { from, amount, .. } if *from == flyer => {
                    flyer_shots += 1;
                    if *amount > 0 {
                        flyer_landed += 1;
                    }
                }
                _ => {}
            }
        }
    }
    assert!(shots >= 200, "rifleman shots sampled: {shots}");
    assert!(flyer_shots >= 200, "gryphon shots sampled: {flyer_shots}");
    let ground_to_air_miss = (shots - landed) as f64 / shots as f64;
    let air_to_ground_miss = (flyer_shots - flyer_landed) as f64 / flyer_shots as f64;
    println!(
        "miss rates: ground->air {:.3} ({} / {}), air->ground {:.3} ({} / {})",
        ground_to_air_miss,
        shots - landed,
        shots,
        air_to_ground_miss,
        flyer_shots - flyer_landed,
        flyer_shots
    );
    assert!(
        (0.01..0.10).contains(&ground_to_air_miss),
        "ground->air miss policy is 5%: {ground_to_air_miss}"
    );
    assert!(
        (0.04..0.18).contains(&air_to_ground_miss),
        "air->ground miss policy is 10%: {air_to_ground_miss}"
    );
    assert!(
        air_to_ground_miss > ground_to_air_miss,
        "air->ground misses twice as often as ground->air"
    );
}

// ----------------------------------------------------------------------
// combat.bounty-xp
// ----------------------------------------------------------------------

#[test]
fn combat_bounty_xp_pays_once_and_levels_nearby_hero() {
    // Shipped bounty row for the forest troll (`UnitBalance.slk`): 5 + 1d3.
    assert_eq!(data::unit_bounty("nftr"), Some((5, 1, 3)));

    let mut sim = flat_sim(0xB017);
    sim.resources = [0, 0];
    let hero = sim
        .spawn_def_exact(0, "Hpal", Vec2::new(600.0, 600.0))
        .expect("hero");
    assert!(sim.unit(hero).unwrap().def().hero);
    let killer = sim
        .spawn_def_exact(0, "hfoo", Vec2::new(700.0, 600.0))
        .expect("footman");
    let creep = sim
        .spawn_def_exact(2, "nftr", Vec2::new(720.0, 600.0))
        .expect("creep");
    let creep_hp = sim.unit(creep).unwrap().hp;
    let xp_before = sim.unit(hero).unwrap().xp;
    let gold_before = sim.resources[0];

    // Lethal hit: the kill must pay bounty exactly once and grant kill XP.
    let dealt = sim.apply_damage(killer, creep, creep_hp + 50, AttackType::Chaos);
    assert!(dealt >= creep_hp);
    assert!(!sim.unit(creep).unwrap().alive);
    let bounties: Vec<(u8, i32)> = sim
        .events
        .iter()
        .filter_map(|event| match event {
            Event::Bounty { owner, unit, gold } if *unit == creep => Some((*owner, *gold)),
            _ => None,
        })
        .collect();
    assert_eq!(bounties.len(), 1, "one bounty per death: {bounties:?}");
    assert_eq!(bounties[0].0, 0, "paid to the killer's player");
    assert!(
        (6..=8).contains(&bounties[0].1),
        "5 + 1d3 bounty: {}",
        bounties[0].1
    );
    let gold_after = sim.resources[0];
    assert_eq!(gold_after - gold_before, bounties[0].1, "gold credited");
    let xp_after = sim.unit(hero).unwrap().xp;
    assert_eq!(
        xp_after - xp_before,
        sim.gameplay.grant_hero_xp[1],
        "level-2 creep grants GrantHeroXP[1] to the hero in range"
    );

    // Hitting the corpse again (and a second lethal strike) pays nothing.
    let gold_after_kill = sim.resources[0];
    sim.events.clear();
    assert_eq!(sim.apply_damage(killer, creep, 500, AttackType::Chaos), 0);
    assert_eq!(sim.resources[0], gold_after_kill, "no double bounty");
    let repeat_bounties = sim
        .events
        .iter()
        .filter(|event| matches!(event, Event::Bounty { unit, .. } if *unit == creep))
        .count();
    assert_eq!(repeat_bounties, 0, "no bounty event for a dead unit");

    // Neutral-on-neutral damage never pays a player (creeps do not infight
    // for gold).
    let mut creeps = flat_sim(0xB018);
    creeps.resources = [0, 0];
    let wolf = creeps
        .spawn_def_exact(2, "nftr", Vec2::new(600.0, 600.0))
        .unwrap();
    let troll = creeps
        .spawn_def_exact(2, "nftr", Vec2::new(700.0, 600.0))
        .unwrap();
    let hp = creeps.unit(troll).unwrap().hp;
    creeps.apply_damage(wolf, troll, hp + 10, AttackType::Chaos);
    assert!(!creeps.unit(troll).unwrap().alive);
    assert!(
        !creeps
            .events
            .iter()
            .any(|event| matches!(event, Event::Bounty { .. })),
        "neutral kills pay no player bounty"
    );
    assert_eq!(creeps.resources, [0, 0]);
}

// ----------------------------------------------------------------------
// combat.splash-falloff
// ----------------------------------------------------------------------

#[test]
fn combat_splash_falloff_radii_factors_and_impact() {
    // Mortar team (`Farea1=25`, `Harea1=150` at `Hfact1=0.4`, `Qarea1=250` at
    // `Qfact1=0.1`) firing at a guard tower; three more towers sit at 120, 220
    // and 420 world units from the impact point.
    let profile = wc3_sim::combat_gen::profile("hmtm").expect("mortar profile").splash;
    let soup = profile.expect("mortar splash profile");
    assert_eq!(soup.full_radius, 25.0);
    assert_eq!(soup.half_radius, 150.0);
    assert_eq!(soup.quarter_radius, 250.0);
    assert_eq!(soup.half_factor, 400);
    assert_eq!(soup.quarter_factor, 100);

    let mut sim = flat_fine(0x5A11);
    // Buildings snap to cell centres, so stage on cell centres of the 32-unit
    // pathing grid: 4 cells (128 world units) is inside `Harea1=150`, 7 cells
    // (224) inside `Qarea1=250` and 17 cells (544) outside it.
    let primary = sim
        .spawn_def_exact(1, "hgtw", cell(&sim, 24, 20))
        .expect("primary tower");
    let near = sim
        .spawn_def_exact(1, "hgtw", cell(&sim, 24, 24))
        .expect("half-radius tower");
    let far = sim
        .spawn_def_exact(1, "hgtw", cell(&sim, 24, 27))
        .expect("quarter-radius tower");
    let outside = sim
        .spawn_def_exact(1, "hgtw", cell(&sim, 24, 41))
        .expect("outside tower");
    // A building re-anchors its centre on the first tick, so settle the
    // towers before measuring the impact point.
    sim.step(&[]);
    sim.events.clear();
    let impact = sim.unit(primary).unwrap().pos;
    let mortar = sim
        .spawn_def_exact(0, "hmtm", cell(&sim, 24, 44))
        .expect("mortar");
    // The allied footman sits exactly 128 units from the impact point.
    let ally = sim
        .spawn_def_exact(0, "hfoo", impact + Vec2::new(128.0, 0.0))
        .expect("allied footman");
    let outside_hp = sim.unit(outside).unwrap().hp;
    println!(
        "splash stage: impact={:?} ally_dist={:.1} near_dist={:.1} far_dist={:.1} \
         outside_dist={:.1}",
        impact,
        sim.unit(ally).unwrap().pos.distance(impact),
        sim.unit(near).unwrap().pos.distance(impact),
        sim.unit(far).unwrap().pos.distance(impact),
        sim.unit(outside).unwrap().pos.distance(impact),
    );
    // Hold keeps the ally where it was placed: an idle footman would charge
    // the towers and walk out of the splash ring.
    sim.step(&[
        Command::Attack {
            units: vec![mortar],
            target: primary,
        },
        Command::Hold {
            units: vec![ally],
        },
    ]);

    // Collect every damage amount the mortar dealt this shot (projectile
    // impact + splash in the same or the following few ticks).
    let mut dealt: Vec<(UnitId, i32)> = Vec::new();
    for _ in 0..120 {
        sim.step(&[]);
        for event in &sim.events {
            if let Event::Damage {
                from,
                to,
                amount,
            } = event
                && *from == mortar
            {
                dealt.push((*to, *amount));
            }
        }
        if dealt.iter().any(|(id, _)| *id == primary) {
            break;
        }
    }
    println!("mortar splash damage: {dealt:?}");
    for (name, id) in [("primary", primary), ("near", near), ("far", far), ("outside", outside), ("ally", ally)] {
        let unit = sim.unit(id).unwrap();
        println!("  {name} id={:?} pos={:?} dist={:.1} hp={}", unit.id, unit.pos, unit.pos.distance(sim.unit(primary).unwrap().pos), unit.hp);
    }
    let amount = |id: UnitId| {
        dealt
            .iter()
            .filter(|(target, _)| *target == id)
            .map(|(_, amount)| *amount)
            .next()
    };
    let primary_damage = amount(primary).expect("primary hit");
    let near_damage = amount(near).expect("half-radius splash");
    let far_damage = amount(far).expect("quarter-radius splash");
    assert!(
        (60..=73).contains(&primary_damage),
        "siege vs fortified armour 5 is 60..=73, got {primary_damage}"
    );
    assert!(
        (23..=28).contains(&near_damage),
        "Hfact1 0.4 splash is 23..=28, got {near_damage}"
    );
    assert!(
        (5..=6).contains(&far_damage),
        "Qfact1 0.1 splash is 5..=6, got {far_damage}"
    );
    assert!(
        primary_damage > near_damage && near_damage > far_damage,
        "falloff must be monotonic"
    );
    assert!(
        amount(outside).is_none(),
        "outside Qarea1 (250) takes no splash"
    );
    assert_eq!(
        sim.unit(outside).unwrap().hp,
        outside_hp,
        "outside Qarea1 keeps full hp"
    );
    let ally_damage = amount(ally).expect("friendly fire splash");
    assert!(
        (8..=10).contains(&ally_damage),
        "siege vs medium armour 2 at Hfact1 is 8..=10, got {ally_damage}"
    );

    // The primary target is hit once per shot, never twice.
    assert_eq!(
        dealt.iter().filter(|(id, _)| *id == primary).count(),
        1,
        "splash never re-hits the primary target"
    );

    // `enemy`-only splash (frost wyrm `ufro`) spares allies.
    //
    // The ally is a sheep, not a footman: a building keeps the footprint it
    // spawned on (it is no longer teleported off its own blocked cells), so an
    // armed ally parked inside its attack range would kill the tower before the
    // wyrm walks into range and the splash check would measure nothing. A sheep
    // has no weapon and only serves as the friendly splash target.
    let mut sim = flat_sim(0x5A12);
    let wyrm = sim
        .spawn_def_exact(0, "ufro", cell(&sim, 20, 29))
        .expect("frost wyrm");
    let target = sim
        .spawn_def_exact(1, "hgtw", cell(&sim, 20, 20))
        .expect("tower");
    let friendly = sim
        .spawn_def_exact(0, "nshe", cell(&sim, 21, 20))
        .expect("ally");
    let target_hp = sim.unit(target).unwrap().hp;
    sim.step(&[
        Command::Attack {
            units: vec![wyrm],
            target,
        },
        Command::Hold {
            units: vec![friendly],
        },
    ]);
    // The tower shoots the ally itself (different owner), so only the wyrm's
    // own damage events decide the splash question.
    let mut wyrm_hits: Vec<(UnitId, i32)> = Vec::new();
    for _ in 0..180 {
        sim.step(&[]);
        for event in &sim.events {
            if let Event::Damage { from, to, amount } = event
                && *from == wyrm
            {
                wyrm_hits.push((*to, *amount));
            }
        }
        if sim.unit(target).unwrap().hp < target_hp {
            break;
        }
    }
    println!("enemy-only splash hits: {wyrm_hits:?}");
    assert!(sim.unit(target).unwrap().hp < target_hp, "enemy took splash");
    assert!(
        wyrm_hits.iter().any(|(id, _)| *id == target),
        "the wyrm damaged its target"
    );
    assert!(
        !wyrm_hits.iter().any(|(id, _)| *id == friendly),
        "`enemy`-flagged splashTargs spares allies: {wyrm_hits:?}"
    );
}

// ----------------------------------------------------------------------
// combat.justice-1v1-table
// ----------------------------------------------------------------------

fn load_justice_pathing() -> Option<PathingMap> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join(JUSTICE_MAP);
    if !path.is_file() {
        eprintln!("skip: (2)Justice.w3x fixture missing at {}", path.display());
        return None;
    }
    let base = map_mpq_base(&path).ok()?;
    let archive = MpqArchive::open_at(&path, base).ok()?;
    PathingMap::parse(&archive.read("war3map.wpm").ok()?).ok()
}

fn justice_sim(wpm: &PathingMap, seed: u64) -> Sim {
    let mut sim = Sim::new(seed, wpm.width, wpm.height);
    sim.passive_income = false;
    let mut walkable = Vec::with_capacity(wpm.width as usize * wpm.height as usize);
    for y in 0..wpm.height {
        for x in 0..wpm.width {
            walkable.push(wpm.walkable(x, y));
        }
    }
    sim.set_pathing(
        wpm.width,
        wpm.height,
        wc3_sim::PATHING_CELL_SIZE,
        walkable,
    );
    sim
}

/// First square block of `cells` walkable cells, returned in world units.
fn open_area(sim: &Sim, cells: u32) -> Vec2 {
    let step = 4;
    for y in (2..sim.grid.height.saturating_sub(cells + 2)).step_by(step) {
        for x in (2..sim.grid.width.saturating_sub(cells + 2)).step_by(step) {
            let mut clear = true;
            'block: for dy in 0..cells {
                for dx in 0..cells {
                    if !sim.grid.walkable(x + dx, y + dy) {
                        clear = false;
                        break 'block;
                    }
                }
            }
            if clear {
                let centre = sim
                    .grid
                    .cell_to_world(x + cells / 2, y + cells / 2);
                return centre;
            }
        }
    }
    panic!("no open {cells}x{cells} cell block on the Justice grid");
}

#[test]
fn combat_justice_1v1_table_armour_upgrade_and_skirmish() {
    let Some(wpm) = load_justice_pathing() else {
        return;
    };

    // Required 1v1 pairs: measured per-hit damage must sit inside the shipped
    // table range.
    let pairs = [
        ("hfoo", "ogru", 90.0, (16, 17)),
        ("hrif", "otau", 380.0, (15, 20)),
        ("hmtm", "hgtw", 900.0, (60, 73)),
    ];
    for (index, (attacker, defender, gap, expected)) in pairs.iter().enumerate() {
        let mut sim = justice_sim(&wpm, 0x1B00 + index as u64);
        let centre = open_area(&sim, 16);
        let a = sim
            .spawn_def_exact(0, attacker, centre)
            .expect("attacker");
        let d = sim
            .spawn_def_exact(1, defender, centre + Vec2::new(*gap, 0.0))
            .expect("defender");
        sim.unit_mut(a).unwrap().hp = 100_000;
        sim.unit_mut(a).unwrap().max_hp = 100_000;
        sim.unit_mut(d).unwrap().hp = 100_000;
        sim.unit_mut(d).unwrap().max_hp = 100_000;
        sim.step(&[Command::Attack {
            units: vec![a],
            target: d,
        }]);
        let hits = hits_between(&mut sim, a, d, 900);
        assert!(
            !hits.is_empty(),
            "{attacker} -> {defender} produced no hits on Justice"
        );
        let min = *hits.iter().min().unwrap();
        let max = *hits.iter().max().unwrap();
        println!(
            "justice 1v1 {attacker}->{defender}: {min}..{max} over {} hits",
            hits.len()
        );
        assert!(
            hits.iter().all(|hit| (expected.0..=expected.1).contains(hit)),
            "{attacker}->{defender} hits outside {expected:?}: {hits:?}"
        );
        if expected.0 != expected.1 {
            assert!(
                max > min,
                "{attacker}->{defender} rolled only {min}"
            );
        }
    }

    // Armour rating reduces the same exchange: the footman's 16..17 against a
    // 2-armour grunt becomes 14..15 at 4 armour.
    let mut sim = justice_sim(&wpm, 0x1B10);
    let centre = open_area(&sim, 16);
    let footman = sim
        .spawn_def_exact(0, "hfoo", centre)
        .expect("footman");
    let grunt = sim
        .spawn_def_exact(1, "ogru", centre + Vec2::new(60.0, 0.0))
        .expect("grunt");
    sim.unit_mut(grunt).unwrap().armor = 4;
    sim.unit_mut(footman).unwrap().hp = 100_000;
    sim.unit_mut(footman).unwrap().max_hp = 100_000;
    sim.unit_mut(grunt).unwrap().hp = 100_000;
    sim.unit_mut(grunt).unwrap().max_hp = 100_000;
    sim.step(&[Command::Attack {
        units: vec![footman],
        target: grunt,
    }]);
    let armoured = hits_between(&mut sim, footman, grunt, 1_200);
    assert!(!armoured.is_empty(), "armoured grunt took no hits");
    println!("justice armour 4 per-hit: {armoured:?}");
    assert!(
        armoured.iter().all(|hit| (14..=15).contains(hit)),
        "2 extra armour lowers 16..17 to 14..15: {armoured:?}"
    );

    // 10-unit skirmish: 5 footmen vs 5 grunts, both sides attack-move; one
    // side is wiped inside a bounded tick budget.
    let mut sim = justice_sim(&wpm, 0x1B20);
    let centre = open_area(&sim, 24);
    let left = centre - Vec2::new(220.0, 0.0);
    let right = centre + Vec2::new(220.0, 0.0);
    let mut humans = Vec::new();
    let mut orcs = Vec::new();
    for slot in 0..5 {
        let offset = Vec2::new(0.0, (slot as f32 - 2.0) * 60.0);
        humans.push(
            sim.spawn_def_exact(0, "hfoo", left + offset)
                .expect("footman"),
        );
        orcs.push(
            sim.spawn_def_exact(1, "ogru", right + offset)
                .expect("grunt"),
        );
    }
    sim.step(&[Command::MoveAttack {
        units: humans.clone(),
        to: right,
    }]);
    sim.step(&[Command::MoveAttack {
        units: orcs.clone(),
        to: left,
    }]);
    let mut ticks = 0u32;
    let mut damage_events = 0usize;
    while ticks < 4_000 {
        sim.step(&[]);
        damage_events += sim
            .events
            .iter()
            .filter(|event| matches!(event, Event::Damage { .. }))
            .count();
        ticks += 1;
        let humans_alive = humans
            .iter()
            .filter(|id| sim.unit(**id).map(|unit| unit.alive).unwrap_or(false))
            .count();
        let orcs_alive = orcs
            .iter()
            .filter(|id| sim.unit(**id).map(|unit| unit.alive).unwrap_or(false))
            .count();
        if humans_alive == 0 || orcs_alive == 0 {
            break;
        }
    }
    let humans_alive = humans
        .iter()
        .filter(|id| sim.unit(**id).map(|unit| unit.alive).unwrap_or(false))
        .count();
    let orcs_alive = orcs
        .iter()
        .filter(|id| sim.unit(**id).map(|unit| unit.alive).unwrap_or(false))
        .count();
    println!(
        "justice skirmish: {ticks} ticks, {damage_events} damage events, \
         humans {humans_alive}/5, orcs {orcs_alive}/5"
    );
    assert!(
        ticks < 4_000,
        "skirmish must resolve inside 4000 ticks (stopped at {ticks})"
    );
    assert!(
        humans_alive == 0 || orcs_alive == 0,
        "one side must be wiped: humans {humans_alive}, orcs {orcs_alive}"
    );
    assert!(damage_events > 10, "the skirmish actually fought");
}
