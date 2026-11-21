//! Ability framework: target kinds, mana, cooldowns, waves and buffs.

mod common;

use common::*;
use wc3_sim::{Sim, Spell, SpellTarget, Vec2};

fn caster(sim: &mut Sim, def: &str, pos: Vec2) -> wc3_sim::UnitId {
    sim.spawn_def(0, def, pos).expect("caster def")
}

#[test]
fn holy_light_heals_an_ally_and_spends_mana_with_cooldown() {
    let mut sim = Sim::new(71, 32, 32);
    let paladin = caster(&mut sim, "Hpal", Vec2::new(500.0, 500.0));
    let ally = footman(&mut sim, 0, Vec2::new(700.0, 500.0));
    sim.unit_mut(ally).unwrap().hp = 100;
    let mana_before = sim.unit(paladin).unwrap().mana;
    sim.cast(paladin, Spell::HolyLight, SpellTarget::Unit(ally))
        .expect("holy light");
    assert_eq!(sim.unit(ally).unwrap().hp, 300, "heals 200");
    assert_eq!(sim.unit(paladin).unwrap().mana, mana_before - 75);
    assert_eq!(
        sim.unit(paladin).unwrap().spell_cd[Spell::HolyLight.index()],
        150
    );
    assert_eq!(
        sim.cast(paladin, Spell::HolyLight, SpellTarget::Unit(ally)),
        Err("spell on cooldown")
    );
}

#[test]
fn holy_light_rejects_enemies_and_wrong_targets() {
    let mut sim = Sim::new(72, 32, 32);
    let paladin = caster(&mut sim, "Hpal", Vec2::new(500.0, 500.0));
    let enemy = footman(&mut sim, 1, Vec2::new(700.0, 500.0));
    assert_eq!(
        sim.cast(paladin, Spell::HolyLight, SpellTarget::Unit(enemy)),
        Err("must target an ally")
    );
    assert_eq!(
        sim.cast(
            paladin,
            Spell::HolyLight,
            SpellTarget::Point(Vec2::new(600.0, 500.0))
        ),
        Err("wrong target kind")
    );
    let outsider = sim.spawn_def(0, "hfoo", Vec2::new(1600.0, 500.0)).unwrap();
    assert_eq!(
        sim.cast(paladin, Spell::HolyLight, SpellTarget::Unit(outsider)),
        Err("out of range")
    );
}

#[test]
fn mana_regenerates_over_ticks() {
    let mut sim = Sim::new(73, 32, 32);
    let priest = caster(&mut sim, "hmpr", Vec2::new(500.0, 500.0));
    sim.unit_mut(priest).unwrap().mana = 0;
    step_n(&mut sim, 1500);
    assert!(sim.unit(priest).unwrap().mana > 0, "mana regenerates");
}

#[test]
fn blizzard_damages_every_enemy_in_the_area_over_waves() {
    let mut sim = Sim::new(74, 32, 32);
    let archmage = caster(&mut sim, "Hamg", Vec2::new(500.0, 500.0));
    let a = footman(&mut sim, 1, Vec2::new(900.0, 500.0));
    let b = footman(&mut sim, 1, Vec2::new(980.0, 560.0));
    let outside = footman(&mut sim, 1, Vec2::new(1500.0, 1500.0));
    sim.hold(&[a, b]);
    sim.cast(
        archmage,
        Spell::Blizzard,
        SpellTarget::Point(Vec2::new(940.0, 530.0)),
    )
    .expect("blizzard");
    step_n(&mut sim, 4);
    let early = sim.unit(a).unwrap().hp;
    step_n(&mut sim, 120);
    let a_hp = sim.unit(a).unwrap().hp;
    let b_hp = sim.unit(b).unwrap().hp;
    assert!(a_hp < sim.unit(a).unwrap().max_hp);
    assert!(b_hp < sim.unit(b).unwrap().max_hp);
    assert_eq!(
        sim.unit(outside).unwrap().hp,
        sim.unit(outside).unwrap().max_hp
    );
    assert!(a_hp <= early, "wave damage accumulates");
    // Six waves of typed magic damage against a medium-armour footman.
    let per_wave = wc3_sim::data::typed_damage(
        wc3_sim::AttackType::Magic,
        wc3_sim::ArmorType::Medium,
        2,
        30,
    );
    // Six Blizzard waves must land; the caster may also auto-attack after
    // channeling, so the total is at least the spell damage.
    assert!(
        sim.unit(a).unwrap().max_hp - a_hp >= per_wave * 6,
        "Blizzard waves: {} < {}",
        sim.unit(a).unwrap().max_hp - a_hp,
        per_wave * 6
    );
}

