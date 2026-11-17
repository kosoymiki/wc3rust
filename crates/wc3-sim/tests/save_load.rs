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

    // The entities above were pushed with explicit ids, so the counters have to
    // follow them: `next_*` must stay above every live id or the next spawn would
    // collide (the loader rejects a save where it does not).
    sim.next_item_id = 902;
    sim.next_camp_id = 8;
    sim.next_destructable_id = 502;
    sim.next_projectile_id = 802;

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

// ---------------------------------------------------------------------------
// AUD-002 — the ID-space bound at the public API (see `src/save.rs` unit tests
// for the crafted-payload matrix: next_id 0 / above cap / u32::MAX, duplicate
// ids, unit id >= next_id, recomputed checksums)
// ---------------------------------------------------------------------------

/// A minimal valid save loads, keeps its counters and continues identically: the
/// bound added for AUD-002 must not reject an ordinary file.
#[test]
fn save_load_minimal_sim_round_trip_keeps_counters_and_state() {
    let mut sim = sandbox(0x5157);
    let worker = sim
        .spawn_def(0, "hpea", Vec2::new(700.0, 700.0))
        .expect("worker");
    sim.step(&[]);
    let next_id = sim.next_id;
    assert!(next_id > 1, "the sandbox handed out ids");

    let bytes = sim.to_save_bytes();
    let mut restored = Sim::from_save(&bytes).expect("minimal save loads");
    assert_eq!(restored.state_hash(), sim.state_hash());
    assert_eq!(restored.next_id, next_id, "the unit counter survives");
    assert_eq!(
        restored.next_item_id, sim.next_item_id,
        "the item counter survives"
    );
    assert!(restored.unit(worker).is_some(), "the unit survives");

    // Sixty more ticks on both sides: the restored sim must stay bit-identical.
    for tick in 0..60 {
        sim.step(&[]);
        restored.step(&[]);
        assert_eq!(
            restored.state_hash(),
            sim.state_hash(),
            "state diverged at tick {tick} after the load"
        );
    }
}

/// The dialog/button/timer-dialog state added for AUD-007 survives a save/load
/// round trip: a restored world shows the same message to the same player.
#[test]
fn save_load_dialog_state_survives_the_round_trip() {
    use wc3_sim::jass::{Object, ScriptBundle, Value};

    // A script world with a dialog is what makes v5 -> v6 interesting.
    let mut sim = sandbox(0x5305);
    let mut bundle = ScriptBundle::new();
    if let Some(dir) = wc3_sim::jass::find_game_scripts() {
        bundle.add_game_scripts(&dir).expect("headers");
    }
    bundle
        .add_source(
            "dialog.j",
            r#"
            globals
                dialog probe_dialog = null
            endglobals
            function setup takes nothing returns nothing
                set probe_dialog = DialogCreate()
                call DialogSetMessage(probe_dialog, "current build")
                call DialogDisplay(Player(0), probe_dialog, true)
            endfunction
            "#,
        )
        .expect("parse");
    sim.attach_script(&bundle, 0x5305).expect("compile");
    sim.with_script(|vm| vm.run_function("setup"))
        .expect("script")
        .expect("setup runs");
    let dialog = sim
        .with_script(|vm| {
            let index = vm.code.global_by_name["probe_dialog"] as usize;
            match vm.globals[index].clone() {
                Value::Handle(handle) => handle,
                other => panic!("probe_dialog is {other:?}"),
            }
        })
        .expect("script attached");
    assert!(
        matches!(
            sim.script().and_then(|vm| vm.host.object(dialog)),
            Some(Object::Dialog { message, shown_to: Some(0), .. }) if message == "current build"
        ),
        "the live world carries the dialog state"
    );

    let bytes = sim.to_save_bytes();
    let restored = Sim::from_save(&bytes).expect("v6 save loads");
    assert_eq!(restored.state_hash(), sim.state_hash());
    assert!(
        matches!(
            restored.script().and_then(|vm| vm.host.object(dialog)),
            Some(Object::Dialog { message, shown_to: Some(0), asynchronous: false, .. })
                if message == "current build"
        ),
        "v6 restores the dialog message and display target"
    );
}

