//! Day/night curve and clock evidence at the crate boundary.
//!
//! Test ids: `render.daynight.curve`, `render.daynight.clock`.
use wc3_render::daynight::{
    DEFAULT_DAY_LENGTH_SECONDS, DayNightClock, DayNightCurve, FogSettings, MapLighting, Phase,
    Preset, is_night, phase,
};

/// test id: render.daynight.curve
#[test]
fn render_daynight_curve() {
    let curve = DayNightCurve::default();
    // Dawn: 04:00 -> 05:30 brightens. The DNC ramp is 05:20..06:00 and at
    // 06:00 the table is already at the noon key, so the sample stops at the
    // middle of the ramp (05:30) — see analysis/agent-re/render/DAYNIGHT-471.md.
    let dawn: Vec<f32> = [4.0, 4.5, 5.0, 5.33, 5.5]
        .iter()
        .map(|h| curve.sample(*h).terrain.luminance)
        .collect();
    for pair in dawn.windows(2) {
        assert!(pair[1] > pair[0], "dawn brightens: {dawn:?}");
    }
    // Dusk: 18:00 -> 24:00 darkens below the noon level.
    let dusk: Vec<f32> = (18..=23)
        .map(|h| curve.sample(h as f32).terrain.luminance)
        .collect();
    for pair in dusk.windows(2) {
        assert!(pair[1] < pair[0], "dusk darkens: {dusk:?}");
    }
    let noon = curve.sample(12.0).terrain.luminance;
    let midnight = curve.sample(0.0).terrain.luminance;
    let last_dawn = *dawn.last().unwrap();
    assert!(
        noon > last_dawn && last_dawn > midnight,
        "noon > dawn > midnight"
    );
    assert!(midnight > 0.05 && midnight > noon * 0.2, "night floor");
    // Unit DNC has the stronger ambient (0.3 vs 0.2).
    assert!(curve.sample(0.0).unit.luminance > midnight);
    // Presets really differ (Ashenvale/Felwood are not Lordaeron copies).
    let ash = DayNightCurve::new(Preset::Ashenvale);
    assert!(
        (ash.sample(0.0).terrain.luminance - midnight).abs() > 0.01,
        "Ashenvale night must differ from Lordaeron"
    );
    // Map override: w3i fog replaces the renderer default base colour.
    let mapped = DayNightCurve::with_fog(
        Preset::Lordaeron,
        FogSettings {
            base_color: [0.2, 0.1, 0.05],
            density: 0.5,
            start: 900.0,
            end: 4000.0,
        },
    );
    let sample = mapped.sample(12.0);
    assert!((sample.fog_density - 0.5).abs() < 1e-6);
    assert!(sample.fog_color[0] > curve.sample(12.0).fog_color[0]);
}

/// test id: render.daynight.clock
#[test]
fn render_daynight_clock() {
    let mut clock = DayNightClock::new();
    assert!((clock.seconds_per_day() - DEFAULT_DAY_LENGTH_SECONDS).abs() < 1e-3);
    clock.advance_ticks(30 * 160); // 160 s at 30 Hz = 8 game hours
    assert!((clock.time_of_day() - 8.0).abs() < 1e-3);
    assert_eq!(clock.day(), 1);
    clock.advance_seconds(320.0);
    assert!((clock.time_of_day() - 0.0).abs() < 1e-2);
    assert_eq!(clock.day(), 2);
    // 8 game hours per 8 configured real minutes.
    let mut configured = DayNightClock::new().with_hours_per_real_minutes(8.0, 8.0);
    configured.advance_seconds(8.0 * 60.0);
    assert!((configured.time_of_day() - 8.0).abs() < 1e-3);
    // Tick helper (sim-side) matches.
    let clock = DayNightClock::new();
    assert!((clock.time_of_day_at_tick(30 * 160) - 8.0).abs() < 1e-3);
    assert_eq!(clock.day_at_tick(30 * 480), 2);
    // Phase/audio flag.
    assert!(!is_night(12.0) && is_night(0.0));
    assert_eq!(phase(5.5), Phase::Dawn);
    assert_eq!(phase(18.5), Phase::Dusk);
}

#[test]
fn w3i_lighting_override_parses() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/MapFiles/DefaultMapFiles/war3map.w3i",
    );
    let Ok(bytes) = std::fs::read(path) else {
        eprintln!("skipping: w3i fixture missing");
        return;
    };
    let map = MapLighting::from_w3i(&bytes).expect("parse w3i");
    assert_eq!(map.version, 28);
    assert!(map.light_environment.is_some());
}