#[test]
fn entangling_roots_immobilises_for_a_while() {
    let mut sim = Sim::new(75, 32, 32);
    let keeper = caster(&mut sim, "Ekee", Vec2::new(500.0, 500.0));
    let victim = footman(&mut sim, 1, Vec2::new(900.0, 500.0));
    sim.cast(keeper, Spell::EntanglingRoots, SpellTarget::Unit(victim))
        .expect("roots");
    assert!(sim.unit(victim).unwrap().rooted_until > sim.tick);
    assert!(sim.unit(victim).unwrap().hp < sim.unit(victim).unwrap().max_hp);
    sim.step(&[wc3_sim::Command::Move {
        units: vec![victim],
        to: Vec2::new(1400.0, 500.0),
    }]);
    let start = sim.unit(victim).unwrap().pos;
    step_n(&mut sim, 60);
    assert!(
        sim.unit(victim).unwrap().pos.distance(start) < 4.0,
        "rooted unit cannot move"
    );
    step_n(&mut sim, 180);
    assert!(
        sim.unit(victim).unwrap().rooted_until <= sim.tick,
        "root expired"
    );
    step_n(&mut sim, 200);
    assert!(
        sim.unit(victim).unwrap().pos.distance(start) > 50.0,
        "moves after root expires"
    );
}

#[test]
fn chain_lightning_bounces_with_decay() {
    let mut sim = Sim::new(76, 32, 32);
    let seer = caster(&mut sim, "Ofar", Vec2::new(500.0, 500.0));
    let a = footman(&mut sim, 1, Vec2::new(900.0, 500.0));
    let b = footman(&mut sim, 1, Vec2::new(1100.0, 500.0));
    let c = footman(&mut sim, 1, Vec2::new(1300.0, 500.0));
    let events = {
        sim.cast(seer, Spell::ChainLightning, SpellTarget::Unit(a))
            .expect("chain lightning");
        sim.events.clone()
    };
    let hits: Vec<i32> = events
        .iter()
        .filter_map(|event| match event {
            wc3_sim::Event::SpellHit { amount, .. } => Some(*amount),
            _ => None,
        })
        .collect();
    assert_eq!(hits.len(), 3, "bounced to three targets");
    assert!(
        hits[0] > hits[1] && hits[1] > hits[2],
        "damage decays: {hits:?}"
    );
    let damaged = [a, b, c]
        .iter()
        .filter(|id| sim.unit(**id).map(|u| u.hp < u.max_hp).unwrap_or(false))
        .count();
    assert_eq!(damaged, 3);
}

#[test]
fn divine_shield_blocks_damage_for_its_duration() {
    let mut sim = Sim::new(77, 32, 32);
    let paladin = caster(&mut sim, "Hpal", Vec2::new(500.0, 500.0));
    let enemy = footman(&mut sim, 1, Vec2::new(650.0, 500.0));
    sim.cast(paladin, Spell::DivineShield, SpellTarget::None)
        .expect("shield");
    assert_eq!(
        sim.apply_damage(enemy, paladin, 500, wc3_sim::AttackType::Chaos),
        0
    );
    step_n(&mut sim, 300);
    assert!(
        sim.apply_damage(enemy, paladin, 10, wc3_sim::AttackType::Chaos) > 0,
        "shield expires"
    );
}

#[test]
fn spells_require_the_caster_to_know_them() {
    let mut sim = Sim::new(78, 16, 16);
    let plain = footman(&mut sim, 0, Vec2::new(200.0, 200.0));
    let ally = footman(&mut sim, 0, Vec2::new(300.0, 200.0));
    assert_eq!(
        sim.cast(plain, Spell::HolyLight, SpellTarget::Unit(ally)),
        Err("spell not available")
    );
}

#[test]
fn spell_kills_emit_unit_died() {
    let mut sim = Sim::new(79, 32, 32);
    let archmage = caster(&mut sim, "Hamg", Vec2::new(500.0, 500.0));
    let victim = footman(&mut sim, 1, Vec2::new(900.0, 500.0));
    sim.unit_mut(victim).unwrap().hp = 5;
    sim.cast(
        archmage,
        Spell::Blizzard,
        SpellTarget::Point(Vec2::new(900.0, 500.0)),
    )
    .expect("blizzard");
    let events = step_collect(&mut sim, 120);
    assert!(!sim.unit(victim).unwrap().alive);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, wc3_sim::Event::UnitDied(id) if *id == victim))
    );
}

