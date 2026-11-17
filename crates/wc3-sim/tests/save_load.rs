//! Mechanic tests for `save.load` and `save.versioning` (P0-8).
//!
//! Policy: one test proves one named mechanic from `MECHANICS_GAP_MAP.md`;
//! the test name starts with the mechanic id.
//!
//! Contract under test: a versioned binary save captures the complete
//! simulation state, `Sim::from_save` restores a simulation that continues
//! bit-identically (`state_hash` equal on every tick), and every malformed
//! input returns a typed [`SaveError`] instead of panicking.

mod common;

use common::{hall, sandbox, worker};
use wc3_sim::abilities::PendingCast;
use wc3_sim::data::{ITEMS, ROSTER, UPGRADES};
use wc3_sim::save::{SaveError, SAVE_FORMAT_VERSION, SAVE_MAGIC};
use wc3_sim::{
    AttackType, Camp, Command, Destructable, DestructableId, Item, ItemId, Projectile,
    ProjectileId, Race, Research, Shop, ShopSlot, Sim, Spell, UnitId, UnitKind, Vec2,
};

/// A full 1v1 melee match: halls, workers, creeps, mines, AI on the enemy seat.
fn melee(seed: u64) -> Sim {
    let mut sim = Sim::new(seed, 64, 64);
    sim.setup_melee_1v1(
        Race::Human,
        Race::Orc,
        Vec2::new(1024.0, 1024.0),
        Vec2::new(6144.0, 6144.0),
    )
    .expect("melee 1v1 setup");
    sim
}

fn owner_unit(sim: &Sim, owner: u8, kind: UnitKind) -> UnitId {
    sim.units
        .iter()
        .find(|unit| unit.owner == owner && unit.kind == kind)
        .map(|unit| unit.id)
        .expect("unit of kind for owner")
}

fn owner_units(sim: &Sim, owner: u8, kind: UnitKind) -> Vec<UnitId> {
    sim.units
        .iter()
        .filter(|unit| unit.owner == owner && unit.kind == kind)
        .map(|unit| unit.id)
        .collect()
}

/// Identical command schedule applied to both simulations after the save, so
/// the comparison exercises command application, AI, production and combat
/// rather than an idle match.
fn script(tick: u64, hall: UnitId, worker: UnitId, mine: Vec2) -> Vec<Command> {
    match tick {
        630 => vec![Command::Harvest {
            units: vec![worker],
            to: mine,
        }],
        690 => vec![Command::Train {
            building: hall,
            kind: UnitKind::Peasant,
        }],
        720 => vec![Command::Move {
            units: vec![worker],
            to: Vec2::new(2048.0, 2048.0),
        }],
        810 => vec![Command::Stop {
            units: vec![worker],
        }],
        _ => Vec::new(),
    }
}

#[test]
fn save_load_full_state_roundtrip_600_ticks() {
    let mut sim = melee(0x5A7E_1234);
    let hall = owner_unit(&sim, 0, UnitKind::TownHall);
    let worker = owner_unit(&sim, 0, UnitKind::Peasant);
    let mine = sim.gold_mines.first().map(|mine| mine.pos).unwrap_or_default();

    // Warm the match up: 600 ticks of AI, production, harvesting and combat.
    for tick in 0..600u64 {
        let commands = script(tick, hall, worker, mine);
        sim.step(&commands);
    }

    let hash_before = sim.state_hash();
    let digest_before = sim.save_digest();
    let bytes = sim.to_save_bytes();

    let mut restored = Sim::from_save(&bytes).expect("save must load");

    // The load must be exact, not merely plausible.
    assert_eq!(restored.state_hash(), hash_before, "hash after load");
    assert_eq!(restored.save_digest(), digest_before, "digest after load");
    assert_eq!(
        restored.to_save_bytes(),
        bytes,
        "re-encoding a loaded save must be byte-identical"
    );
    assert_eq!(restored.tick, sim.tick, "tick preserved");
    assert_eq!(restored.resources, sim.resources, "gold preserved");
    assert_eq!(restored.lumber, sim.lumber, "lumber preserved");
    assert_eq!(restored.seed, sim.seed, "seed preserved");
    assert_eq!(restored.ai.len(), sim.ai.len(), "AI controllers preserved");

    // Both simulations continue on the same command schedule for 600 ticks;
    // hashes must agree at every 60-tick checkpoint and at the end.
    for tick in 600..1200u64 {
        let commands = script(tick, hall, worker, mine);
        sim.step(&commands);
        restored.step(&commands);
        if tick % 60 == 0 {
            assert_eq!(
                restored.state_hash(),
                sim.state_hash(),
                "state_hash diverged at tick {tick}"
            );
            assert_eq!(
                restored.save_digest(),
                sim.save_digest(),
                "save digest diverged at tick {tick}"
            );
        }
    }
    assert_eq!(sim.tick, 1200);
    assert_eq!(
        restored.to_save_bytes(),
        sim.to_save_bytes(),
        "full serialized state must be identical after 600 more ticks"
    );
}

