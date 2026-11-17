//! The gold sack is the carry geoset of `Walk Gold` / `Stand Gold` /
//! `stand work gold` (Peon.mdx). A hauling worker must play those clips, and a
//! worker walking to the tree must not play the chop swing.
use std::path::Path;
use wc3_game::assets::Assets;
use wc3_game::game::Game;
use wc3_sim::Command;

fn install() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let mpq = [
        "/data/data/com.termux/files/usr/tmp/opencode/wc3_iso/War3.mpq",
        "/data/data/org.herofolends.p0/files/War3.mpq",
    ]
    .iter()
    .map(std::path::PathBuf::from)
    .find(|path| path.is_file())?;
    let map = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/26/Warcraft 3 TFT-TFT multi-TFT 1.31.0.12071 multi-(2)Justice.w3x",
    );
    map.is_file().then_some((mpq, map))
}

#[test]
fn hauling_worker_plays_the_carry_clips() {
    let Some((mpq, map)) = install() else {
        eprintln!("skip: install/map missing");
        return;
    };
    let mut game = Game::new(Assets::load(Some(&mpq), Some(&map)), 0x5EED);
    game.enter_battle();
    let worker = game
        .sim
        .alive_units()
        .find(|unit| unit.owner == game.player && unit.def().stats.is_worker)
        .map(|unit| unit.id)
        .expect("worker");
    let start = game.sim.unit(worker).unwrap().pos;
    let tree = game
        .sim
        .destructables
        .iter()
        .filter(|tree| tree.alive)
        .min_by(|a, b| {
            (a.pos - start)
                .length()
                .partial_cmp(&(b.pos - start).length())
                .unwrap()
        })
        .map(|tree| tree.pos)
        .expect("a tree");
    game.issue(Command::HarvestLumber {
        units: vec![worker],
        to: tree,
    });

    let mut saw_walk = false;
    let mut saw_carry = false;
    let mut saw_work = false;
    let mut chopped_while_walking = 0usize;
    for _ in 0..1_800 {
        game.update(1.0 / 30.0);
        let Some(unit) = game.sim.unit(worker) else { break };
        let moving = !unit.path.is_empty() || unit.goal.is_some();
        let sequence = game
            .unit_sequence_name(worker)
            .unwrap_or_default()
            .to_ascii_lowercase();
        if moving && sequence.contains("attack") && !sequence.contains("lumber") {
            chopped_while_walking += 1;
        }
        if sequence.contains("walk") {
            saw_walk = true;
        }
        if sequence.contains("walk lumber") || sequence.contains("stand lumber") {
            saw_carry = true;
        }
        if sequence.contains("stand work lumber") || sequence.contains("attack lumber") {
            saw_work = true;
        }
    }
    eprintln!(
        "clips: walk={saw_walk} carry={saw_carry} work={saw_work} chop_while_walking={chopped_while_walking}"
    );
    assert!(saw_walk, "a hauling worker must play a Walk clip");
    assert!(saw_work, "chopping must play the lumber work clip");
    assert!(
        saw_carry,
        "the return trip must play `Walk Lumber` (that clip carries the log geoset)"
    );
    assert_eq!(
        chopped_while_walking, 0,
        "the worker must never play the chop swing while moving"
    );
}
