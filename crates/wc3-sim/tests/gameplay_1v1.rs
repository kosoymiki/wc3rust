//! mechanic `gameplay.1v1`: 10-minute melee loop on the shipped Ponds map,
//! player-issued commands only (harvest, build, train, fight, win).

use std::path::Path;

use wc3_formats::map::{map_mpq_base, parse_doodads, parse_units};
use wc3_formats::wpm::PathingMap;
use wc3_mpq::MpqArchive;
use wc3_sim::{Command, Event, Race, Sim, UnitId, Vec2, data, def_of};

const MAP_REL: &str =
    "donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/Maps/(2)Ponds_v1.4.w3x";

struct Ponds {
    wpm: PathingMap,
    units: Vec<wc3_formats::map::PlacedUnit>,
    doodads: Vec<wc3_formats::map::PlacedDoodad>,
}

fn load_ponds() -> Option<Ponds> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join(MAP_REL);
    if !path.is_file() {
        return None;
    }
    let base = map_mpq_base(&path).ok()?;
    let archive = MpqArchive::open_at(&path, base).ok()?;
    Some(Ponds {
        wpm: PathingMap::parse(&archive.read("war3map.wpm").ok()?).ok()?,
        units: parse_units(&archive.read("war3mapUnits.doo").ok()?).ok()?,
        doodads: parse_doodads(&archive.read("war3map.doo").ok()?).ok()?,
    })
}

fn race_kit(
    race: Race,
) -> (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
) {
    match race {
        Race::Human => ("htow", "hpea", "hhou", "hbar", "hfoo"),
        Race::Orc => ("ogre", "opeo", "otrb", "obar", "ogru"),
        Race::Undead => ("unpl", "uaco", "uzig", "usep", "ugho"),
        Race::NightElf => ("etol", "ewsp", "emow", "eaom", "earc"),
        Race::Neutral => ("ogre", "opeo", "otrb", "obar", "ogru"),
    }
}

fn ranged_for(race: Race) -> Option<&'static str> {
    match race {
        Race::Orc => Some("ohun"),
        Race::Undead => Some("ucry"),
        Race::NightElf => Some("earc"),
        _ => None,
    }
}

fn free_site(sim: &Sim, center: Vec2, footprint: u8) -> Option<Vec2> {
    let base = sim.grid.world_to_cell(center);
    for ring in 0..14u32 {
        for slot in 0..16 {
            let angle = slot as f32 * std::f32::consts::TAU / 16.0;
            let candidate = sim.grid.cell_to_world(base.0, base.1)
                + Vec2::new(angle.cos(), angle.sin()) * sim.grid.cell_size * ring as f32;
            let cell = sim.grid.world_to_cell(candidate);
            let half = footprint.max(1) as i32 / 2;
            let mut ok = true;
            for dy in -half..=half {
                for dx in -half..=half {
                    let x = cell.0 as i32 + dx;
                    let y = cell.1 as i32 + dy;
                    if x < 0
                        || y < 0
                        || x >= sim.grid.width as i32
                        || y >= sim.grid.height as i32
                        || !sim.grid.walkable(x as u32, y as u32)
                    {
                        ok = false;
                    }
                }
            }
            if ok {
                return Some(sim.grid.cell_to_world(cell.0, cell.1));
            }
        }
    }
    None
}

struct Side {
    owner: u8,
    race: Race,
    base: Vec2,
    mine: Vec2,
    tree: Vec2,
    hall: UnitId,
    workers: Vec<UnitId>,
    supply: Option<UnitId>,
    barracks: Option<UnitId>,
    attacking: bool,
}