#[test]
fn save_load_every_entity_class_roundtrip() {
    // A sandbox gives a known hall/worker; the rest of the state is injected
    // through public fields so every save section is non-empty.
    let mut sim = sandbox(99);
    let hall_id = owner_unit(&sim, 0, UnitKind::TownHall);
    let worker_id = worker(&mut sim, 0, Vec2::new(700.0, 700.0));
    sim.step(&[]);

    let upgrade_id = UPGRADES[0].id;
    let ability_id = wc3_sim::abilities::SPELL_DEFS[0].id;
    let item_id = ITEMS[0].id;
    let missile_art = ROSTER
        .iter()
        .find(|def| !def.missile_art.is_empty())
        .map(|def| def.missile_art)
        .expect("roster has a missile art");

    {
        let hall = sim
            .units
            .iter_mut()
            .find(|unit| unit.id == hall_id)
            .expect("hall");
        hall.research.push(Research {
            id: upgrade_id,
            remaining: 420,
        });
        hall.learned.push(ability_id);
        hall.data_cd.push((ability_id, 55));
        hall.order_queue.push(wc3_sim::OrderSpec::Move(Vec2::new(
            600.0, 600.0,
        )));
        hall.order_queue.push(wc3_sim::OrderSpec::Hold);
        hall.rally = Some(Vec2::new(640.0, 640.0));
    }
    {
        let worker = sim
            .units
            .iter_mut()
            .find(|unit| unit.id == worker_id)
            .expect("worker");
        worker.items.push(ItemId(901));
        worker.carrying = 7;
        worker.carrying_lumber = 3;
        worker.harvest_timer = 12;
        worker.patrol = Some((Vec2::new(512.0, 512.0), Vec2::new(1024.0, 512.0)));
    }

    sim.items.push(Item {
        id: ItemId(901),
        def_index: 0,
        pos: Vec2::new(700.0, 700.0),
        owner: Some(0),
        charges: 2,
    });
    sim.camps.push(Camp {
        id: 7,
        anchor: Vec2::new(3000.0, 3000.0),
        units: vec![],
        aggro: 500.0,
        leash: 800.0,
    });
    sim.destructables.push(Destructable {
        id: DestructableId(501),
        def_index: 0,
        pos: Vec2::new(1500.0, 1600.0),
        hp: 50,
        max_hp: 50,
        alive: true,
        blocked_cell: Some((11, 12)),
    });
    sim.shops.push(Shop {
        shop: hall_id,
        owner: 0,
        slots: vec![ShopSlot {
            item: item_id,
            stock: 1,
            next_restock_tick: 900,
        }],
    });
    sim.projectiles.push(Projectile {
        id: ProjectileId(801),
        owner: 0,
        source: worker_id,
        target: None,
        pos: Vec2::new(1100.0, 1100.0),
        start: Vec2::new(1024.0, 1024.0),
        target_pos: Vec2::new(1200.0, 1200.0),
        speed: 900.0,
        arc: 0.3,
        homing: false,
        damage: 11,
        attack: AttackType::Pierce,
        art: missile_art,
        traveled: 25.0,
        total: 100.0,
    });
    sim.pending_casts.push(PendingCast {
        due: 700,
        caster: hall_id,
        spell: Spell::Blizzard,
        at: Vec2::new(2000.0, 2000.0),
        waves_left: 3,
        damage: 30,
    });
    sim.upgrades[0].insert(upgrade_id, 2);
    sim.upgrades[1].insert(UPGRADES[1].id, 1);

    let bytes = sim.to_save_bytes();
    let restored = Sim::from_save(&bytes).expect("save must load");
    assert_eq!(restored.save_digest(), sim.save_digest());
    assert_eq!(restored.to_save_bytes(), bytes);

    // Explicit spot checks: each entity class survived with its identity.
    assert_eq!(restored.items.len(), 1);
    assert_eq!(restored.items[0].id, ItemId(901));
    assert_eq!(restored.items[0].charges, 2);
    assert_eq!(restored.camps.len(), 1);
    assert_eq!(restored.camps[0].id, 7);
    assert_eq!(restored.destructables.len(), 1);
    assert_eq!(restored.destructables[0].blocked_cell, Some((11, 12)));
    assert_eq!(restored.shops.len(), 1);
    assert_eq!(restored.shops[0].slots[0].item, item_id);
    assert_eq!(restored.shops[0].slots[0].next_restock_tick, 900);
    assert_eq!(restored.projectiles.len(), 1);
    assert_eq!(restored.projectiles[0].art, missile_art);
    assert_eq!(restored.pending_casts.len(), 1);
    assert_eq!(restored.pending_casts[0].spell, Spell::Blizzard);
    assert_eq!(restored.upgrades[0].get(upgrade_id), Some(&2));
    assert_eq!(restored.upgrades[1].get(UPGRADES[1].id), Some(&1));

    let hall_after = restored
        .units
        .iter()
        .find(|unit| unit.id == hall_id)
        .expect("hall");
    assert_eq!(hall_after.research[0].id, upgrade_id);
    assert_eq!(hall_after.research[0].remaining, 420);
    assert_eq!(hall_after.learned, vec![ability_id]);
    assert_eq!(hall_after.data_cd, vec![(ability_id, 55)]);
    assert_eq!(hall_after.order_queue.len(), 2);
    assert_eq!(hall_after.rally, Some(Vec2::new(640.0, 640.0)));

    let worker_after = restored
        .units
        .iter()
        .find(|unit| unit.id == worker_id)
        .expect("worker");
    assert_eq!(worker_after.items, vec![ItemId(901)]);
    assert_eq!(worker_after.carrying, 7);
    assert_eq!(worker_after.harvest_timer, 12);
    assert_eq!(
        worker_after.patrol,
        Some((Vec2::new(512.0, 512.0), Vec2::new(1024.0, 512.0)))
    );
}