// ---------------------------------------------------------------------------
// Orc01 campaign acceptance + roster-wide coverage (lane 511).
//
// The whole build is accepted on `Maps\Campaign\Orc01.w3m`, so the coverage
// test reads the real map: 37 placed unit types and the three ability rawcodes
// its script adds (`AOcl`, `AOsh`, `AOws`).  No synthetic unit ids are used for
// the mission check.

mod orc01 {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    use wc3_formats::map::{map_mpq_base, parse_units};
    use wc3_mpq::MpqArchive;
    use wc3_sim::abilities::{
        ability_coverage, census_ability_ids, curated_data_only_reason, data_ability_has_effect,
        generic_ability, roster_ability_ids, spell_for_ability, Spell,
    };
    use wc3_sim::{def_of, Event, Sim, SpellTarget, Vec2};

    fn orc01_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3/Maps/Campaign/Orc01.w3m")
    }

    fn orc01_archive() -> Option<MpqArchive> {
        let path = orc01_path();
        if !path.is_file() {
            return None;
        }
        let base = map_mpq_base(&path).ok()?;
        MpqArchive::open_at(&path, base).ok()
    }

    fn placed_unit_types(archive: &MpqArchive) -> Vec<String> {
        let bytes = archive.read("war3mapUnits.doo").expect("war3mapUnits.doo");
        let placed = parse_units(&bytes).expect("parse units");
        let mut ids: Vec<String> = placed
            .iter()
            .map(|unit| String::from_utf8_lossy(&unit.type_id).to_string())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    fn script_ability_codes(archive: &MpqArchive) -> BTreeSet<&'static str> {
        let bytes = archive
            .read("war3map.j")
            .or_else(|_| archive.read("Scripts\\war3map.j"))
            .or_else(|_| archive.read("scripts\\war3map.j"))
            .expect("map script");
        let text = String::from_utf8_lossy(&bytes);
        let mut codes = BTreeSet::new();
        let bytes = text.as_bytes();
        let mut index = 0usize;
        while index + 5 < bytes.len() {
            if bytes[index] == b'\'' {
                let inner = &bytes[index + 1..index + 5];
                if bytes[index + 5] == b'\''
                    && inner[0] == b'A'
                    && inner.iter().all(|byte| byte.is_ascii_alphanumeric())
                {
                    let code = String::from_utf8_lossy(inner).to_string();
                    if let Some(def) = generic_ability(&code) {
                        codes.insert(def.id);
                    }
                }
            }
            index += 1;
        }
        codes
    }

    /// Every ability referenced by Orc01: placed-unit `UnitAbilities.slk` rows,
    /// the rawcodes its script adds, and `abil` lists from the map's `w3u`
    /// object data (campaign custom units).
    fn orc01_ability_ids(archive: &MpqArchive) -> Vec<&'static str> {
        let mut ids = BTreeSet::new();
        for type_id in placed_unit_types(archive) {
            let Some(def) = def_of(&type_id) else {
                continue;
            };
            for ability in def.ability_ids {
                ids.insert(*ability);
            }
        }
        ids.extend(script_ability_codes(archive));
        ids.extend(object_data_abilities(archive));
        ids.into_iter().collect()
    }

    /// Ability ids written into `war3map.w3u` unit-object rows: the `abil`
    /// override plus the base unit's `UnitAbilities` rows when the map builds a
    /// campaign unit on top of a known base id.
    fn object_data_abilities(archive: &MpqArchive) -> BTreeSet<&'static str> {
        let mut ids = BTreeSet::new();
        let Ok(bytes) = archive.read("war3map.w3u") else {
            return ids;
        };
        let Ok(data) = wc3_formats::objects::ObjectData::parse(&bytes) else {
            return ids;
        };
        println!("ORC01 war3map.w3u entries={}", data.entries.len());
        for entry in data.entries {
            let old_id = String::from_utf8_lossy(&entry.old_id).to_string();
            let new_id = String::from_utf8_lossy(&entry.new_id).to_string();
            if let Some(base) = def_of(&old_id) {
                for ability in base.ability_ids {
                    ids.insert(*ability);
                }
            }
            for modification in entry.modifications {
                if modification.id != *b"abil" {
                    continue;
                }
                if let wc3_formats::objects::ModValue::String(list) = modification.value {
                    println!("ORC01 w3u {new_id} (base {old_id}) abil={list}");
                    for code in list.split(',') {
                        let code = code.trim();
                        if let Some(def) = generic_ability(code) {
                            ids.insert(def.id);
                        }
                    }
                }
            }
        }
        ids
    }

    fn placed_carrier(archive: &MpqArchive, ability: &str) -> Option<&'static str> {
        for type_id in placed_unit_types(archive) {
            let Some(def) = def_of(&type_id) else {
                continue;
            };
            if def.ability_ids.contains(&ability) {
                return Some(def.id);
            }
        }
        None
    }

    /// Base id of a map-defined unit (`war3map.w3u`), if the map defines it.
    fn object_data_base(archive: &MpqArchive, unit_id: &str) -> Option<String> {
        let bytes = archive.read("war3map.w3u").ok()?;
        let data = wc3_formats::objects::ObjectData::parse(&bytes).ok()?;
        let entry = data
            .entries
            .iter()
            .find(|entry| String::from_utf8_lossy(&entry.new_id) == unit_id)?;
        Some(String::from_utf8_lossy(&entry.old_id).to_string())
    }

    /// `abilities.orc01.roster-resolution`: every ability used by the mission
    /// resolves to an effect row or to a DataOnly row with a printable reason.
    #[test]
    fn abilities_orc01_roster_resolution() {
        let Some(archive) = orc01_archive() else {
            eprintln!("skip: Orc01.w3m fixture missing");
            return;
        };
        let ids = orc01_ability_ids(&archive);
        let mut item_placements = Vec::new();
        let mut unknown_units = Vec::new();
        for type_id in placed_unit_types(&archive) {
            if def_of(&type_id).is_some() {
                continue;
            }
            if wc3_sim::data::item_def(&type_id).is_some() {
                item_placements.push(type_id);
            } else {
                unknown_units.push(type_id);
            }
        }
        println!(
            "ORC01 doo entries: {} ground items {item_placements:?}; unit types without a def (content lane 507): {} {unknown_units:?}",
            item_placements.len(),
            unknown_units.len()
        );
        for unit_id in &unknown_units {
            match object_data_base(&archive, unit_id) {
                Some(base) => println!("ORC01 unit {unit_id} is map-defined on base {base}"),
                None => println!("ORC01 unit {unit_id} is not in war3map.w3u (campaign-only def)"),
            }
        }
        let census = census_ability_ids(&ids);
        println!(
            "ORC01 abilities total={} effectful={} data_only={} typed={} unmapped={} missing_reason={}",
            census.total,
            census.effectful,
            census.data_only,
            census.typed,
            census.unmapped.len(),
            census.missing_reason.len()
        );
        assert_eq!(census.unmapped, Vec::<String>::new(), "unmapped Orc01 abilities");
        assert!(
            census.missing_reason.is_empty(),
            "DataOnly rows without a reason: {:?}",
            census.missing_reason
        );
        assert_eq!(census.total, ids.len(), "one coverage row per id");
        assert!(census.total >= 12, "Orc01 references at least 12 abilities");
        assert_eq!(
            census.total,
            census.effectful + census.data_only,
            "every row is effectful or DataOnly"
        );
        // The mission roster's known ids, observed from the map on 2026-11-18.
        for id in [
            "AOae", "AOcl", "AOeq", "AOfs", "AOre", "AOsf", "AOsh", "AOws", "Aion", "Asal", "Awar",
            "Sbsk",
        ] {
            assert!(ids.contains(&id), "Orc01 references {id}");
            assert!(ability_coverage(id).is_some(), "{id} has a coverage row");
        }
        // Effectful rows really have a handler family; the rest carry a reason.
        for row in &census.rows {
            match row.effect {
                Some(_) => assert!(row.reason.is_none(), "{} effectful without reason", row.id),
                None => assert!(
                    row.reason.as_deref().map(|r| !r.is_empty()).unwrap_or(false),
                    "{} has no effect and no reason",
                    row.id
                ),
            }
        }
        // Far Sight must not be sold as damage (Data11=3 is a reveal constant).
        assert_eq!(
            spell_for_ability("AOfs"),
            Some(Spell::DataOnly { id: "AOfs" }),
            "Far Sight is a vision row, not 3 area damage"
        );
        // Spirit Wolves: the summoned `osw1` type is absent from the shipped
        // def tables, so the summon handler would skip; classify it honestly.
        assert_eq!(
            spell_for_ability("AOsf"),
            Some(Spell::DataOnly { id: "AOsf" })
        );
        assert!(
            ability_coverage("AOsf").unwrap().reason.unwrap().contains("osw1"),
            "the Spirit Wolf reason names the missing unit type"
        );
    }

    /// `abilities.orc01.no-silent-noop`: an effectless ability carried by an
    /// Orc01 unit never pays mana and never emits a cast event.
    #[test]
    fn abilities_orc01_no_silent_noop() {
        let Some(archive) = orc01_archive() else {
            eprintln!("skip: Orc01.w3m fixture missing");
            return;
        };
        let ids = orc01_ability_ids(&archive);
        let mut checked = 0usize;
        for id in ids {
            let Some(row) = ability_coverage(id) else {
                panic!("{id} has no coverage row");
            };
            if row.effect.is_some() {
                continue;
            }
            let Some(carrier) = placed_carrier(&archive, id) else {
                println!("ORC01 no placed carrier for {id} (def absent; reason still required)");
                continue;
            };
            let mut sim = Sim::new(9110, 64, 64);
            let caster = sim
                .spawn_def(0, carrier, Vec2::new(500.0, 500.0))
                .unwrap_or_else(|| panic!("spawn Orc01 carrier {carrier}"));
            {
                let unit = sim.unit_mut(caster).unwrap();
                unit.mana = unit.max_mana.max(10_000);
                if unit.def().hero && unit.def().ability_ids.contains(&id) {
                    unit.learned.push(id);
                }
            }
            let def = generic_ability(id).expect("row");
            let target = match def.target {
                wc3_sim::TargetKind::Point => SpellTarget::Point(Vec2::new(560.0, 500.0)),
                wc3_sim::TargetKind::Unit => {
                    let enemy = sim
                        .spawn_def(1, carrier, Vec2::new(560.0, 500.0))
                        .expect("enemy carrier");
                    SpellTarget::Unit(enemy)
                }
                wc3_sim::TargetKind::None => SpellTarget::None,
            };
            let mana_before = sim.unit(caster).unwrap().mana;
            let events_before = sim.events.len();
            let spell = spell_for_ability(id).expect("resolved spell");
            let result = sim.cast(caster, spell, target);
            let unit = sim.unit(caster).unwrap();
            assert!(result.is_err(), "{id} on {carrier} must refuse, got Ok");
            assert_eq!(
                unit.mana, mana_before,
                "{id} on {carrier} paid mana for no effect"
            );
            assert!(
                !sim.events[events_before..]
                    .iter()
                    .any(|event| matches!(event, Event::DataAbilityCast { ability, .. } if *ability == id)),
                "{id} on {carrier} emitted a cast event without an effect"
            );
            checked += 1;
        }
        println!("ORC01 effectless abilities checked={checked}");
        assert!(checked >= 6, "Orc01 has at least six effectless abilities");
    }

    /// `abilities.orc01.effects-run`: the mission's effectful abilities execute
    /// through the real cast paths on Orc01 defs (no synthetic units).
    #[test]
    fn abilities_orc01_effects_run() {
        let Some(archive) = orc01_archive() else {
            eprintln!("skip: Orc01.w3m fixture missing");
            return;
        };
        // Chain Lightning (typed) from Thrall onto a map creep.
        let mut sim = Sim::new(9111, 64, 64);
        let thrall = sim
            .spawn_def(
                0,
                placed_carrier(&archive, "AOcl").expect("Orc01 Thrall"),
                Vec2::new(500.0, 500.0),
            )
            .expect("Thrall");
        sim.unit_mut(thrall).unwrap().learned.push("AOcl");
        let creep = sim
            .spawn_def(1, "nrzb", Vec2::new(700.0, 500.0))
            .expect("Orc01 creep def");
        let hp = sim.unit(creep).unwrap().hp;
        assert_eq!(spell_for_ability("AOcl"), Some(Spell::ChainLightning));
        sim.cast(thrall, Spell::ChainLightning, SpellTarget::Unit(creep))
            .expect("chain lightning");
        assert!(
            sim.unit(creep).unwrap().hp < hp,
            "Chain Lightning damages the mission creep"
        );
        assert!(sim
            .events
            .iter()
            .any(|event| matches!(event, Event::SpellHit { .. })));

        // Earthquake (data area damage) from the same Thrall row.
        sim.unit_mut(thrall).unwrap().learned.push("AOeq");
        sim.unit_mut(thrall).unwrap().mana = 500;
        let hp_before_quake = sim.unit(creep).unwrap().hp;
        let quake = spell_for_ability("AOeq").expect("AOeq row");
        sim.cast(
            thrall,
            quake,
            SpellTarget::Point(Vec2::new(700.0, 500.0)),
        )
        .expect("earthquake");
        assert!(
            sim.unit(creep).unwrap().hp < hp_before_quake,
            "Earthquake damages the mission creep"
        );

        // Shock Wave (data damage) from Cairne at a point next to a creep.
        let mut sim = Sim::new(9112, 64, 64);
        let cairne = sim
            .spawn_def(
                0,
                placed_carrier(&archive, "AOsh").expect("Orc01 Cairne"),
                Vec2::new(500.0, 500.0),
            )
            .expect("Cairne");
        sim.unit_mut(cairne).unwrap().learned.push("AOsh");
        let creep = sim
            .spawn_def(1, "nrzb", Vec2::new(620.0, 500.0))
            .expect("Orc01 creep def");
        let hp = sim.unit(creep).unwrap().hp;
        let data_spell = spell_for_ability("AOsh").expect("AOsh row");
        assert!(matches!(data_spell, Spell::DataDamage { .. }));
        sim.cast(
            cairne,
            data_spell,
            SpellTarget::Point(Vec2::new(620.0, 500.0)),
        )
        .expect("shock wave");
        assert!(
            sim.unit(creep).unwrap().hp < hp,
            "Shock Wave damages the mission creep"
        );

        // War Stomp (unit-target data row) from the same Cairne.
        let mut sim = Sim::new(9113, 64, 64);
        let cairne = sim
            .spawn_def(
                0,
                placed_carrier(&archive, "AOws").expect("Orc01 Cairne"),
                Vec2::new(500.0, 500.0),
            )
            .expect("Cairne");
        sim.unit_mut(cairne).unwrap().learned.push("AOws");
        let creep = sim
            .spawn_def(1, "nrzb", Vec2::new(510.0, 500.0))
            .expect("Orc01 creep def");
        let hp = sim.unit(creep).unwrap().hp;
        let stomp = spell_for_ability("AOws").expect("AOws row");
        sim.cast(cairne, stomp, SpellTarget::Unit(creep))
            .expect("war stomp");
        assert!(
            sim.unit(creep).unwrap().hp < hp,
            "War Stomp damages the mission creep"
        );
    }

    /// `abilities.coverage.roster`: the full shipped roster (all races, creeps,
    /// shops, campaign defs) has no unresolved id and no unexplained DataOnly.
    #[test]
    fn abilities_roster_coverage() {
        let ids = roster_ability_ids();
        let census = census_ability_ids(&ids);
        println!(
            "ROSTER abilities total={} effectful={} data_only={} typed={} unmapped={} missing_reason={}",
            census.total,
            census.effectful,
            census.data_only,
            census.typed,
            census.unmapped.len(),
            census.missing_reason.len()
        );
        assert_eq!(census.unmapped, Vec::<String>::new());
        assert!(census.missing_reason.is_empty(), "{:?}", census.missing_reason);
        assert!(census.total >= 200, "roster references: {}", census.total);
        assert!(
            census.effectful >= 50,
            "effectful roster abilities: {}",
            census.effectful
        );
        // Every curated mission reason is still attached to a resolved id.
        for (id, reason) in wc3_sim::abilities::DATA_ONLY_REASONS {
            assert!(!reason.is_empty(), "{id} curated reason is empty");
            assert_eq!(
                curated_data_only_reason(id),
                Some(*reason),
                "{id} lookup"
            );
            let def = generic_ability(id).unwrap_or_else(|| panic!("{id} row"));
            assert!(!data_ability_has_effect(def), "{id} is curated as no-effect");
        }
        // No resolved row may be effectless without a reason.
        for row in &census.rows {
            if row.effect.is_none() {
                assert!(
                    row.reason.as_deref().map(|r| !r.is_empty()).unwrap_or(false),
                    "{} effectless without a reason",
                    row.id
                );
            }
        }
    }
}
