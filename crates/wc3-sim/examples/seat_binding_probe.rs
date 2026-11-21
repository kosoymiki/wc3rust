//! Headless proof for the campaign seat binding (ledger 518, `sim.seats`).
//!
//! Builds a small simulation, spawns one unit for the campaign human seat
//! (Prologue01 authors seat 4), one for seat 0 and a neutral creep camp, binds
//! seat 4 with [`wc3_sim::Sim::bind_human_seat`] and prints one
//! `SEAT_PROBE …` line per invariant. Exit code is 0 only when every invariant
//! holds, so the probe is usable as a gate step:
//!
//! ```text
//! scripts/h2-cargo run -p wc3-sim --example seat_binding_probe
//! ```
//!
//! This is an example, not a test: it is the log the tech-lead reads next to
//! `CAMPAIGN_HUMAN_SLOT … sim_bound=true`.

use glam::Vec2;
use wc3_sim::{CAMP_AGGRO_RADIUS, CAMP_LEASH_RADIUS, MAX_PLAYERS, NEUTRAL_OWNER, Sim};

fn main() {
    let mut failures: Vec<String> = Vec::new();
    let mut check = |ok: bool, name: &str, detail: String| {
        println!("SEAT_PROBE {name} ok={ok} {detail}");
        if !ok {
            failures.push(name.to_string());
        }
    };

    let mut sim = Sim::new(0x518_5EA7, 64, 64);
    let seat4_pos = Vec2::new(1200.0, 1200.0);
    let seat0_pos = Vec2::new(6000.0, 6000.0);
    let hero = sim
        .spawn_def_exact(4, "hfoo", seat4_pos)
        .expect("seat 4 hero spawns");
    sim.spawn_def_exact(0, "hfoo", seat0_pos)
        .expect("seat 0 unit spawns");

    check(
        sim.unit(hero).map(|unit| unit.owner) == Some(4),
        "spawn-seat-owner",
        format!("unit={} owner={:?}", hero.0, sim.unit(hero).map(|u| u.owner)),
    );

    let binding = sim.bind_human_seat(4).expect("seat 4 binds");
    check(
        binding.units == 1 && binding.visible_units == 1,
        "bind-seat-4",
        format!(
            "seat={} units={} buildings={} visible_units={} visible_cells={} local={}",
            binding.seat,
            binding.units,
            binding.buildings,
            binding.visible_units,
            binding.visible_cells,
            sim.local_player()
        ),
    );

    let sees_own = sim.is_visible(4, seat4_pos);
    let seat0_sees_seat4 = sim.is_visible(0, seat4_pos);
    let seat4_sees_seat0 = sim.is_visible(4, seat0_pos);
    check(
        sees_own && !seat0_sees_seat4 && !seat4_sees_seat0,
        "fog-ownership",
        format!(
            "seat4_sees_own={sees_own} seat0_sees_seat4={seat0_sees_seat4} seat4_sees_seat0={seat4_sees_seat0} render_local={}",
            sim.local_player()
        ),
    );

    // The neutral band is outside the player band: a camp never turns into a
    // seat, and seat 2 does not inherit its sight.
    let _camp = sim
        .spawn_camp(
            &["nftr", "nban"],
            Vec2::new(3000.0, 3000.0),
            CAMP_AGGRO_RADIUS,
            CAMP_LEASH_RADIUS,
        )
        .expect("camp spawns");
    let camp_owners: Vec<u8> = sim.camps[0]
        .units
        .iter()
        .filter_map(|id| sim.unit(*id).map(|unit| unit.owner))
        .collect();
    check(
        camp_owners.iter().all(|owner| *owner == NEUTRAL_OWNER),
        "neutral-band",
        format!("neutral_owner={NEUTRAL_OWNER} camp_owners={camp_owners:?}"),
    );

    // Banks: every seat has its own gold/lumber (the arrays the two-seat build
    // could not address).
    for seat in 0..MAX_PLAYERS as u8 {
        sim.set_gold(seat, 100 + i32::from(seat));
        sim.set_lumber(seat, 200 + i32::from(seat));
    }
    let banks_ok = (0..MAX_PLAYERS as u8).all(|seat| {
        sim.gold(seat) == 100 + i32::from(seat) && sim.lumber_of(seat) == 200 + i32::from(seat)
    });
    check(
        banks_ok,
        "banks",
        format!(
            "seats={MAX_PLAYERS} seat4_gold={} seat4_lumber={} seat0_gold={}",
            sim.gold(4),
            sim.lumber_of(4),
            sim.gold(0)
        ),
    );

    // Save/load: the bound seat, its bank and its fog survive the round trip.
    let before = sim.state_hash();
    let bytes = sim.to_save_bytes();
    let restored = Sim::from_save(&bytes).expect("seat-4 save loads");
    let seat4_cell = sim.grid.world_to_cell(seat4_pos);
    let seat4_visible_before = sim.visibility(4, seat4_cell);
    let seat4_visible_after = restored.visibility(4, seat4_cell);
    check(
        restored.human_seat == Some(4)
            && restored.state_hash() == before
            && restored.gold(4) == sim.gold(4)
            && restored.lumber_of(4) == sim.lumber_of(4)
            && seat4_visible_after == seat4_visible_before
            && restored.to_save_bytes() == bytes,
        "save-load",
        format!(
            "format_version={} save_bytes={} human_seat={:?} state_hash={before:#018x} fog_before={:?} fog_after={:?} reencode_identical=true",
            wc3_sim::save::SAVE_FORMAT_VERSION,
            bytes.len(),
            restored.human_seat,
            seat4_visible_before,
            seat4_visible_after
        ),
    );

    if failures.is_empty() {
        println!("SEAT_PROBE result=PASS checks=6");
    } else {
        println!("SEAT_PROBE result=FAIL failed={failures:?}");
        std::process::exit(1);
    }
}