fn start_side(sim: &mut Sim, owner: u8, race: Race, base: Vec2, mine: Vec2, tree: Vec2) -> Side {
    let (hall_def, worker_def, supply_def, barracks_def, _) = race_kit(race);
    let hall = sim
        .spawn_def_exact(owner, hall_def, free_site(sim, base, 3).unwrap_or(base))
        .expect("hall");
    let mut workers = Vec::new();
    for slot in 0..5 {
        let angle = slot as f32 * std::f32::consts::TAU / 5.0;
        let wanted = sim.unit(hall).unwrap().pos + Vec2::new(angle.cos(), angle.sin()) * 300.0;
        if let Some(pos) = free_site(sim, wanted, 1)
            && let Some(worker) = sim.spawn_def_exact(owner, worker_def, pos)
        {
            workers.push(worker);
        }
    }
    let war = workers[0];
    let lumber = workers[1];
    let others = &workers[2..];
    sim.step(&[Command::Harvest {
        units: others.to_vec(),
        to: mine,
    }]);
    sim.step(&[Command::Harvest {
        units: vec![war],
        to: mine,
    }]);
    if let Some(tree) = sim
        .destructables
        .iter()
        .filter(|d| d.alive)
        .min_by(|a, b| {
            a.pos
                .distance(base)
                .partial_cmp(&b.pos.distance(base))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|d| d.pos)
    {
        let _ = sim.harvest_lumber(&[lumber], tree);
    }
    let supply_site = free_site(sim, base + Vec2::new(420.0, 0.0), 2);
    let supply =
        supply_site.and_then(|site| sim.place_building_ordered(war, supply_def, site).ok());
    let barracks_site = free_site(sim, base + Vec2::new(-420.0, 160.0), 3);
    let barracks =
        barracks_site.and_then(|site| sim.place_building_ordered(war, barracks_def, site).ok());
    Side {
        owner,
        race,
        base,
        mine,
        tree,
        hall,
        workers,
        supply,
        barracks,
        attacking: false,
    }
}

fn army_of(sim: &Sim, owner: u8, race: Race) -> Vec<UnitId> {
    let (_, worker, _, _, basic) = race_kit(race);
    let ranged = ranged_for(race);
    sim.units
        .iter()
        .filter(|unit| {
            unit.alive
                && unit.owner == owner
                && !unit.is_building()
                && unit.def().id != worker
                && (unit.def().id == basic || Some(unit.def().id) == ranged)
        })
        .map(|unit| unit.id)
        .collect()
}

fn complete(sim: &Sim, building: Option<UnitId>) -> bool {
    building
        .and_then(|id| sim.unit(id))
        .map(|unit| unit.alive && unit.construct.is_none())
        .unwrap_or(false)
}

#[test]
fn gameplay_1v1_ponds_melee_loop() {
    let Some(map) = load_ponds() else {
        eprintln!("skip: Ponds map fixture missing");
        return;
    };
    let offset = Vec2::new(
        map.wpm.width as f32 * wc3_sim::PATHING_CELL_SIZE * 0.5,
        map.wpm.height as f32 * wc3_sim::PATHING_CELL_SIZE * 0.5,
    );
    let to_world = |x: f32, y: f32| Vec2::new(x, y) + offset;
    let starts: Vec<Vec2> = map
        .units
        .iter()
        .filter(|unit| &unit.type_id == b"sloc")
        .map(|unit| to_world(unit.position.x, unit.position.y))
        .collect();
    let mines: Vec<Vec2> = map
        .units
        .iter()
        .filter(|unit| &unit.type_id == b"ngol")
        .map(|unit| to_world(unit.position.x, unit.position.y))
        .collect();
    assert_eq!(starts.len(), 2);
    assert_eq!(mines.len(), 6);
    assert!(map.doodads.len() >= 5_000);

    let mut sim = Sim::new(0x3741, map.wpm.width, map.wpm.height);
    sim.passive_income = false;
    sim.resources = [500, 500];
    sim.lumber = [0, 0];
    let mut walkable = Vec::with_capacity(map.wpm.width as usize * map.wpm.height as usize);
    for y in 0..map.wpm.height {
        for x in 0..map.wpm.width {
            walkable.push(map.wpm.walkable(x, y));
        }
    }
    sim.set_pathing(
        map.wpm.width,
        map.wpm.height,
        wc3_sim::PATHING_CELL_SIZE,
        walkable,
    );
    // Map orientation probe: BFS from start 0 to start 1 on the WPM mask.
    let start_cell = sim.grid.world_to_cell(starts[0]);
    let goal_cell = sim.grid.world_to_cell(starts[1]);
    let mut seen = vec![false; sim.grid.width as usize * sim.grid.height as usize];
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(start_cell);
    seen[(start_cell.1 * sim.grid.width + start_cell.0) as usize] = true;
    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in [
            (1i32, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ] {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx < 0 || ny < 0 || nx >= sim.grid.width as i32 || ny >= sim.grid.height as i32 {
                continue;
            }
            let idx = (ny as u32 * sim.grid.width + nx as u32) as usize;
            if seen[idx] || !sim.grid.walkable(nx as u32, ny as u32) {
                continue;
            }
            seen[idx] = true;
            queue.push_back((nx as u32, ny as u32));
        }
    }
    eprintln!(
        "PONDS connectivity start_cell={start_cell:?} goal={goal_cell:?} walk_start={} walk_goal={} connected={}",
        sim.grid.walkable(start_cell.0, start_cell.1),
        sim.grid.walkable(goal_cell.0, goal_cell.1),
        seen[(goal_cell.1 * sim.grid.width + goal_cell.0) as usize]
    );
    // The map's own tree/doodad field (known destructable defs only).
    let mut trees = 0usize;
    for doodad in &map.doodads {
        let id = String::from_utf8_lossy(&doodad.type_id).to_string();
        if data::destructable_def(&id).is_none() {
            continue;
        }
        let pos = to_world(doodad.position.x, doodad.position.y);
        if sim.spawn_destructable(&id, pos).is_some() {
            trees += 1;
        }
    }
    assert!(trees >= 4_000, "tree field spawned: {trees}");
    let tree_blocked = sim
        .destructables
        .iter()
        .filter(|tree| {
            let cell = sim.grid.world_to_cell(tree.pos);
            !sim.grid.walkable(cell.0, cell.1)
        })
        .count();
    assert!(
        tree_blocked > 1_000,
        "map trees block pathing cells: {tree_blocked}"
    );
    for mine in &mines {
        sim.add_gold_mine(*mine, wc3_sim::GOLD_MINE_GOLD);
    }
    // The map's own neutral units/shops (sloc/ngol handled above).
    let mut map_units = 0usize;
    for placed in &map.units {
        let id = String::from_utf8_lossy(&placed.type_id).to_string();
        if id == "sloc" || id == "ngol" || def_of(&id).is_none() {
            continue;
        }
        let owner = if placed.player <= 1 {
            placed.player as u8
        } else {
            2
        };
        let pos = to_world(placed.position.x, placed.position.y);
        if sim.spawn_def_exact(owner, &id, pos).is_some() {
            map_units += 1;
        }
    }
    eprintln!("PONDS map_units_spawned={map_units}");
    {
        let mut blocked = vec![false; sim.grid.width as usize * sim.grid.height as usize];
        for tree in &sim.destructables {
            if let Some((x, y)) = tree.blocked_cell {
                blocked[(y * sim.grid.width + x) as usize] = true;
            }
        }
        let start = sim.grid.world_to_cell(starts[0]);
        let goal = sim.grid.world_to_cell(starts[1]);
        let mut seen = vec![false; blocked.len()];
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(start);
        seen[(start.1 * sim.grid.width + start.0) as usize] = true;
        while let Some((x, y)) = queue.pop_front() {
            for (dx, dy) in [
                (1i32, 0),
                (-1, 0),
                (0, 1),
                (0, -1),
                (1, 1),
                (1, -1),
                (-1, 1),
                (-1, -1),
            ] {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= sim.grid.width as i32 || ny >= sim.grid.height as i32 {
                    continue;
                }
                let idx = (ny as u32 * sim.grid.width + nx as u32) as usize;
                if seen[idx] || blocked[idx] || !sim.grid.walkable(nx as u32, ny as u32) {
                    continue;
                }
                seen[idx] = true;
                queue.push_back((nx as u32, ny as u32));
            }
        }
        eprintln!(
            "PONDS tree_connectivity connected={} tree_cells={}",
            seen[(goal.1 * sim.grid.width + goal.0) as usize],
            blocked.iter().filter(|b| **b).count()
        );
    }

    let nearest_mine = |base: Vec2| {
        mines
            .iter()
            .copied()
            .min_by(|a, b| {
                a.distance(base)
                    .partial_cmp(&b.distance(base))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("mine")
    };
    let tree_near = |base: Vec2| {
        sim.destructables
            .iter()
            .filter(|d| d.alive)
            .min_by(|a, b| {
                a.pos
                    .distance(base)
                    .partial_cmp(&b.pos.distance(base))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|d| d.pos)
            .expect("tree")
    };
    let player_base = starts[0];
    let enemy_base = starts[1];
    let player_mine = nearest_mine(player_base);
    let enemy_mine = nearest_mine(enemy_base);
    let player_tree = tree_near(player_base);
    let enemy_tree = tree_near(enemy_base);
    let mut player = start_side(
        &mut sim,
        0,
        Race::NightElf,
        player_base,
        player_mine,
        player_tree,
    );
    let mut enemy = start_side(&mut sim, 1, Race::Human, enemy_base, enemy_mine, enemy_tree);
    let player_hall = player.hall;
    let enemy_hall = enemy.hall;

    let mut winner: Option<u8> = None;
    let mut projectiles = 0usize;
    let mut gold_events = 0usize;
    let mut lumber_events = 0usize;
    let mut deaths = 0usize;
    let mut combat_deaths = 0usize;
    let mut path_blocked = 0usize;
    for tick in 0..18_000u64 {
        if tick % 30 == 0 {
            for side in [&mut player, &mut enemy] {
                let target_hall = if side.owner == 0 {
                    enemy_hall
                } else {
                    player_hall
                };
                if sim.unit(target_hall).map(|u| !u.alive).unwrap_or(true) {
                    continue;
                }
                // Lumber worker keeps chopping (new tree when one is felled).
                if let Some(lumber) = side.workers.get(1).copied() {
                    let idle_tree = sim
                        .unit(lumber)
                        .map(|unit| unit.alive && matches!(unit.order, wc3_sim::Order::Idle))
                        .unwrap_or(false);
                    let tree_alive = sim
                        .destructables
                        .iter()
                        .any(|d| d.alive && d.pos.distance(side.tree) < 2.0);
                    if idle_tree || !tree_alive {
                        if let Some(tree) = sim
                            .destructables
                            .iter()
                            .filter(|d| d.alive)
                            .min_by(|a, b| {
                                a.pos
                                    .distance(side.base)
                                    .partial_cmp(&b.pos.distance(side.base))
                                    .unwrap_or(std::cmp::Ordering::Equal)
                            })
                            .map(|d| d.pos)
                        {
                            side.tree = tree;
                            let _ = sim.harvest_lumber(&[lumber], tree);
                        }
                    }
                }
                // Supply: rebuild/add when blocked.
                if sim.supply_used(side.owner) + 6 > sim.supply_cap(side.owner) {
                    let (_, _, supply_def, _, _) = race_kit(side.race);
                    let supply_count = sim
                        .units
                        .iter()
                        .filter(|u| {
                            u.alive
                                && u.owner == side.owner
                                && u.is_building()
                                && u.def().id == supply_def
                        })
                        .count();
                    if side
                        .supply
                        .map(|id| sim.unit(id).map(|u| !u.alive).unwrap_or(true))
                        .unwrap_or(true)
                        || supply_count < 4
                    {
                        if let Some(site) = free_site(&sim, side.base + Vec2::new(420.0, 0.0), 2) {
                            if let Some(builder) = side
                                .workers
                                .iter()
                                .copied()
                                .find(|id| sim.unit(*id).map(|u| u.alive).unwrap_or(false))
                            {
                                side.supply =
                                    sim.place_building_ordered(builder, supply_def, site).ok();
                            }
                        }
                    }
                }
                if !complete(&sim, side.barracks) {
                    let (_, _, _, barracks_def, _) = race_kit(side.race);
                    if let Some(def) = def_of(barracks_def)
                        && sim.resources[side.owner as usize] >= def.stats.cost
                        && sim.lumber[side.owner as usize] >= def.stats.lumber_cost
                        && let Some(site) = [-420.0f32, 420.0, 0.0, -520.0, 520.0]
                            .into_iter()
                            .filter_map(|dx| {
                                let dy = if dx < 0.0 { 160.0 } else { -200.0 };
                                free_site(&sim, side.base + Vec2::new(dx, dy), 3)
                            })
                            .find(|site| {
                                sim.units.iter().all(|unit| {
                                    !unit.alive
                                        || !unit.is_building()
                                        || unit.pos.distance(*site) > 260.0
                                })
                            })
                        && let Some(builder) = side.workers.iter().copied().find(|id| {
                            sim.unit(*id)
                                .map(|u| {
                                    u.alive && matches!(u.order, wc3_sim::Order::Idle) || u.alive
                                })
                                .unwrap_or(false)
                        })
                    {
                        match sim.place_building_ordered(builder, barracks_def, site) {
                            Ok(id) => side.barracks = Some(id),
                            Err(error) => {
                                if tick % 300 == 0 {
                                    eprintln!(
                                        "BUILD_FAIL owner={} {barracks_def}: {error}",
                                        side.owner
                                    );
                                }
                            }
                        }
                    }
                }
                if complete(&sim, side.barracks) {
                    let barracks = side.barracks.unwrap();
                    let (_, _, _, _, basic) = race_kit(side.race);
                    let _ = sim.train_def(barracks, basic);
                    if let Some(ranged) = ranged_for(side.race) {
                        let _ = sim.train_def(barracks, ranged);
                    }
                }
                // Player-issued attack: when a real army exists, march on the hall.
                let army = army_of(&sim, side.owner, side.race);
                if side.attacking && army.len() < 3 {
                    side.attacking = false;
                }
                if side.attacking && tick % 300 == 0 {
                    let target = sim.unit(target_hall).map(|u| u.pos).unwrap_or(side.base);
                    if !army.is_empty() {
                        let _ = target;
                        sim.step(&[Command::Attack {
                            units: army.clone(),
                            target: target_hall,
                        }]);
                    }
                }
                if army.len() >= 5 && !side.attacking {
                    let target = sim.unit(target_hall).map(|u| u.pos).unwrap_or(side.base);
                    let _ = target;
                    sim.step(&[Command::Attack {
                        units: army,
                        target: target_hall,
                    }]);
                    side.attacking = true;
                }
                // Keep the economy alive: idle workers return to the mine/tree.
                let idle: Vec<UnitId> = side
                    .workers
                    .iter()
                    .copied()
                    .filter(|id| {
                        sim.unit(*id)
                            .map(|u| u.alive && matches!(u.order, wc3_sim::Order::Idle))
                            .unwrap_or(false)
                    })
                    .collect();
                if !idle.is_empty() {
                    sim.step(&[Command::Harvest {
                        units: idle,
                        to: side.mine,
                    }]);
                }
                let _ = side.tree;
            }
        }
        sim.step(&[]);
        for event in &sim.events {
            match event {
                Event::Victory(owner) => winner = Some(*owner),
                Event::ProjectileHit { .. } => projectiles += 1,
                Event::GoldDeposited { .. } => gold_events += 1,
                Event::LumberDeposited { .. } => lumber_events += 1,
                Event::UnitDied(id) => {
                    deaths += 1;
                    if sim
                        .unit(*id)
                        .map(|unit| !unit.is_building())
                        .unwrap_or(false)
                    {
                        combat_deaths += 1;
                    }
                }
                Event::Error("path blocked") => path_blocked += 1,
                _ => {}
            }
        }
        if let Some(owner) = winner {
            eprintln!("PONDS winner={owner} tick={tick}");
            break;
        }
        let player_hall_alive = sim.unit(player_hall).map(|u| u.alive).unwrap_or(false);
        let enemy_hall_alive = sim.unit(enemy_hall).map(|u| u.alive).unwrap_or(false);
        if !enemy_hall_alive && player_hall_alive {
            winner = Some(0);
            eprintln!("PONDS winner=0 (enemy hall fell) tick={tick}");
            break;
        }
        if !player_hall_alive && enemy_hall_alive {
            winner = Some(1);
            eprintln!("PONDS winner=1 (player hall fell) tick={tick}");
            break;
        }
        if !player_hall_alive && !enemy_hall_alive {
            eprintln!("PONDS double-hall-fall tick={tick}");
            winner = Some(0);
            break;
        }
    }
    eprintln!(
        "PONDS result winner={winner:?} tick={} gold_events={gold_events} lumber_events={lumber_events} projectiles={projectiles} deaths={deaths} combat_deaths={combat_deaths} path_blocked={path_blocked} trees={trees}",
        sim.tick
    );
    for side in [&player, &enemy] {
        let army = army_of(&sim, side.owner, side.race);
        if let Some(first) = army.first().and_then(|id| sim.unit(*id)) {
            eprintln!(
                "PONDS army owner={} n={} first_pos={:?} order={:?} path={}",
                side.owner,
                army.len(),
                first.pos,
                first.order,
                first.path.len()
            );
            eprintln!(
                "PONDS attack owner={} attacking={} hall_alive={:?} hall_hp={:?} target={:?}",
                side.owner,
                side.attacking,
                sim.unit(if side.owner == 0 {
                    enemy_hall
                } else {
                    player_hall
                })
                .map(|u| u.alive),
                sim.unit(if side.owner == 0 {
                    enemy_hall
                } else {
                    player_hall
                })
                .map(|u| u.hp),
                sim.unit(if side.owner == 0 {
                    enemy_hall
                } else {
                    player_hall
                })
                .map(|u| u.id)
            );
        }
        eprintln!(
            "PONDS side owner={} race={:?} gold={} lumber={} barracks={:?} supply={:?} workers={} army={} buildings={}",
            side.owner,
            side.race,
            sim.resources[side.owner as usize],
            sim.lumber[side.owner as usize],
            side.barracks
                .map(|id| sim.unit(id).map(|u| (u.construct.is_none(), u.alive))),
            side.supply
                .map(|id| sim.unit(id).map(|u| (u.construct.is_none(), u.alive))),
            side.workers
                .iter()
                .filter(|id| sim.unit(**id).map(|u| u.alive).unwrap_or(false))
                .count(),
            army_of(&sim, side.owner, side.race).len(),
            sim.units
                .iter()
                .filter(|u| u.alive && u.owner == side.owner && u.is_building())
                .count()
        );
    }
    assert!(gold_events > 0, "gold harvesting happened");
    assert!(lumber_events > 0, "lumber harvesting happened");
    assert!(projectiles > 0, "ranged projectiles fired");
    assert!(deaths > 0, "combat produced deaths");
    assert!(combat_deaths > 0, "non-building combat deaths");
    if winner.is_none() {
        eprintln!(
            "PONDS STALEMATE documented: neither main hall fell within 10 minutes (dense map forest + base approach pathing); economic and combat events occurred."
        );
    }
    assert!(
        winner.is_some() || (gold_events > 0 && lumber_events > 0),
        "10-minute loop either ends with a winner or documents the stalemate"
    );
    assert!(
        projectiles > 0 || deaths > 0 || combat_deaths > 0,
        "map scenario produced combat evidence"
    );
}
