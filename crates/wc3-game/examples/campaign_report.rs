//! Campaign report: the campaign registry, one mission's live quest panel, the
//! cutscene beats the runtime played, and the storyboard file the analysis tree
//! keeps.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-game --example campaign_report -- \
//!     --mission Human01 --frames 900 --write
//! ```
//!
//! `--write` produces `analysis/campaign/<mission>.json` (the static mission
//! model) and `analysis/campaign/<mission>-storyboard.md` (the timed cutscene
//! beats: camera target/framing, fades, subtitles, and the hooks the screens
//! lane consumes).

use std::path::{Path, PathBuf};

use wc3_game::assets::Assets;
use wc3_game::campaign::{
    beat_totals, campaign_map_dir, CampaignProgress, CampaignRegistry, Mission,
    MissionSessionReport,
};
use wc3_game::game::{Game, TICK_SECONDS};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut mission_id = "Human01".to_string();
    let mut frames = 900usize;
    let mut write = false;
    let mut out_dir = PathBuf::from("analysis/campaign");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mission" => {
                if let Some(value) = args.next() {
                    mission_id = value;
                }
            }
            "--frames" => {
                if let Some(value) = args.next() {
                    frames = value.parse()?;
                }
            }
            "--write" => write = true,
            "--out" => {
                if let Some(value) = args.next() {
                    out_dir = PathBuf::from(value);
                }
            }
            other => eprintln!("ignoring unknown argument {other:?}"),
        }
    }

    let Some(dir) = campaign_map_dir() else {
        eprintln!("campaign maps not found: set HEROFOLENDS_DATA or HEROFOLENDS_CAMPAIGN");
        return Ok(());
    };
    let registry =
        CampaignRegistry::from_map_dir(&dir, CampaignProgress::default()).expect("registry");
    println!(
        "CAMPAIGN_ROOT dir={} chapters={} save={}",
        dir.display(),
        registry.len(),
        CampaignProgress::default_path().display()
    );
    println!("CAMPAIGN_CHAPTERS idx\tstate\tinterlude\tid\ttitle");
    for row in registry.rows() {
        println!(
            "  {}\t{}\t{}\t{}\t{}",
            row.index,
            row.state.as_str(),
            row.interlude,
            row.id,
            row.title
        );
    }

    let map = dir.join(format!("{mission_id}.w3m"));
    if !map.is_file() {
        eprintln!("map {} missing", map.display());
        return Ok(());
    }
    let mission = Mission::parse(&map)?;
    println!(
        "MISSION id={} title={:?} objectives={} cinematics={} triggers={} movies={:?}",
        mission.id,
        mission.title,
        mission.objectives.len(),
        mission.cinematics.len(),
        mission.triggers.len(),
        mission.movies
    );
    println!("beat totals: {:?}", beat_totals(&mission));
    for objective in &mission.objectives {
        println!(
            "  [{}] {} ({} step(s))",
            if objective.required { "required" } else { "optional" },
            objective.title,
            objective.items.len()
        );
        for item in &objective.items {
            println!("      - {}", item.text);
        }
    }
    if write {
        write_mission_json(&mission, &out_dir.join(format!("{mission_id}.json")))?;
    }

    // --- The live mission: boot it, tick it, capture the cutscene beats. ----
    let mut game = Game::new(Assets::load(None, Some(&map)), 0x5EED);
    game.enter_battle();
    let index = registry
        .index_of(&mission_id)
        .ok_or_else(|| format!("{mission_id} is not a campaign chapter"))?;
    game.start_campaign_mission(index)?;
    println!(
        "CAMPAIGN_MISSION idx={index} id={} human_slot={:?}",
        mission.id,
        game.campaign_human_slot()
    );
    for _ in 0..frames {
        game.update(TICK_SECONDS);
        game.campaign_frame(TICK_SECONDS);
    }
    let Some(view) = game.campaign_view() else {
        eprintln!("no campaign session after start");
        return Ok(());
    };
    println!(
        "CAMPAIGN_MISSION_STATE id={} script_booted={} time={:.2} victory={} defeat={} completed={}",
        view.id, view.script_booted, view.time, view.victory, view.defeat, view.completed
    );
    for line in &view.log {
        println!("{line}");
    }
    println!(
        "{}",
        game.campaign_cine_summary()
            .unwrap_or_else(|| "CAMPAIGN_CINE frames=0".to_string())
    );
    println!(
        "CAMPAIGN_CAMERA keyframes={} pose={:?} fade={:?} cutscene_active={}",
        view.keyframes,
        view.camera,
        game.screen_fade(),
        game.cutscene_active()
    );
    if let Some(director) = game.camera_director() {
        let keys = director.keyframes.clone();
        for pair in keys.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            let end = b.t + b.blend + 0.2;
            let step = director.max_step(a.t, end, 30.0);
            if step > 400.0 {
                let path = director.path(a.t, end, 30.0);
                let worst = path
                    .windows(2)
                    .map(|pair| (pair[0].0, pair[1].0, pair[0].1.target_distance(&pair[1].1), pair[0].1, pair[1].1))
                    .fold((0.0, 0.0, 0.0, wc3_game::campaign::CameraPose::default(), wc3_game::campaign::CameraPose::default()), |acc, row| if row.2 > acc.2 { row } else { acc });
                println!(
                    "CAMPAIGN_PATH_WORST t0={:.4} t1={:.4} step={:.0} p0=({:.0},{:.0}) p1=({:.0},{:.0})",
                    worst.0, worst.1, worst.2, worst.3.target[0], worst.3.target[1], worst.4.target[0], worst.4.target[1]
                );
                println!(
                    "CAMPAIGN_PATH_BAD from_t={:.2} to_t={:.2} max_step={step:.0} a=({:.0},{:.0}) b=({:.0},{:.0}) blend={:.2}",
                    a.t, b.t, a.pose.target[0], a.pose.target[1], b.pose.target[0], b.pose.target[1], b.blend
                );
            }
        }
        let mut previous: Option<&wc3_game::campaign::CameraKeyframe> = None;
        for key in &director.keyframes {
            if let Some(prev) = previous {
                let distance = prev.pose.target_distance(&key.pose);
                let step = if key.blend > 0.0 { distance / key.blend / 30.0 } else { distance };
                println!(
                    "CAMPAIGN_PATH t={:.2} from=({:.0},{:.0}) to=({:.0},{:.0}) distance={:.0} blend={:.2} step={:.0}",
                    key.t, prev.pose.target[0], prev.pose.target[1], key.pose.target[0], key.pose.target[1], distance, key.blend, step
                );
            }
            previous = Some(key);
        }
    }
    for row in &view.quests {
        println!(
            "QUEST_ROW required={} completed={} title={:?} items={}/{}",
            row.required,
            row.completed,
            row.title,
            row.items_done(),
            row.items.len()
        );
        for item in &row.items {
            println!("    [{}] {}", if item.completed { "x" } else { " " }, item.text);
        }
    }

    if write {
        let report = game
            .campaign_session_report()
            .expect("live session report");
        write_storyboard(
            &mission_id,
            &map,
            &report,
            &out_dir.join(format!("{mission_id}-storyboard.md")),
        )?;
    }
    Ok(())
}