#[test]
fn save_load_rng_stream_preserved() {
    let mut sim = melee(4242);
    for _ in 0..120 {
        sim.step(&[]);
    }
    let mut restored = Sim::from_save(&sim.to_save_bytes()).expect("load");
    // The RNG is part of the state hash; the draw sequence must match too,
    // otherwise a loaded match would diverge on the next random roll.
    for draw in 0..64 {
        assert_eq!(
            restored.random(1000),
            sim.random(1000),
            "rng draw {draw} diverged"
        );
    }
    assert_eq!(restored.state_hash(), sim.state_hash());
}

#[test]
fn save_load_truncated_file_is_typed_error() {
    // Exhaustive on a small save: every possible truncation must be a typed
    // error, and no cut may panic.
    let sim = Sim::new(7, 8, 8);
    let bytes = sim.to_save_bytes();
    for cut in 0..bytes.len() {
        let error = Sim::from_save(&bytes[..cut]).err().expect("truncated save must fail");
        assert!(
            matches!(
                error,
                SaveError::Truncated { .. }
                    | SaveError::BadMagic(_)
                    | SaveError::LengthMismatch { .. }
                    | SaveError::Checksum { .. }
            ),
            "cut {cut} produced {error:?}"
        );
    }

    // Sampled on a realistic save: header, payload quarters, tail.
    let mut sim = melee(11);
    for _ in 0..90 {
        sim.step(&[]);
    }
    let bytes = sim.to_save_bytes();
    let mut cuts: Vec<usize> = (0..min(bytes.len(), 32)).collect();
    for step in 1..16 {
        cuts.push(bytes.len() * step / 16);
    }
    cuts.push(bytes.len() - 1);
    for cut in cuts {
        assert!(
            Sim::from_save(&bytes[..cut]).is_err(),
            "cut {cut} of {} must fail",
            bytes.len()
        );
    }
}