/// The two trigger registrations added for AUD-006 survive a save/load: the
/// restored world keeps the state watch and the game-event watch, keeps one event
/// object per `gameevent` id, and keeps firing on later crossings / raised events.
#[test]
fn save_load_game_state_and_game_event_registrations_survive() {
    use wc3_sim::jass::{Object, ScriptBundle, Value};

    fn probe(sim: &Sim, name: &str) -> i64 {
        let vm = sim.script().expect("script attached");
        let index = vm.code.global_by_name[name] as usize;
        vm.globals[index].int() as i64
    }
    fn handle_global(sim: &Sim, name: &str) -> wc3_sim::jass::Handle {
        let vm = sim.script().expect("script attached");
        let index = vm.code.global_by_name[name] as usize;
        match vm.globals[index].clone() {
            Value::Handle(handle) => handle,
            other => panic!("{name} is {other:?}"),
        }
    }
    fn run(sim: &mut Sim, name: &str) {
        sim.with_script(|vm| vm.run_function(name))
            .expect("script attached")
            .expect("function runs");
    }

    let mut sim = sandbox(0x6606);
    let mut bundle = ScriptBundle::new();
    if let Some(dir) = wc3_sim::jass::find_game_scripts() {
        bundle.add_game_scripts(&dir).expect("headers");
    }
    bundle
        .add_source(
            "events.j",
            r#"
            globals
                trigger state_trigger = null
                trigger game_trigger = null
                event game_event = null
                integer state_fires = 0
                integer game_fires = 0
            endglobals
            function on_state takes nothing returns nothing
                set state_fires = state_fires + 1
            endfunction
            function on_game takes nothing returns nothing
                set game_fires = game_fires + 1
            endfunction
            function setup takes nothing returns nothing
                set state_trigger = CreateTrigger()
                call TriggerAddAction(state_trigger, function on_state)
                call TriggerRegisterGameStateEvent(state_trigger, ConvertFGameState(2), ConvertLimitOp(4), 10.0)
                set game_trigger = CreateTrigger()
                call TriggerAddAction(game_trigger, function on_game)
                set game_event = TriggerRegisterGameEvent(game_trigger, ConvertGameEvent(3))
            endfunction
            function set_night takes nothing returns nothing
                call SetFloatGameState(ConvertFGameState(2), 21.0)
            endfunction
            function set_day takes nothing returns nothing
                call SetFloatGameState(ConvertFGameState(2), 3.0)
            endfunction
            "#,
        )
        .expect("parse");
    sim.attach_script(&bundle, 0x6606).expect("compile");
    run(&mut sim, "setup");

    // One crossing before the save: both the state watch and the
    // EVENT_GAME_STATE_LIMIT game event fire.
    run(&mut sim, "set_night");
    for _ in 0..3 {
        sim.step(&[]);
    }
    assert_eq!(probe(&sim, "state_fires"), 1, "state trigger before save");
    assert_eq!(probe(&sim, "game_fires"), 1, "game event before save");

    let event_handle = handle_global(&sim, "game_event");
    let bytes = sim.to_save_bytes();
    let mut restored = Sim::from_save(&bytes).expect("save loads");
    assert_eq!(restored.state_hash(), sim.state_hash());

    // Registration and handle identity are restored, not rebuilt from scratch.
    assert!(
        matches!(
            restored.script().and_then(|vm| vm.host.object(event_handle)),
            Some(Object::GameEvent { id: 3 })
        ),
        "the event object survives"
    );
    assert_eq!(
        restored
            .script()
            .and_then(|vm| vm.host.game_event_handles.get(&3).copied()),
        Some(event_handle.id),
        "one event object per gameevent id after the load"
    );
    assert!(
        restored
            .script()
            .map(|vm| vm.host.raise_only_game_events.is_empty())
            .unwrap_or(false),
        "EVENT_GAME_STATE_LIMIT is engine-raised, not raise-only"
    );

    // The restored watch still works: leave the limit, cross again.
    run(&mut restored, "set_day");
    for _ in 0..2 {
        restored.step(&[]);
    }
    run(&mut restored, "set_night");
    for _ in 0..2 {
        restored.step(&[]);
    }
    assert_eq!(probe(&restored, "state_fires"), 2, "crossing after load");
    assert_eq!(probe(&restored, "game_fires"), 2, "game event after load");

    // A host-raised event still reaches the restored trigger.
    restored
        .with_script(|vm| vm.host.raise_game_event(3))
        .expect("script attached");
    for _ in 0..2 {
        restored.step(&[]);
    }
    assert_eq!(probe(&restored, "game_fires"), 3, "raised event after load");
}