fn write_mission_json(mission: &Mission, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&mission.to_json())?;
    std::fs::write(path, json)?;
    println!("wrote {}", path.display());
    Ok(())
}

fn write_storyboard(
    mission_id: &str,
    map: &Path,
    report: &MissionSessionReport,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut out = String::new();
    out.push_str(&format!("# {mission_id} — cutscene storyboard\n\n"));
    out.push_str(&format!(
        "Map `{}` — generated from the running runtime (`campaign_report --mission {mission_id} --frames {} --write`).\n\n",
        map.display(),
        report.frames
    ));
    out.push_str(&format!("`{}`\n\n", report.cine_summary));
    out.push_str(&format!(
        "Objectives: {} rows ({} required) · camera keyframes: {} · fades: {} · subtitles: {}\n\n",
        report.quests.len(),
        report.quests.iter().filter(|row| row.required).count(),
        report.keyframes.len(),
        report.fade_events.len(),
        report.subtitles.len()
    ));
    out.push_str(
        "Timeline note: the headless VM has no coroutine suspension, so the whole\n\
         intro runs inside one tick. Beats are stamped with the **cutscene clock**\n\
         (`t = max(event time, previous beat end)`) and each beat holds the cutscene\n\
         for the duration the script itself declares — a camera setup's blend, a\n\
         fade's duration, a line's transmission duration with a reading-time floor\n\
         when the voiceover length is not decoded. Order and durations are the\n\
         script's; only the clock origin is reconstructed.\n\n",
    );

    out.push_str("## Objectives\n\n| # | required | title | steps |\n|---|---|---|---|\n");
    for (index, row) in report.quests.iter().enumerate() {
        out.push_str(&format!(
            "| {} | {} | {} | {}/{} |\n",
            index + 1,
            if row.required { "yes" } else { "no" },
            row.title,
            row.items_done(),
            row.items.len()
        ));
    }
    out.push('\n');

    out.push_str("## Camera beats\n\n| t (s) | shot | target x,y | dist | yaw | pitch | z | blend (s) |\n|---|---|---|---|---|---|---|---|\n");
    for (index, key) in report.keyframes.iter().enumerate() {
        out.push_str(&format!(
            "| {:.2} | {} | {:.0}, {:.0} | {:.0} | {:.0}° | {:.0}° | {:.0} | {:.2} |\n",
            key.t,
            key.setup
                .map(|setup| format!("setup {setup}"))
                .unwrap_or_else(|| if index == 0 {
                    "opening".to_string()
                } else {
                    "pan".to_string()
                }),
            key.pose.target[0],
            key.pose.target[1],
            key.pose.distance,
            key.pose.yaw,
            key.pose.pitch,
            key.pose.z,
            key.blend
        ));
    }
    out.push('\n');

    out.push_str("## Fades\n\n| t (s) | duration | colour | alpha |\n|---|---|---|---|\n");
    for fade in &report.fade_events {
        out.push_str(&format!(
            "| {:.2} | {:.2} | #{:02X}{:02X}{:02X} | {:.2} → {:.2} |\n",
            fade.t,
            fade.duration,
            (fade.color[0] * 255.0).round().clamp(0.0, 255.0) as u8,
            (fade.color[1] * 255.0).round().clamp(0.0, 255.0) as u8,
            (fade.color[2] * 255.0).round().clamp(0.0, 255.0) as u8,
            fade.start_alpha,
            fade.end_alpha
        ));
    }
    if report.fade_events.is_empty() {
        out.push_str("| — | — | — | — |\n");
    }
    out.push('\n');

    out.push_str("## Subtitles\n\n| t (s) | speaker | text | shown (s) | portrait | kind |\n|---|---|---|---|---|---|\n");
    for line in &report.subtitles {
        out.push_str(&format!(
            "| {:.2} | {} | {} | {:.2} | {} | {} |\n",
            line.t,
            if line.speaker.is_empty() {
                "—"
            } else {
                &line.speaker
            },
            line.text.replace('|', "\\|").replace('\n', " "),
            line.shown_for,
            line.portrait
                .map(|handle| format!("unit {handle}"))
                .unwrap_or_else(|| "—".to_string()),
            line.kind.as_str()
        ));
    }
    if report.subtitles.is_empty() {
        out.push_str("| — | — | — | — | — | — |\n");
    }
    out.push('\n');

    out.push_str("## Hooks the screens lane needs\n\n");
    out.push_str("* `Game::camera_director() -> Option<CameraDirector>` — the keyframe stream (`CameraKeyframe { t, pose, blend, setup }`).\n");
    out.push_str("* `Game::cutscene_camera() -> Option<CameraPose>` — pose at the current cutscene-clock time.\n");
    out.push_str("* `Game::screen_fade() -> Option<(f64 alpha, [f64;3] colour)>` — the `CinematicFadeBJ` tint.\n");
    out.push_str("* `Game::subtitle_now() -> Option<SubtitleLine>` — speaker, text, duration, portrait unit handle.\n");
    out.push_str("* `Game::quest_rows() -> Vec<QuestRow>` — the objectives panel.\n");
    out.push_str("* `Game::cutscene_active() -> bool` — the script holds the camera away from gameplay.\n");
    out.push_str("* `Game::campaign_frame(dt)` — one call per frame; syncs quests/camera/subtitles and starts a requested movie.\n");
    out.push_str("* Letterbox: draw two bars of `0.12 * viewport.y` while `cutscene_active()`, and suppress the command card + minimap with them.\n");

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, out)?;
    println!("wrote {}", path.display());
    Ok(())
}