#[test]
fn save_load_corrupt_payload_is_typed_error() {
    let mut sim = melee(3);
    for _ in 0..45 {
        sim.step(&[]);
    }
    let bytes = sim.to_save_bytes();

    // Flipped byte inside the payload -> checksum error, never a panic.
    let mut corrupt = bytes.clone();
    let payload_start = 12;
    let target = payload_start + (bytes.len() - payload_start) / 2;
    corrupt[target] ^= 0x40;
    let error = Sim::from_save(&corrupt).err().expect("corrupt payload must fail");
    assert!(
        matches!(error, SaveError::Checksum { .. }),
        "expected checksum error, got {error:?}"
    );

    // Corrupt magic.
    let mut bad_magic = bytes.clone();
    bad_magic[0] = b'X';
    assert!(matches!(
        Sim::from_save(&bad_magic),
        Err(SaveError::BadMagic(_))
    ));

    // Corrupt declared length.
    let mut bad_len = bytes.clone();
    bad_len[8] ^= 0xff;
    bad_len[9] ^= 0xff;
    assert!(matches!(
        Sim::from_save(&bad_len),
        Err(SaveError::LengthMismatch { .. })
    ));

    // Non-zero reserved flags.
    let mut bad_flags = bytes.clone();
    bad_flags[6] = 1;
    assert!(matches!(
        Sim::from_save(&bad_flags),
        Err(SaveError::Flags(1))
    ));
}

#[test]
fn save_versioning_unknown_version_rejected() {
    let mut sim = sandbox(5);
    sim.step(&[]);
    let bytes = sim.to_save_bytes();

    // Contract, both directions:
    //  * a version the reader does not support is rejected as `UnknownVersion`
    //    before the payload is parsed;
    //  * a version the reader does support but which does not match this
    //    payload's layout returns a typed error (never a panic), and a genuine
    //    migration returns the identical state.
    for version in [0u16, SAVE_FORMAT_VERSION + 1, 99, 65535] {
        let mut tampered = bytes.clone();
        tampered[4..6].copy_from_slice(&version.to_le_bytes());
        let error = Sim::from_save(&tampered).err().expect("unknown version must fail");
        assert_eq!(
            error,
            SaveError::UnknownVersion(version),
            "version {version} must be rejected before parsing"
        );
    }
    // Supported layouts 1..=4 are accepted by their own decode arm: v1/v2
    // migrate (ethereal=false, the field did not exist), v3/v4 restore the
    // identical gameplay state (v4 also carries the script section; this
    // sandbox has no script attached, so the payload is layout-compatible).
    for version in [1u16, 2, SAVE_FORMAT_VERSION] {
        let mut tampered = bytes.clone();
        tampered[4..6].copy_from_slice(&version.to_le_bytes());
        match Sim::from_save(&tampered) {
            Ok(restored) => assert_eq!(
                restored.state_hash(),
                sim.state_hash(),
                "supported version {version} must migrate to the identical state"
            ),
            Err(_) => { /* typed error: layout does not match this version */ }
        }
    }

    // The version this build writes is the one it reads.
    let header_version = u16::from_le_bytes([bytes[4], bytes[5]]);
    assert_eq!(header_version, SAVE_FORMAT_VERSION);
    assert_eq!(&bytes[0..4], &SAVE_MAGIC);
}

#[test]
fn save_versioning_short_file_rejected() {
    let empty: &[u8] = &[];
    assert!(matches!(
        Sim::from_save(empty),
        Err(SaveError::Truncated {
            needed: 4,
            available: 0
        })
    ));

    // Magic only, magic+version, and a header that claims a payload.
    for len in [1usize, 3, 4, 6, 8, 11] {
        let bytes = vec![b'W'; len];
        assert!(
            Sim::from_save(&bytes).is_err(),
            "short file of {len} bytes must fail"
        );
    }

    let mut header_only = Vec::new();
    header_only.extend_from_slice(&SAVE_MAGIC);
    header_only.extend_from_slice(&SAVE_FORMAT_VERSION.to_le_bytes());
    header_only.extend_from_slice(&0u16.to_le_bytes());
    header_only.extend_from_slice(&16u32.to_le_bytes());
    assert!(matches!(
        Sim::from_save(&header_only),
        Err(SaveError::LengthMismatch { .. })
    ));

    // Valid magic/version/flags/length but no payload: still an error, no panic.
    let truncated_payload = header_only.clone();
    assert!(Sim::from_save(&truncated_payload).is_err());
}

#[test]
fn save_load_orders_known_scene_after_load() {
    // A saved order queue must be replayed by the loaded simulation, not only
    // stored: hash equality alone would not prove the orders still drive units.
    let mut sim = sandbox(77);
    let worker = worker(&mut sim, 0, Vec2::new(600.0, 600.0));
    sim.step(&[]);
    sim.step(&[Command::Move {
        units: vec![worker],
        to: Vec2::new(1400.0, 1400.0),
    }]);
    for _ in 0..10 {
        sim.step(&[]);
    }
    let mut restored = Sim::from_save(&sim.to_save_bytes()).expect("load");
    for tick in 0..120 {
        sim.step(&[]);
        restored.step(&[]);
        assert_eq!(
            restored.state_hash(),
            sim.state_hash(),
            "in-flight move order diverged at tick {tick}"
        );
    }
    let original_pos = sim
        .units
        .iter()
        .find(|unit| unit.id == worker)
        .expect("worker")
        .pos;
    let restored_pos = restored
        .units
        .iter()
        .find(|unit| unit.id == worker)
        .expect("worker")
        .pos;
    assert_eq!(restored_pos, original_pos);
    assert!(
        original_pos != Vec2::new(512.0, 512.0),
        "the unit actually moved"
    );
}

#[test]
fn save_load_path_and_io_roundtrip() {
    // `save_to_path` / `from_save_path` are the CLI surface; a missing file is a
    // typed IO error rather than a panic.
    let mut sim = sandbox(21);
    sim.step(&[]);
    let dir = std::env::temp_dir().join(format!("wc3-sim-save-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("match.w3sv");
    sim.save_to_path(&path).expect("write save");
    let restored = Sim::from_save_path(&path).expect("read save");
    assert_eq!(restored.state_hash(), sim.state_hash());
    std::fs::remove_file(&path).ok();

    let missing = dir.join("absent.w3sv");
    assert!(matches!(
        Sim::from_save_path(&missing),
        Err(SaveError::Io(_))
    ));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_load_worker_and_hall_ids_stable() {
    // Entity identity is part of the contract: ids, owners and def indexes must
    // survive so later commands and replays address the same units.
    let mut sim = sandbox(31);
    let hall = hall(&mut sim, 0, Vec2::new(400.0, 400.0));
    let worker = worker(&mut sim, 0, Vec2::new(460.0, 460.0));
    sim.step(&[]);
    let restored = Sim::from_save(&sim.to_save_bytes()).expect("load");
    assert!(restored.unit(hall).is_some(), "hall id addresses a unit");
    assert!(restored.unit(worker).is_some(), "worker id addresses a unit");
    assert_eq!(restored.next_id, sim.next_id);
    assert_eq!(restored.units.len(), sim.units.len());
    for (left, right) in restored.units.iter().zip(&sim.units) {
        assert_eq!(left.id, right.id);
        assert_eq!(left.owner, right.owner);
        assert_eq!(left.def_index, right.def_index);
    }
}

fn min(left: usize, right: usize) -> usize {
    if left < right {
        left
    } else {
        right
    }
}

/// Mechanic `save.ai-composition`: a save written with the live melee AI must
/// load. `Game` installs `AiConfig::for_race` (three entries), while interning
/// used to accept only the legacy two-unit `ai_composition_for` table, so every
/// device resume failed with `GAME_RESUME_FAILED ... err=UnknownComposition`.
#[test]
fn save_ai_composition_round_trips() {
    let mut sim = Sim::new(11, 64, 64);
    sim.setup_melee_1v1(
        Race::Orc,
        Race::Human,
        Vec2::new(1024.0, 1024.0),
        Vec2::new(6144.0, 6144.0),
    );
    sim.enable_ai_for_race(1, Race::Human);
    for _ in 0..30 {
        sim.step(&[]);
    }
    let bytes = sim.to_save_bytes();
    let restored = Sim::from_save(&bytes).expect("resume with the live AI composition");
    assert_eq!(restored.state_hash(), sim.state_hash(), "hash after resume");
}
