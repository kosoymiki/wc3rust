//! WC3 day/night lighting curve, extracted from the shipped DNC light tables.
//!
//! # Where the data comes from
//!
//! Warcraft 3 does not tint the world with a hand-written grade. It instantiates
//! two MDX models per tileset — the *terrain* DNC and the *unit* DNC — and lets
//! their animated directional light drive the whole frame:
//!
//! * `UI/WorldEditData.txt` `[TerrainLights]` / `[UnitLights]` map every tileset
//!   letter to a model (`L=Environment\DNC\DNCLordaeron\DNCLordaeronTerrain\…`).
//!   Letters without a row fall back to Lordaeron (comment in the table).
//! * `Scripts/common.j`: `SetDayNightModels(terrainDNCFile, unitDNCFile)` swaps
//!   them at runtime, which is why units and terrain can have different ambient
//!   intensities.
//! * Each DNC model owns one directional light `FDirectSun`: static attenuation
//!   `(80, 200)`, light intensity `1.3`, ambient intensity `0.2` (terrain) /
//!   `0.3` (unit), a single `KGRT` rotation, an animated light colour (`KLAC`)
//!   and an animated ambient colour (`KLBC`).
//! * The `Stand` sequence is 60 000 ms (`SEQS` 333…60333) and covers one 24 h
//!   game day. `time_of_day / 24` is the sequence ratio (Warsmash
//!   `War3MapViewer.java:1793-1807`, `CSimulation.getGameTimeOfDay()`).
//!
//! The tables below are generated from the retail models by
//! `analysis/agent-re/render/daynight_extract.py --rust`; the `daynight`
//! test re-parses the shipped MDX files and asserts every keyframe, intensity
//! and direction matches, so the embedded copy cannot silently drift.
//!
//! **`Environment.slk` / `Lighting.slk` do not exist in this install.** They are
//! Reforged files; the 1.26 TFT data (War3x.mpq) ships the DNC MDX models
//! instead. `war3map.w3i` has no day/night length (that is a gameplay constant,
//! `Units/MiscData.txt:63-68`: `DayLength=480`, `Dawn=6`, `Dusk=18`,
//! `DayHours=24`); it does carry the map's fog and light-environment override,
//! which [`MapLighting::from_w3i`] reads.
//!
//! # Frame order
//!
//! The day/night grade is applied *inside* the world pass (terrain, water and
//! models — units read the unit DNC, terrain/water the terrain DNC). The
//! fog-of-war overlay is recorded **after** the world pass and **before** the
//! UI: it multiplies its own black veil over the already-graded world, so night
//! never brightens unexplored ground and the veil never gets a second daylight
//! tint. The HUD is ungraded (WC3 keeps the interface lit at night).
//!
//! # Disabling
//!
//! `H2_DAYNIGHT=off` (or `0`) pins the clock at noon for A/B captures.

pub const DAY_HOURS: f32 = 24.0;
/// `Units/MiscData.txt:63` — real seconds for a full 24 game-hour day.
pub const DEFAULT_DAY_LENGTH_SECONDS: f32 = 480.0;
/// `Units/MiscData.txt:64` — the hour the day starts.
pub const DEFAULT_DAWN_HOUR: f32 = 6.0;
/// `Units/MiscData.txt:65` — the hour the night starts.
pub const DEFAULT_DUSK_HOUR: f32 = 18.0;
/// `wc3_sim::TICKS_PER_SECOND` (30 Hz melee tick).
pub const SIM_TICKS_PER_SECOND: f32 = 30.0;
/// DNC `Stand` sequence length in milliseconds (`SEQS` 333…60333).
pub const DNC_SEQUENCE_MS: f32 = 60_000.0;
/// `N · L` of a flat, up-facing surface with the shipped `KGRT` direction
/// (`sun_direction.y = 0.615662`). The DNC constants are calibrated so that
/// `1.3 * FLAT_NDL + 0.2 * 0.956 ≈ 1.0` at noon: a flat surface is fully lit at
/// midday and the night keeps the same ratio the model was authored with.
pub const FLAT_NDL: f32 = 0.615662;

/// One key of a DNC colour track: `at` is the day fraction in `0..=1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key {
    pub at: f32,
    pub rgb: [f32; 3],
}

/// A DNC light as embedded in the binary (`&'static` tracks).
#[derive(Debug)]
pub struct DncLight {
    /// Node name in the model (`FDirectSun` in every shipped DNC model).
    pub model: &'static str,
    /// 0 omnidirectional, 1 directional, 2 ambient.
    pub light_type: u32,
    /// `(start, end)` attenuation in world units.
    pub attenuation: [f32; 2],
    /// Static light colour (the animated `KLAC` track multiplies it).
    pub sun_color: [f32; 3],
    /// Static light intensity (1.3 in every retail DNC model).
    pub sun_intensity: f32,
    /// Static ambient colour (the animated `KLBC` track multiplies it).
    pub ambient_color: [f32; 3],
    /// Terrain `0.2`, unit `0.3` (Ashenvale unit `0.5`, Dalaran unit `0.7`).
    pub ambient_intensity: f32,
    /// Unit vector **towards** the light, Y-up, derived from `KGRT`
    /// (`rotate(quat, +Z)`, MDX Z-up -> renderer Y-up, `+Y` up).
    pub sun_direction: [f32; 3],
    /// `KLAC` light-colour track.
    pub sun_keys: &'static [Key],
    /// `KLBC` ambient-colour track.
    pub ambient_keys: &'static [Key],
}

/// Owned copy of [`DncLight`], produced by [`parse_dnc_lights`].
#[derive(Clone, Debug, PartialEq)]
pub struct DncLightData {
    pub model: String,
    pub light_type: u32,
    pub attenuation: [f32; 2],
    pub sun_color: [f32; 3],
    pub sun_intensity: f32,
    pub ambient_color: [f32; 3],
    pub ambient_intensity: f32,
    pub sun_direction: [f32; 3],
    pub sun_keys: Vec<Key>,
    pub ambient_keys: Vec<Key>,
}

impl DncLight {
    /// Owned copy for comparison/logging.
    pub fn to_data(&self) -> DncLightData {
        DncLightData {
            model: self.model.to_string(),
            light_type: self.light_type,
            attenuation: self.attenuation,
            sun_color: self.sun_color,
            sun_intensity: self.sun_intensity,
            ambient_color: self.ambient_color,
            ambient_intensity: self.ambient_intensity,
            sun_direction: self.sun_direction,
            sun_keys: self.sun_keys.to_vec(),
            ambient_keys: self.ambient_keys.to_vec(),
        }
    }
}

/// The six distinct DNC presets shipped in the install.
///
/// `WorldEditData.txt` aliases the remaining tileset letters:
/// `F, W, B, N, Y, V, Q` -> Lordaeron (the hard-coded default).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Lordaeron,
    Ashenvale,
    Felwood,
    Dalaran,
    Dungeon,
    Underground,
}

impl Preset {
    /// Resolve a W3E / w3i tileset letter (`'L'`, `'A'`, …).
    pub fn for_tileset(letter: char) -> Self {
        match letter.to_ascii_uppercase() {
            'A' => Self::Ashenvale,
            'C' => Self::Felwood,
            'X' => Self::Dalaran,
            'D' => Self::Dungeon,
            'G' => Self::Underground,
            // L (Lordaeron summer), F/W/B/N/Y/V/Q: the table's documented
            // fallback is the Lordaeron model.
            _ => Self::Lordaeron,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Lordaeron => "lordaeron",
            Self::Ashenvale => "ashenvale",
            Self::Felwood => "felwood",
            Self::Dalaran => "dalaran",
            Self::Dungeon => "dungeon",
            Self::Underground => "underground",
        }
    }

    pub fn terrain(self) -> &'static DncLight {
        match self {
            Self::Lordaeron => &tables::LORDAERON_TERRAIN,
            Self::Ashenvale => &tables::ASHENVALE_TERRAIN,
            Self::Felwood => &tables::FELWOOD_TERRAIN,
            Self::Dalaran => &tables::DALARAN_TERRAIN,
            Self::Dungeon => &tables::DUNGEON_TERRAIN,
            Self::Underground => &tables::UNDERGROUND_TERRAIN,
        }
    }

    pub fn unit(self) -> &'static DncLight {
        match self {
            Self::Lordaeron => &tables::LORDAERON_UNIT,
            Self::Ashenvale => &tables::ASHENVALE_UNIT,
            Self::Felwood => &tables::FELWOOD_UNIT,
            Self::Dalaran => &tables::DALARAN_UNIT,
            Self::Dungeon => &tables::DUNGEON_UNIT,
            Self::Underground => &tables::UNDERGROUND_UNIT,
        }
    }
}

/// Linear-space fog/sky grading. `base_color` is the renderer's fog colour at
/// noon; density comes from `war3map.w3i` (0 when the map has no fog, which is
/// the case for the RoC campaign maps).
///
/// `start`/`end` are the map's fog distances in world units (the `w3i` fog
/// block stores them next to the colour and density). The terrain and model
/// shaders apply the haze as
/// `t = saturate((distance - start) / max(end - start, 1)) * density`,
/// `colour = mix(colour, fog_color, t)` — this is the WC3/Warcraft-3-style
/// linear distance fog, not an aerial-perspective curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FogSettings {
    pub base_color: [f32; 3],
    pub density: f32,
    /// Distance (world units) where the haze starts.
    pub start: f32,
    /// Distance (world units) where the haze reaches `density`.
    pub end: f32,
}

impl FogSettings {
    /// The renderer's pre-day/night sky colour (`render.rs` clear value), in
    /// linear space. Night scales it down from this noon value.
    pub const fn renderer_default() -> Self {
        Self {
            base_color: [0.045, 0.075, 0.11],
            density: 0.0,
            start: 0.0,
            end: 0.0,
        }
    }

    /// Default *atmospheric* haze for maps whose `war3map.w3i` carries no fog
    /// block (every RoC campaign map ships `density = 0`).
    ///
    /// WC3's campaign maps have no authored distance fog, but the renderer must
    /// still give the far terrain a horizon: without it the world ends in a hard
    /// cut against the clear colour and the night reads as a black screen
    /// (operator report 2026-11-17: «где туман???»). The ramp is in WC3 world
    /// units: the camera sits ~1.6–2.6k units from the target, so the haze
    /// starts just behind the play area and reaches [`ATMOSPHERE_DENSITY`] at
    /// the far edge of a 96x96 map. `base_color` is a light blue-grey in linear
    /// space; [`DayNightCurve::sample`] scales it with the hour so noon is a
    /// bright horizon and midnight is a dark blue one, never black.
    pub const fn atmosphere() -> Self {
        Self {
            base_color: [0.16, 0.23, 0.34],
            density: ATMOSPHERE_DENSITY,
            start: ATMOSPHERE_START,
            end: ATMOSPHERE_END,
        }
    }

    /// Atmosphere haze parameters with an explicit colour (the map's fog colour
    /// when it has one, [`Self::atmosphere`]'s blue-grey otherwise).
    pub const fn atmosphere_with(base_color: [f32; 3]) -> Self {
        Self {
            base_color,
            density: ATMOSPHERE_DENSITY,
            start: ATMOSPHERE_START,
            end: ATMOSPHERE_END,
        }
    }

    /// Clamp the parsed map row into the range the shader can use: a density of
    /// 0 disables the haze (maps without a fog block), and a degenerate
    /// `end <= start` falls back to a 0..4000 unit ramp.
    pub fn sanitized(self) -> Self {
        let density = if self.density.is_finite() {
            self.density.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let start = if self.start.is_finite() {
            self.start.max(0.0)
        } else {
            0.0
        };
        let end = if self.end.is_finite() && self.end > start {
            self.end
        } else {
            start + 4000.0
        };
        Self {
            base_color: [
                self.base_color[0].clamp(0.0, 1.0),
                self.base_color[1].clamp(0.0, 1.0),
                self.base_color[2].clamp(0.0, 1.0),
            ],
            density,
            start,
            end,
        }
    }
}

/// How much of the haze colour the far plane reaches (0..=1).
pub const ATMOSPHERE_DENSITY: f32 = 0.65;
/// Distance (world units) where the atmosphere starts to build up.
pub const ATMOSPHERE_START: f32 = 1100.0;
/// Distance (world units) where the atmosphere reaches its full density.
pub const ATMOSPHERE_END: f32 = 3800.0;

impl Default for FogSettings {
    fn default() -> Self {
        Self::renderer_default()
    }
}

/// Map lighting override parsed from `war3map.w3i` (v23+ carries fog; v25+
/// carries the light-environment tileset; older maps only have the terrain
/// tileset in `war3map.w3e`).
#[derive(Clone, Debug, PartialEq)]
pub struct MapLighting {
    pub version: u32,
    /// `LightEnvironment` tileset letter, when the format carries it.
    pub light_environment: Option<char>,
    /// Fog block, when the format carries one and it is not "off".
    pub fog: Option<FogSettings>,
}

impl MapLighting {
    /// Parse the header fields of `war3map.w3i` up to the light environment.
    ///
    /// Field order is the one War3Net's `MapInfo.ReadFrom` implements
    /// (`donors/War3Net/src/War3Net.Build.Core/Serialization/Binary/Info/MapInfo.cs:12-133`);
    /// this mirrors it for the v18…v31 range the install ships.
    pub fn from_w3i(bytes: &[u8]) -> Result<Self, String> {
        let mut r = Reader { bytes, pos: 0 };
        let version = r.u32()?;
        if version >= 18 {
            let _saves = r.u32()?;
            let _editor = r.u32()?;
            if version >= 27 {
                for _ in 0..4 {
                    let _ = r.u32()?;
                }
            }
        }
        for _ in 0..4 {
            let _ = r.cstr()?;
        }
        if version == 8 {
            for _ in 0..4 {
                let _ = r.u32()?;
            }
        }
        // Camera bounds: 4 floats (v8 also has complements, but those versions
        // predate the fog block).
        for _ in 0..4 {
            let _ = r.f32()?;
        }
        if version >= 15 {
            for _ in 0..4 {
                let _ = r.f32()?;
            }
        }
        let _width = r.u32()?;
        let _height = r.u32()?;
        if version == 8 {
            let _ = r.u32()?;
        }
        let _flags = r.u32()?;
        let _tileset = r.u8()?;
        if version >= 23 {
            let _loading_bg = r.u32()?;
            if version >= 39 {
                let _race_hud = r.u32()?;
            }
            let _loading_path = r.cstr()?;
        } else if version >= 18 {
            let _campaign_bg = r.u32()?;
        } else if version >= 15 {
            let _loading_path = r.cstr()?;
        }
        if version >= 10 {
            let _loading_text = r.cstr()?;
            let _loading_title = r.cstr()?;
            if version >= 15 {
                let _loading_subtitle = r.cstr()?;
            }
            if version >= 23 {
                let _game_data_set = r.u32()?;
                let _prologue_path = r.cstr()?;
            } else if version >= 18 {
                let _loading_number = r.u32()?;
            } else if version >= 15 {
                let _prologue_path = r.cstr()?;
            }
            if version >= 11 {
                let _prologue_text = r.cstr()?;
                let _prologue_title = r.cstr()?;
                if version >= 15 {
                    let _prologue_subtitle = r.cstr()?;
                }
            }
        }
        let mut fog = None;
        let mut light_environment = None;
        if version >= 23 {
            let style = r.u32()?;
            let start = r.f32()?;
            let end = r.f32()?;
            let density = r.f32()?;
            let color = r.bgra()?;
            if version >= 39 {
                for _ in 0..5 {
                    let _ = r.f32()?;
                }
                let _draw_over_sky = r.u32()?;
            }
            if version >= 25 {
                let _weather = r.u32()?;
            }
            let _sound_environment = r.cstr()?;
            let light = r.u8()?;
            let _water_tint = r.bgra()?;
            // Style 0 with density 0 is the editor's "no fog" state; keep the
            // row only when it actually fogs the world.
            if density > 0.0 {
                fog = Some(FogSettings {
                    base_color: color,
                    density,
                    start,
                    end,
                });
            }
            light_environment = Some(light as char);
        }
        Ok(Self {
            version,
            light_environment,
            fog,
        })
    }
}

/// Per-surface light for one instant of the day.
///
/// This is the WC3 fixed-function light model, split the way the shipped DNC
/// model splits it: one directional `FDirectSun` (light colour × light
/// intensity) plus one ambient term (ambient colour × ambient intensity). The
/// shaders evaluate
///
/// ```text
/// lit = clamp(ambient + sun * saturate(N · L), 0, 1)
/// ```
///
/// per channel, which is what D3D8 vertex lighting did (the clamp is part of
/// the fixed pipeline, not an artistic choice). The constants are tuned so a
/// flat, up-facing surface at noon lands on ~1.0:
/// `1.3 * 0.6157 + 0.2 * 0.956 ≈ 1.0` for Lordaeron terrain.
///
/// Earlier builds multiplied the *sum* `sun + ambient` into every pixel
/// (`grade`), which ignored `N · L` entirely: a warm sun then tinted the whole
/// frame uniformly and the night stayed at ~55 % of noon brightness with the
/// blue channel crushed — the "yellow filter" the operator reported. `grade` is
/// kept as the flat-surface equivalent of the correct model (`N · L = 0.6157`)
/// for callers that only know one colour, but the renderer uses `sun` and
/// `ambient` separately.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceLight {
    /// Light colour × intensity, linear space.
    pub sun: [f32; 3],
    /// Ambient colour × ambient intensity, linear space.
    pub ambient: [f32; 3],
    /// `[r, g, b, 1]`: the flat-surface (up-facing) light of this instant,
    /// clamped like the shader does. `sun + ambient` stays available for
    /// tests/logging, and `luminance` is the unclamped sum's luminance.
    pub grade: [f32; 4],
    /// Luminance of `sun + ambient` (linear), for tests and logging.
    pub luminance: f32,
}

impl SurfaceLight {
    /// `[r, g, b, w]` for the camera's `sun` uniform: the directional colour
    /// (linear, colour × intensity) in `rgb` and the ambient intensity's
    /// companion scale in `w` (always 1 — kept for a future exposure control).
    pub fn sun_uniform(&self) -> [f32; 4] {
        [self.sun[0], self.sun[1], self.sun[2], 1.0]
    }

    /// `[r, g, b, w]` for the camera's `ambient` uniform.
    pub fn ambient_uniform(&self) -> [f32; 4] {
        [self.ambient[0], self.ambient[1], self.ambient[2], 0.0]
    }

    /// Light for a surface whose normal faces the sun at `ndl = max(N·L, 0)`,
    /// clamped to `[0, 1]` per channel like the fixed-function pipeline.
    pub fn lit(&self, ndl: f32) -> [f32; 3] {
        let ndl = ndl.clamp(0.0, 1.0);
        [
            (self.ambient[0] + self.sun[0] * ndl).clamp(0.0, 1.0),
            (self.ambient[1] + self.sun[1] * ndl).clamp(0.0, 1.0),
            (self.ambient[2] + self.sun[2] * ndl).clamp(0.0, 1.0),
        ]
    }
}

/// Everything one frame needs from the curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DayNightSample {
    /// Clock hour in `0..24`.
    pub hour: f32,
    /// Terrain DNC (`[TerrainLights]`): terrain and water.
    pub terrain: SurfaceLight,
    /// Unit DNC (`[UnitLights]`): models and doodads.
    pub unit: SurfaceLight,
    /// Direction **towards** the sun (Y-up).
    pub sun_direction: [f32; 3],
    /// Fog/sky colour, linear space.
    pub fog_color: [f32; 3],
    /// Map fog density (0 when the map has none).
    pub fog_density: f32,
    /// Map fog distance ramp in world units (`start`, `end`).
    pub fog_range: [f32; 2],
    /// WC3 `daytime` flag (`Dawn <= hour < Dusk`).
    pub is_night: bool,
}

impl DayNightSample {
    /// `GAME_LIGHT hour=… ambient=… sun_dir=… fog=…` — one line per game hour,
/// written by the renderer so a device log can be checked against the curve.
    pub fn light_line(&self) -> String {
        format!(
            "GAME_LIGHT hour={:05.2} ambient={:.3},{:.3},{:.3} sun={:.3},{:.3},{:.3} \
             sun_dir={:.3},{:.3},{:.3} fog={:.3},{:.3},{:.3} fog_density={:.3} \
             fog_range={:.0},{:.0} night={}",
            self.hour,
            self.terrain.ambient[0],
            self.terrain.ambient[1],
            self.terrain.ambient[2],
            self.terrain.sun[0],
            self.terrain.sun[1],
            self.terrain.sun[2],
            self.sun_direction[0],
            self.sun_direction[1],
            self.sun_direction[2],
            self.fog_color[0],
            self.fog_color[1],
            self.fog_color[2],
            self.fog_density,
            self.fog_range[0],
            self.fog_range[1],
            self.is_night as u8
        )
    }
}

/// The "ungraded" reference frame: flat white light, no fog tint beyond the
/// renderer default. `H2_DAYNIGHT=identity` and the `render.color.no-yellow-cast`
/// test use it to separate the DNC grade from the material/texture colour.
pub fn neutral_sample(hours: f32) -> DayNightSample {
    let hour = hours.rem_euclid(DAY_HOURS);
    let flat = SurfaceLight {
        sun: [0.0; 3],
        ambient: [1.0; 3],
        grade: [1.0, 1.0, 1.0, 1.0],
        luminance: 1.0,
    };
    DayNightSample {
        hour,
        terrain: flat,
        unit: flat,
        sun_direction: Preset::Lordaeron.terrain().sun_direction,
        fog_color: FogSettings::renderer_default().base_color,
        fog_density: 0.0,
        fog_range: [0.0, 4000.0],
        is_night: is_night(hour),
    }
}

/// The curve: one preset + the map's fog override.
#[derive(Clone, Copy, Debug)]
pub struct DayNightCurve {
    preset: Preset,
    fog: FogSettings,
    /// Apply the default atmospheric haze when the map has no fog block.
    atmosphere: bool,
    /// Per-channel gain that makes the noon flat-surface light exactly white.
    ///
    /// The shipped DNC noon is *not* neutral: `KLBC` is
    /// `(0.980, 0.840, 0.840)` sRGB, so the raw flat light is
    /// `(0.985, 0.928, 0.928)` linear — a pink cast that reads as the
    /// operator's «жуткий жёлтый фильтр» once the terrain lamp multiplies it.
    /// Normalising the noon light to white removes the constant cast and leaves
    /// the hour-to-hour ratio untouched: 18:00 stays warm because its ratio
    /// differs from noon, midnight stays warm-dark, and noon is neutral by
    /// construction.
    white_balance: [f32; 3],
    /// Max component of the white-balanced `sun + ambient` at noon.
    noon_max: f32,
    /// Luminance of the white-balanced flat light at noon (1.0 by construction).
    noon_luminance: f32,
}

impl DayNightCurve {
    pub fn new(preset: Preset) -> Self {
        Self::with_fog(preset, FogSettings::default())
    }

    pub fn with_fog(preset: Preset, fog: FogSettings) -> Self {
        let fog = fog.sanitized();
        let mut curve = Self {
            preset,
            fog,
            atmosphere: false,
            white_balance: [1.0; 3],
            noon_max: 1.0,
            noon_luminance: 1.0,
        };
        // White balance: neutralise the *noon* flat light per channel. The DNC
        // data stays untouched, so every other hour keeps its authored ratio.
        let (sun, ambient) = curve.raw_light(preset.terrain(), 12.0 / DAY_HOURS);
        let flat = flat_light(sun, ambient);
        curve.white_balance = [
            (1.0 / flat[0].max(1e-3)).clamp(0.5, 2.0),
            (1.0 / flat[1].max(1e-3)).clamp(0.5, 2.0),
            (1.0 / flat[2].max(1e-3)).clamp(0.5, 2.0),
        ];
        let (sun, ambient) = curve.raw_light(preset.terrain(), 12.0 / DAY_HOURS);
        let total = add3(sun, ambient);
        curve.noon_max = total.iter().copied().fold(0.0f32, f32::max).max(1e-4);
        curve.noon_luminance = luminance(flat_light(sun, ambient)).max(1e-4);
        curve
    }

    /// Build from the preset resolved from a `war3map.w3i` light environment
    /// (falls back to Lordaeron when the map is too old to carry one).
    pub fn from_map(map: &MapLighting) -> Self {
        let preset = map
            .light_environment
            .map(Preset::for_tileset)
            .unwrap_or(Preset::Lordaeron);
        Self::with_fog(preset, map.fog.unwrap_or_default())
    }

    pub fn preset(&self) -> Preset {
        self.preset
    }

    pub fn set_preset(&mut self, preset: Preset) {
        let atmosphere = self.atmosphere;
        *self = Self::with_fog(preset, self.fog);
        self.atmosphere = atmosphere;
    }

    pub fn fog(&self) -> FogSettings {
        self.fog
    }

    pub fn set_fog(&mut self, fog: FogSettings) {
        let atmosphere = self.atmosphere;
        *self = Self::with_fog(self.preset, fog);
        self.atmosphere = atmosphere;
    }

    /// Enable/disable the default atmospheric haze used when the map carries no
    /// fog block. The game renderer enables it (WC3 look); pure-data users of
    /// the curve keep the map's exact settings.
    pub fn set_atmosphere(&mut self, enabled: bool) {
        self.atmosphere = enabled;
    }

    pub fn atmosphere(&self) -> bool {
        self.atmosphere
    }

    /// The haze actually in force this frame.
    ///
    /// The map's `war3map.w3i` fog block wins when it has one (`density > 0`).
    /// Maps without a block (every RoC campaign map) get
    /// [`FogSettings::atmosphere`] so the far terrain still has a horizon; the
    /// map's own `base_color` is reused when the w3i carried a colour but
    /// density 0. `atmosphere = false` returns the raw block.
    pub fn fog_effective(&self) -> FogSettings {
        if self.fog.density > 0.0 {
            self.fog
        } else if self.atmosphere {
            FogSettings::atmosphere_with(self.fog.base_color)
                .sanitized()
        } else {
            self.fog
        }
    }

    /// Sample the curve at a clock hour (`0..24`, wrapped).
    pub fn sample(&self, hours: f32) -> DayNightSample {
        let hour = hours.rem_euclid(DAY_HOURS);
        let at = hour / DAY_HOURS;
        let terrain = self.surface(self.preset.terrain(), at);
        let unit = self.surface(self.preset.unit(), at);
        // Sky/fog follows the flat-surface light level, floored so night keeps a
        // readable horizon instead of a black screen.
        let day_factor = (luminance(terrain.lit(FLAT_NDL)) / self.noon_luminance).clamp(0.0, 1.0);
        let fog_scale = 0.22 + 0.78 * day_factor;
        let fog = self.fog_effective();
        let fog_color = [
            fog.base_color[0] * fog_scale,
            fog.base_color[1] * fog_scale,
            fog.base_color[2] * fog_scale,
        ];
        DayNightSample {
            hour,
            terrain,
            unit,
            sun_direction: self.preset.terrain().sun_direction,
            fog_color,
            fog_density: fog.density,
            fog_range: [fog.start, fog.end],
            is_night: is_night(hour),
        }
    }

    /// One surface light at day fraction `at`.
    fn surface(&self, light: &DncLight, at: f32) -> SurfaceLight {
        let (sun, ambient) = self.raw_light(light, at);
        let total = add3(sun, ambient);
        let flat = flat_light(sun, ambient);
        SurfaceLight {
            sun,
            ambient,
            grade: [flat[0], flat[1], flat[2], 1.0],
            luminance: luminance(total),
        }
    }

    /// Linear sun/ambient for one DNC light at day fraction `at`.
    ///
    /// The MDX colour tracks are authored in sRGB (they are keyframed colours,
    /// not radiometric intensities), so they are decoded once here and then
    /// multiplied by the curve's white-balance gain: the shader consumes linear
    /// light and the target is an sRGB swapchain. Applying the tint *after* the
    /// decode is what keeps noon white; multiplying sRGB values into a linear
    /// buffer (the old path) shifted every midtone warm.
    fn raw_light(&self, light: &DncLight, at: f32) -> ([f32; 3], [f32; 3]) {
        let sun_srgb = mul3(light.sun_color, track_rgb(light.sun_keys, at));
        let ambient_srgb = mul3(light.ambient_color, track_rgb(light.ambient_keys, at));
        let wb = self.white_balance;
        (
            scale3(mul3(srgb_to_linear3(sun_srgb), wb), light.sun_intensity),
            scale3(mul3(srgb_to_linear3(ambient_srgb), wb), light.ambient_intensity),
        )
    }
}

impl Default for DayNightCurve {
    fn default() -> Self {
        Self::new(Preset::Lordaeron)
    }
}

/// WC3 `daytime` flag: day is `[Dawn, Dusk)`.
pub fn is_night(hours: f32) -> bool {
    let hour = hours.rem_euclid(DAY_HOURS);
    !(DEFAULT_DAWN_HOUR..DEFAULT_DUSK_HOUR).contains(&hour)
}

/// Coarse phase label (for logs, audio and evidence).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Night,
    Dawn,
    Day,
    Dusk,
}

/// Dawn/dusk are the one-hour windows the DNC tracks spend ramping; the
/// interior is full day/night.
pub fn phase(hours: f32) -> Phase {
    let hour = hours.rem_euclid(DAY_HOURS);
    if (DEFAULT_DAWN_HOUR - 1.0..DEFAULT_DAWN_HOUR).contains(&hour) {
        Phase::Dawn
    } else if (DEFAULT_DAWN_HOUR..DEFAULT_DUSK_HOUR).contains(&hour) {
        Phase::Day
    } else if (DEFAULT_DUSK_HOUR..DEFAULT_DUSK_HOUR + 1.0).contains(&hour) {
        Phase::Dusk
    } else {
        Phase::Night
    }
}

/// Game-day clock. Defaults to WC3's `DayLength=480` seconds for 24 hours
/// (8 real minutes), i.e. 3 game hours per real minute.
#[derive(Clone, Copy, Debug)]
pub struct DayNightClock {
    seconds_per_day: f32,
    scale: f32,
    elapsed: f32,
}

impl DayNightClock {
    pub fn new() -> Self {
        Self {
            seconds_per_day: DEFAULT_DAY_LENGTH_SECONDS,
            scale: 1.0,
            elapsed: 0.0,
        }
    }

    /// Override the full-day real duration (map/gameplay-constant override).
    pub fn with_seconds_per_day(mut self, seconds: f32) -> Self {
        self.seconds_per_day = seconds.max(1e-3);
        self
    }

    /// Configure "`hours` game hours per `minutes` real minutes".
    pub fn with_hours_per_real_minutes(mut self, hours: f32, minutes: f32) -> Self {
        let per_minute = (hours / minutes.max(1e-6)).abs();
        self.seconds_per_day = (DAY_HOURS / per_minute.max(1e-6)) * 60.0;
        self
    }

    /// Jass `SetTimeOfDayScale` (1.0 = normal).
    pub fn with_scale(mut self, scale: f32) -> Self {
        self.scale = scale.max(0.0);
        self
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale.max(0.0);
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn seconds_per_day(&self) -> f32 {
        self.seconds_per_day
    }

    pub fn advance_seconds(&mut self, seconds: f32) {
        if seconds.is_finite() && seconds > 0.0 {
            self.elapsed += seconds * self.scale;
        }
    }

    /// Advance by simulation ticks (the authoritative RTS clock).
    pub fn advance_ticks(&mut self, ticks: u64) {
        self.advance_seconds(ticks as f32 / SIM_TICKS_PER_SECOND);
    }

    /// Time of day for an absolute simulation tick, without mutating the clock.
    pub fn time_of_day_at_tick(&self, tick: u64) -> f32 {
        let seconds = tick as f32 / SIM_TICKS_PER_SECOND;
        (seconds / self.seconds_per_day * DAY_HOURS).rem_euclid(DAY_HOURS)
    }

    /// 1-based game day (`day=1` on the first day).
    pub fn day_at_tick(&self, tick: u64) -> u32 {
        let seconds = tick as f32 / SIM_TICKS_PER_SECOND;
        1 + (seconds / self.seconds_per_day).floor().max(0.0) as u32
    }

    pub fn time_of_day(&self) -> f32 {
        (self.elapsed / self.seconds_per_day * DAY_HOURS).rem_euclid(DAY_HOURS)
    }

    pub fn day(&self) -> u32 {
        1 + (self.elapsed / self.seconds_per_day).floor().max(0.0) as u32
    }

    pub fn set_time_of_day(&mut self, hours: f32) {
        let hours = hours.rem_euclid(DAY_HOURS);
        self.elapsed = hours / DAY_HOURS * self.seconds_per_day;
    }
}

impl Default for DayNightClock {
    fn default() -> Self {
        Self::new()
    }
}

/// Day/night source of truth for one renderer.
///
/// The simulation owns the authoritative clock (`Sim::time_of_day`, derived
/// from the tick count). The renderer must never run a second, competing clock:
/// the game loop calls [`DayNightState::sync`] every frame with the simulation
/// hour and the renderer samples exactly that value. That is the fix for the
/// device symptom where `GAME_TIME hour=00.00` logged midnight while the frame
/// rendered at the renderer's own, unrelated hour.
///
/// [`DayNightState::release`] returns to the free-running clock for tools with
/// no simulation (map viewer, headless gallery, asset bench).
#[derive(Clone, Copy, Debug)]
pub struct DayNightState {
    clock: DayNightClock,
    synced: Option<f32>,
}

impl DayNightState {
    pub fn new() -> Self {
        Self {
            clock: DayNightClock::new(),
            synced: None,
        }
    }

    /// Follow the simulation hour (`0..24`, wrapped). Stored, never advanced:
    /// a 10 s hitch cannot move the sun.
    pub fn sync(&mut self, hours: f32) {
        let hours = hours.rem_euclid(DAY_HOURS);
        self.synced = Some(hours);
        self.clock.set_time_of_day(hours);
    }

    /// Hand the clock back to real time, starting from the last synced hour.
    pub fn release(&mut self) {
        self.synced = None;
        self.clock.set_time_of_day(self.clock.time_of_day());
    }

    /// The injected hour when synced.
    pub fn synced_hour(&self) -> Option<f32> {
        self.synced
    }

    /// Advance the free-running clock by `dt` real seconds. A synced state is
    /// never advanced — the simulation is the only clock then.
    pub fn advance_seconds(&mut self, dt: f32) {
        if self.synced.is_none() {
            self.clock.advance_seconds(dt);
        }
    }

    /// Clock hour in `0..24` (the synced value when synced).
    pub fn hour(&self) -> f32 {
        self.synced.unwrap_or_else(|| self.clock.time_of_day())
    }

    pub fn day(&self) -> u32 {
        self.clock.day()
    }

    pub fn clock(&self) -> &DayNightClock {
        &self.clock
    }

    pub fn clock_mut(&mut self) -> &mut DayNightClock {
        &mut self.clock
    }
}

impl Default for DayNightState {
    fn default() -> Self {
        Self::new()
    }
}

/// `H2_ATMOSPHERE=off|0` disables the default distance haze (maps without a
/// `war3map.w3i` fog block). A map's own fog block is never disabled by it.
pub fn atmosphere_enabled() -> bool {
    !matches!(
        std::env::var("H2_ATMOSPHERE"),
        Ok(value) if value.eq_ignore_ascii_case("off") || value == "0"
    )
}

/// `H2_DAYNIGHT=off|0` disables the cycle for A/B captures (pins noon).
pub fn enabled() -> bool {
    !matches!(
        std::env::var("H2_DAYNIGHT"),
        Ok(value) if value.eq_ignore_ascii_case("off") || value == "0"
    )
}

/// `H2_DAYNIGHT=identity` renders the **ungraded** reference frame (flat white
/// light, no DNC tint) while keeping the clock at the requested hour. It is the
/// A/B baseline for `render.color.no-yellow-cast`: the same camera, the same
/// materials, only the light differs.
pub fn identity_light() -> bool {
    matches!(
        std::env::var("H2_DAYNIGHT"),
        Ok(value) if value.eq_ignore_ascii_case("identity") || value.eq_ignore_ascii_case("neutral")
    )
}

/// `GAME_TIME day=… hour=…` payload. Kept here so the renderer and headless
/// tests agree on the formatting.
pub fn game_time_line(day: u32, hours: f32) -> String {
    format!(
        "GAME_TIME day={day} hour={:05.2}",
        hours.rem_euclid(DAY_HOURS)
    )
}

/// `GAME_LIGHT hour=12.00 night=false` — the day/night sync line.
///
/// Emitted once per game hour by the renderer next to `GAME_TIME`, so a device
/// log proves which hour the *grade* used (the operator's mismatch was
/// `GAME_TIME hour=00.00` while the frame was rendered at bright daylight).
pub fn game_light_line(hours: f32) -> String {
    let hour = hours.rem_euclid(DAY_HOURS);
    format!("GAME_LIGHT hour={hour:.2} night={}", is_night(hour))
}

// ---------------------------------------------------------------- helpers

fn track_rgb(keys: &[Key], at: f32) -> [f32; 3] {
    if keys.is_empty() {
        return [1.0; 3];
    }
    let at = at.rem_euclid(1.0);
    if at <= keys[0].at {
        return keys[0].rgb;
    }
    for pair in keys.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if at <= b.at {
            let span = (b.at - a.at).max(1e-6);
            let t = ((at - a.at) / span).clamp(0.0, 1.0);
            return [
                a.rgb[0] + (b.rgb[0] - a.rgb[0]) * t,
                a.rgb[1] + (b.rgb[1] - a.rgb[1]) * t,
                a.rgb[2] + (b.rgb[2] - a.rgb[2]) * t,
            ];
        }
    }
    keys[keys.len() - 1].rgb
}

fn srgb_to_linear(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn srgb_to_linear3(rgb: [f32; 3]) -> [f32; 3] {
    [
        srgb_to_linear(rgb[0]),
        srgb_to_linear(rgb[1]),
        srgb_to_linear(rgb[2]),
    ]
}

fn luminance(rgb: [f32; 3]) -> f32 {
    rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722
}

fn mul3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2]]
}

fn scale3(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn add3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// Flat, up-facing surface light (`N · L = FLAT_NDL`), clamped per channel the
/// way the fixed-function pipeline does.
fn flat_light(sun: [f32; 3], ambient: [f32; 3]) -> [f32; 3] {
    [
        (ambient[0] + sun[0] * FLAT_NDL).clamp(0.0, 1.0),
        (ambient[1] + sun[1] * FLAT_NDL).clamp(0.0, 1.0),
        (ambient[2] + sun[2] * FLAT_NDL).clamp(0.0, 1.0),
    ]
}

// ---------------------------------------------------------------- MDX parser

/// Parse the single `LITE` entry of a shipped DNC model.
///
/// v800 DNC entries nest the generic object behind its own inclusive size:
/// `entry_size, gen_size, name[80], object_id, parent_id, flags, node anims,
/// static light fields, light anims`. A unit model's `BONE`/`HELP` entries use
/// the flat layout; only `LITE` is accepted here.
pub fn parse_dnc_lights(bytes: &[u8]) -> Result<DncLightData, String> {
    if bytes.get(0..4) != Some(b"MDLX") {
        return Err("not an MDX file".into());
    }
    let mut pos = 4usize;
    let mut lite: Option<(usize, usize)> = None;
    let mut seqs: Option<(usize, usize)> = None;
    while pos + 8 <= bytes.len() {
        let tag = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let start = pos + 8;
        let end = start.checked_add(size).ok_or("chunk size overflow")?;
        if end > bytes.len() {
            return Err(format!("chunk {:?} exceeds file", tag));
        }
        match tag {
            b"LITE" => lite = Some((start, end)),
            b"SEQS" => seqs = Some((start, end)),
            _ => {}
        }
        pos = end;
    }
    let (start, end) = lite.ok_or("DNC model has no LITE chunk")?;
    let (seq_start, seq_end) = match seqs {
        Some((s, _)) if s + 88 <= bytes.len() => (
            u32::from_le_bytes(bytes[s + 80..s + 84].try_into().unwrap()) as f32,
            u32::from_le_bytes(bytes[s + 84..s + 88].try_into().unwrap()) as f32,
        ),
        _ => (0.0, DNC_SEQUENCE_MS),
    };
    let span = (seq_end - seq_start).max(1.0);
    let mut p = start;
    let entry = read_u32(bytes, &mut p)? as usize;
    if p + 4 > end {
        return Err("truncated light entry".into());
    }
    let gen_size = read_u32(bytes, &mut p)? as usize;
    let name = read_fixed_str(bytes, p, 80)?;
    p += 80;
    let _object_id = read_u32(bytes, &mut p)?;
    let _parent_id = read_i32(bytes, &mut p)?;
    let _flags = read_u32(bytes, &mut p)?;
    let gen_end = start + 4 + gen_size;
    let mut sun_direction = [0.0f32; 3];
    let mut node = Vec::new();
    while p < gen_end {
        let (tag, keys, next) = read_track(bytes, p)?;
        p = next;
        if tag == *b"KGRT" {
            if let Some((_, q)) = keys.first().filter(|(_, v)| v.len() == 4) {
                sun_direction = quat_towards_light(&[q[0], q[1], q[2], q[3]]);
            }
        }
        node.push((tag, keys));
    }
    let _ = entry;
    let light_type = read_u32(bytes, &mut p)?;
    let attenuation = [read_f32(bytes, &mut p)?, read_f32(bytes, &mut p)?];
    let sun_color = [
        read_f32(bytes, &mut p)?,
        read_f32(bytes, &mut p)?,
        read_f32(bytes, &mut p)?,
    ];
    let sun_intensity = read_f32(bytes, &mut p)?;
    let ambient_color = [
        read_f32(bytes, &mut p)?,
        read_f32(bytes, &mut p)?,
        read_f32(bytes, &mut p)?,
    ];
    let ambient_intensity = read_f32(bytes, &mut p)?;
    let mut sun_keys = Vec::new();
    let mut ambient_keys = Vec::new();
    while p + 4 <= end {
        let (tag, keys, next) = read_track(bytes, p)?;
        p = next;
        let mut normalized = |keys: Vec<(u32, Vec<f32>)>| -> Vec<Key> {
            keys.into_iter()
                .filter(|(_, v)| v.len() == 3)
                .map(|(t, v)| Key {
                    at: ((t as f32 - seq_start) / span).clamp(0.0, 1.0),
                    rgb: [v[0], v[1], v[2]],
                })
                .collect()
        };
        match &tag {
            b"KLAC" => sun_keys = normalized(keys),
            b"KLBC" => ambient_keys = normalized(keys),
            _ => {}
        }
    }
    Ok(DncLightData {
        model: name,
        light_type,
        attenuation,
        sun_color,
        sun_intensity,
        ambient_color,
        ambient_intensity,
        sun_direction,
        sun_keys,
        ambient_keys,
    })
}

fn quat_towards_light(q: &[f32; 4]) -> [f32; 3] {
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    // MDX is Z-up and the light travels along -Z of its node; +Z is the
    // direction to the light. Convert Z-up -> Y-up: (x, y, z) -> (x, z, -y).
    let v = (0.0f32, 0.0f32, 1.0f32);
    let tx = 2.0 * (y * v.2 - z * v.1);
    let ty = 2.0 * (z * v.0 - x * v.2);
    let tz = 2.0 * (x * v.1 - y * v.0);
    let rx = v.0 + w * tx + (y * tz - z * ty);
    let ry = v.1 + w * ty + (z * tx - x * tz);
    let rz = v.2 + w * tz + (x * ty - y * tx);
    let yup = [rx, rz, -ry];
    let len = (yup[0] * yup[0] + yup[1] * yup[1] + yup[2] * yup[2])
        .sqrt()
        .max(1e-6);
    let mut out = [yup[0] / len, yup[1] / len, yup[2] / len];
    if out[1] < 0.0 {
        out = [-out[0], -out[1], -out[2]];
    }
    out
}

fn read_track(
    bytes: &[u8],
    mut p: usize,
) -> Result<([u8; 4], Vec<(u32, Vec<f32>)>, usize), String> {
    if p + 16 > bytes.len() {
        return Err("truncated animation track".into());
    }
    let tag: [u8; 4] = bytes[p..p + 4].try_into().unwrap();
    let count = u32::from_le_bytes(bytes[p + 4..p + 8].try_into().unwrap()) as usize;
    let interp = i32::from_le_bytes(bytes[p + 8..p + 12].try_into().unwrap());
    p += 16;
    let width = match &tag {
        b"KLAC" | b"KLBC" | b"KGTR" | b"KGSC" => 3,
        b"KGRT" => 4,
        _ => 1,
    };
    if count > 100_000 {
        return Err("implausible track key count".into());
    }
    let mut keys = Vec::with_capacity(count);
    for _ in 0..count {
        if p + 4 + width * 4 > bytes.len() {
            return Err("truncated track key".into());
        }
        let time = u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap());
        p += 4;
        let mut value = Vec::with_capacity(width);
        for _ in 0..width {
            value.push(f32::from_le_bytes(bytes[p..p + 4].try_into().unwrap()));
            p += 4;
        }
        if interp > 1 {
            p += width * 4 * 2;
        }
        keys.push((time, value));
    }
    Ok((tag, keys, p))
}

fn read_u32(bytes: &[u8], p: &mut usize) -> Result<u32, String> {
    if *p + 4 > bytes.len() {
        return Err("truncated u32".into());
    }
    let value = u32::from_le_bytes(bytes[*p..*p + 4].try_into().unwrap());
    *p += 4;
    Ok(value)
}

fn read_i32(bytes: &[u8], p: &mut usize) -> Result<i32, String> {
    Ok(read_u32(bytes, p)? as i32)
}

fn read_f32(bytes: &[u8], p: &mut usize) -> Result<f32, String> {
    Ok(f32::from_bits(read_u32(bytes, p)?))
}

fn read_fixed_str(bytes: &[u8], at: usize, len: usize) -> Result<String, String> {
    let slice = bytes
        .get(at..at + len)
        .ok_or_else(|| "truncated fixed string".to_string())?;
    let end = slice.iter().position(|b| *b == 0).unwrap_or(len);
    Ok(String::from_utf8_lossy(&slice[..end]).into_owned())
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn u8(&mut self) -> Result<u8, String> {
        let value = *self.bytes.get(self.pos).ok_or("truncated w3i")?;
        self.pos += 1;
        Ok(value)
    }

    fn u32(&mut self) -> Result<u32, String> {
        let slice = self
            .bytes
            .get(self.pos..self.pos + 4)
            .ok_or("truncated w3i")?;
        self.pos += 4;
        Ok(u32::from_le_bytes(slice.try_into().unwrap()))
    }

    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_bits(self.u32()?))
    }

    fn cstr(&mut self) -> Result<String, String> {
        let rest = self.bytes.get(self.pos..).ok_or("truncated w3i")?;
        let end = rest
            .iter()
            .position(|b| *b == 0)
            .ok_or("unterminated w3i string")?;
        let text = String::from_utf8_lossy(&rest[..end]).into_owned();
        self.pos += end + 1;
        Ok(text)
    }

    /// BGRA byte order -> linear RGB (w3i colours are sRGB bytes).
    fn bgra(&mut self) -> Result<[f32; 3], String> {
        let slice = self
            .bytes
            .get(self.pos..self.pos + 4)
            .ok_or("truncated w3i colour")?;
        self.pos += 4;
        let b = slice[0] as f32 / 255.0;
        let g = slice[1] as f32 / 255.0;
        let r = slice[2] as f32 / 255.0;
        Ok([srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b)])
    }
}

mod tables {
    use super::{DncLight, Key};

    pub static LORDAERON_TERRAIN: DncLight = DncLight {
        model: "FDirectSun",
        light_type: 1,
        attenuation: [80.000000, 200.000000],
        sun_color: [1.000000, 1.000000, 1.000000],
        sun_intensity: 1.300000,
        ambient_color: [1.000000, 1.000000, 1.000000],
        ambient_intensity: 0.200000,
        sun_direction: [-0.675456, 0.615662, 0.405856],
        sun_keys: &[
            Key {
                at: 0.000000,
                rgb: [0.799191, 0.532794, 0.313408],
            },
            Key {
                at: 0.222233,
                rgb: [0.996078, 0.755867, 0.626817],
            },
            Key {
                at: 0.250000,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.250567,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.722783,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.750567,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.803900,
                rgb: [0.996078, 0.848251, 0.671882],
            },
            Key {
                at: 1.000000,
                rgb: [0.799191, 0.532794, 0.313408],
            },
        ],
        ambient_keys: &[
            Key {
                at: 0.000000,
                rgb: [0.799191, 0.532794, 0.313408],
            },
            Key {
                at: 0.223333,
                rgb: [0.965830, 0.522347, 0.467523],
            },
            Key {
                at: 0.250000,
                rgb: [0.980392, 0.839852, 0.839852],
            },
            Key {
                at: 0.250567,
                rgb: [0.980392, 0.839852, 0.839852],
            },
            Key {
                at: 0.722783,
                rgb: [0.980392, 0.839852, 0.839852],
            },
            Key {
                at: 0.751667,
                rgb: [0.902841, 0.522347, 0.636354],
            },
            Key {
                at: 0.773333,
                rgb: [0.902841, 0.522347, 0.369559],
            },
            Key {
                at: 0.800000,
                rgb: [0.965830, 0.522347, 0.261174],
            },
            Key {
                at: 1.000000,
                rgb: [0.799191, 0.532794, 0.313408],
            },
        ],
    };

    pub static LORDAERON_UNIT: DncLight = DncLight {
        model: "FDirectSun",
        light_type: 1,
        attenuation: [80.000000, 200.000000],
        sun_color: [1.000000, 1.000000, 1.000000],
        sun_intensity: 1.300000,
        ambient_color: [1.000000, 1.000000, 1.000000],
        ambient_intensity: 0.300000,
        sun_direction: [-0.675456, 0.615662, 0.405856],
        sun_keys: &[
            Key {
                at: 0.000000,
                rgb: [0.799191, 0.532794, 0.313408],
            },
            Key {
                at: 0.222233,
                rgb: [0.996078, 0.755867, 0.626817],
            },
            Key {
                at: 0.250000,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.250567,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.722783,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.750567,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.803900,
                rgb: [0.996078, 0.848251, 0.671882],
            },
            Key {
                at: 1.000000,
                rgb: [0.799191, 0.532794, 0.313408],
            },
        ],
        ambient_keys: &[
            Key {
                at: 0.000000,
                rgb: [0.799191, 0.532794, 0.313408],
            },
            Key {
                at: 0.223333,
                rgb: [0.965830, 0.522347, 0.467523],
            },
            Key {
                at: 0.250000,
                rgb: [0.980392, 0.839852, 0.839852],
            },
            Key {
                at: 0.250567,
                rgb: [0.980392, 0.839852, 0.839852],
            },
            Key {
                at: 0.722783,
                rgb: [0.980392, 0.839852, 0.839852],
            },
            Key {
                at: 0.751667,
                rgb: [0.902841, 0.522347, 0.636354],
            },
            Key {
                at: 0.773333,
                rgb: [0.902841, 0.522347, 0.369559],
            },
            Key {
                at: 0.800000,
                rgb: [0.965830, 0.522347, 0.261174],
            },
            Key {
                at: 1.000000,
                rgb: [0.799191, 0.532794, 0.313408],
            },
        ],
    };

    pub static ASHENVALE_TERRAIN: DncLight = DncLight {
        model: "FDirectSun",
        light_type: 1,
        attenuation: [80.000000, 200.000000],
        sun_color: [1.000000, 1.000000, 1.000000],
        sun_intensity: 1.300000,
        ambient_color: [1.000000, 1.000000, 1.000000],
        ambient_intensity: 0.300000,
        sun_direction: [-0.675456, 0.615662, 0.405856],
        sun_keys: &[
            Key {
                at: 0.000000,
                rgb: [0.600000, 0.400000, 0.235294],
            },
            Key {
                at: 0.222233,
                rgb: [0.996078, 0.705882, 0.470588],
            },
            Key {
                at: 0.250000,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.250567,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.722783,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.750567,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.803900,
                rgb: [0.996078, 0.792157, 0.627451],
            },
            Key {
                at: 1.000000,
                rgb: [0.600000, 0.400000, 0.235294],
            },
        ],
        ambient_keys: &[
            Key {
                at: 0.000000,
                rgb: [0.509804, 0.509804, 0.235294],
            },
            Key {
                at: 0.231117,
                rgb: [0.901961, 0.731347, 0.350997],
            },
            Key {
                at: 0.246667,
                rgb: [0.941177, 0.784314, 0.814924],
            },
            Key {
                at: 0.253333,
                rgb: [0.941177, 1.000000, 0.765979],
            },
            Key {
                at: 0.731117,
                rgb: [0.941177, 1.000000, 0.758340],
            },
            Key {
                at: 0.771667,
                rgb: [0.843137, 0.748154, 0.477748],
            },
            Key {
                at: 0.793333,
                rgb: [0.843137, 0.748154, 0.277450],
            },
            Key {
                at: 0.820000,
                rgb: [0.901961, 0.748154, 0.196078],
            },
            Key {
                at: 1.000000,
                rgb: [0.509804, 0.509804, 0.235294],
            },
        ],
    };

    pub static ASHENVALE_UNIT: DncLight = DncLight {
        model: "FDirectSun",
        light_type: 1,
        attenuation: [80.000000, 200.000000],
        sun_color: [1.000000, 1.000000, 1.000000],
        sun_intensity: 1.300000,
        ambient_color: [1.000000, 1.000000, 1.000000],
        ambient_intensity: 0.500000,
        sun_direction: [-0.675456, 0.615662, 0.405856],
        sun_keys: &[
            Key {
                at: 0.000000,
                rgb: [0.600000, 0.400000, 0.235294],
            },
            Key {
                at: 0.222233,
                rgb: [0.996078, 0.705882, 0.470588],
            },
            Key {
                at: 0.250000,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.250567,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.722783,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.750567,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.803900,
                rgb: [0.996078, 0.792157, 0.627451],
            },
            Key {
                at: 1.000000,
                rgb: [0.600000, 0.400000, 0.235294],
            },
        ],
        ambient_keys: &[
            Key {
                at: 0.000000,
                rgb: [0.509804, 0.509804, 0.235294],
            },
            Key {
                at: 0.231117,
                rgb: [0.901961, 0.731347, 0.350997],
            },
            Key {
                at: 0.246667,
                rgb: [0.941177, 0.784314, 0.814924],
            },
            Key {
                at: 0.253333,
                rgb: [0.941177, 1.000000, 0.765979],
            },
            Key {
                at: 0.731117,
                rgb: [0.941177, 1.000000, 0.758340],
            },
            Key {
                at: 0.771667,
                rgb: [0.843137, 0.748154, 0.477748],
            },
            Key {
                at: 0.793333,
                rgb: [0.843137, 0.748154, 0.277450],
            },
            Key {
                at: 0.820000,
                rgb: [0.901961, 0.748154, 0.196078],
            },
            Key {
                at: 1.000000,
                rgb: [0.509804, 0.509804, 0.235294],
            },
        ],
    };

    pub static FELWOOD_TERRAIN: DncLight = DncLight {
        model: "FDirectSun",
        light_type: 1,
        attenuation: [80.000000, 200.000000],
        sun_color: [1.000000, 1.000000, 1.000000],
        sun_intensity: 1.300000,
        ambient_color: [1.000000, 1.000000, 1.000000],
        ambient_intensity: 0.250000,
        sun_direction: [-0.675456, 0.615662, 0.405856],
        sun_keys: &[
            Key {
                at: 0.000000,
                rgb: [0.594072, 0.660840, 0.235294],
            },
            Key {
                at: 0.222233,
                rgb: [0.772289, 0.877800, 0.470588],
            },
            Key {
                at: 0.250000,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.250567,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.722783,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.750567,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.803900,
                rgb: [0.867140, 0.907757, 0.627451],
            },
            Key {
                at: 1.000000,
                rgb: [0.594072, 0.660840, 0.235294],
            },
        ],
        ambient_keys: &[
            Key {
                at: 0.000000,
                rgb: [0.509804, 0.509804, 0.235294],
            },
            Key {
                at: 0.231117,
                rgb: [0.901961, 0.731347, 0.350997],
            },
            Key {
                at: 0.246667,
                rgb: [0.941177, 0.784314, 0.814924],
            },
            Key {
                at: 0.253333,
                rgb: [0.941177, 1.000000, 0.765979],
            },
            Key {
                at: 0.731117,
                rgb: [0.941177, 1.000000, 0.758340],
            },
            Key {
                at: 0.771667,
                rgb: [0.843137, 0.748154, 0.477748],
            },
            Key {
                at: 0.793333,
                rgb: [0.843137, 0.748154, 0.277450],
            },
            Key {
                at: 0.820000,
                rgb: [0.901961, 0.748154, 0.196078],
            },
            Key {
                at: 1.000000,
                rgb: [0.509804, 0.509804, 0.235294],
            },
        ],
    };

    pub static FELWOOD_UNIT: DncLight = DncLight {
        model: "FDirectSun",
        light_type: 1,
        attenuation: [80.000000, 200.000000],
        sun_color: [1.000000, 1.000000, 1.000000],
        sun_intensity: 1.300000,
        ambient_color: [1.000000, 1.000000, 1.000000],
        ambient_intensity: 0.400000,
        sun_direction: [-0.675456, 0.615662, 0.405856],
        sun_keys: &[
            Key {
                at: 0.000000,
                rgb: [0.594072, 0.660840, 0.235294],
            },
            Key {
                at: 0.222233,
                rgb: [0.772289, 0.877800, 0.470588],
            },
            Key {
                at: 0.250000,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.250567,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.722783,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.750567,
                rgb: [0.996078, 0.996078, 0.996078],
            },
            Key {
                at: 0.803900,
                rgb: [0.867140, 0.907757, 0.627451],
            },
            Key {
                at: 1.000000,
                rgb: [0.594072, 0.660840, 0.235294],
            },
        ],
        ambient_keys: &[
            Key {
                at: 0.000000,
                rgb: [0.509804, 0.509804, 0.235294],
            },
            Key {
                at: 0.231117,
                rgb: [0.901961, 0.731347, 0.350997],
            },
            Key {
                at: 0.246667,
                rgb: [0.941177, 0.784314, 0.814924],
            },
            Key {
                at: 0.253333,
                rgb: [0.941177, 1.000000, 0.765979],
            },
            Key {
                at: 0.731117,
                rgb: [0.941177, 1.000000, 0.758340],
            },
            Key {
                at: 0.771667,
                rgb: [0.843137, 0.748154, 0.477748],
            },
            Key {
                at: 0.793333,
                rgb: [0.843137, 0.748154, 0.277450],
            },
            Key {
                at: 0.820000,
                rgb: [0.901961, 0.748154, 0.196078],
            },
            Key {
                at: 1.000000,
                rgb: [0.509804, 0.509804, 0.235294],
            },
        ],
    };

    pub static DALARAN_TERRAIN: DncLight = DncLight {
        model: "FDirectSun",
        light_type: 1,
        attenuation: [80.000000, 200.000000],
        sun_color: [1.000000, 1.000000, 1.000000],
        sun_intensity: 1.300000,
        ambient_color: [1.000000, 1.000000, 1.000000],
        ambient_intensity: 0.300000,
        sun_direction: [-0.675456, 0.615662, 0.405856],
        sun_keys: &[
            Key {
                at: 0.000000,
                rgb: [0.600000, 0.400000, 0.235294],
            },
            Key {
                at: 0.222233,
                rgb: [1.000000, 0.705882, 0.470588],
            },
            Key {
                at: 0.250000,
                rgb: [1.000000, 1.000000, 1.000000],
            },
            Key {
                at: 0.250567,
                rgb: [1.000000, 1.000000, 1.000000],
            },
            Key {
                at: 0.722783,
                rgb: [1.000000, 1.000000, 1.000000],
            },
            Key {
                at: 0.750567,
                rgb: [1.000000, 1.000000, 1.000000],
            },
            Key {
                at: 0.811667,
                rgb: [0.942026, 0.792157, 0.743399],
            },
            Key {
                at: 1.000000,
                rgb: [0.600000, 0.400000, 0.235294],
            },
        ],
        ambient_keys: &[
            Key {
                at: 0.000000,
                rgb: [0.600000, 0.400000, 0.235294],
            },
            Key {
                at: 0.223333,
                rgb: [0.901961, 0.392157, 0.350997],
            },
            Key {
                at: 0.256667,
                rgb: [0.830782, 0.877820, 1.000000],
            },
            Key {
                at: 0.726117,
                rgb: [0.830782, 0.877820, 1.000000],
            },
            Key {
                at: 0.755000,
                rgb: [0.815085, 0.392157, 0.717125],
            },
            Key {
                at: 0.786117,
                rgb: [0.830046, 0.392157, 0.447632],
            },
            Key {
                at: 0.823900,
                rgb: [0.890740, 0.392157, 0.366260],
            },
            Key {
                at: 1.000000,
                rgb: [0.600000, 0.400000, 0.235294],
            },
        ],
    };

    pub static DALARAN_UNIT: DncLight = DncLight {
        model: "FDirectSun",
        light_type: 1,
        attenuation: [80.000000, 200.000000],
        sun_color: [1.000000, 1.000000, 1.000000],
        sun_intensity: 1.300000,
        ambient_color: [1.000000, 1.000000, 1.000000],
        ambient_intensity: 0.700000,
        sun_direction: [-0.675456, 0.615662, 0.405856],
        sun_keys: &[
            Key {
                at: 0.000000,
                rgb: [0.600000, 0.400000, 0.235294],
            },
            Key {
                at: 0.222233,
                rgb: [1.000000, 0.705882, 0.470588],
            },
            Key {
                at: 0.250000,
                rgb: [1.000000, 1.000000, 1.000000],
            },
            Key {
                at: 0.250567,
                rgb: [1.000000, 1.000000, 1.000000],
            },
            Key {
                at: 0.722783,
                rgb: [1.000000, 1.000000, 1.000000],
            },
            Key {
                at: 0.750567,
                rgb: [1.000000, 1.000000, 1.000000],
            },
            Key {
                at: 0.811667,
                rgb: [0.942026, 0.792157, 0.743399],
            },
            Key {
                at: 1.000000,
                rgb: [0.600000, 0.400000, 0.235294],
            },
        ],
        ambient_keys: &[
            Key {
                at: 0.000000,
                rgb: [0.600000, 0.400000, 0.235294],
            },
            Key {
                at: 0.223333,
                rgb: [0.901961, 0.392157, 0.350997],
            },
            Key {
                at: 0.256667,
                rgb: [0.830782, 0.877820, 1.000000],
            },
            Key {
                at: 0.726117,
                rgb: [0.830782, 0.877820, 1.000000],
            },
            Key {
                at: 0.755000,
                rgb: [0.815085, 0.392157, 0.717125],
            },
            Key {
                at: 0.786117,
                rgb: [0.830046, 0.392157, 0.447632],
            },
            Key {
                at: 0.823900,
                rgb: [0.890740, 0.392157, 0.366260],
            },
            Key {
                at: 1.000000,
                rgb: [0.600000, 0.400000, 0.235294],
            },
        ],
    };

    pub static DUNGEON_TERRAIN: DncLight = DncLight {
        model: "FDirectSun",
        light_type: 1,
        attenuation: [80.000000, 200.000000],
        sun_color: [1.000000, 1.000000, 1.000000],
        sun_intensity: 1.300000,
        ambient_color: [1.000000, 1.000000, 1.000000],
        ambient_intensity: 0.300000,
        sun_direction: [0.507538, 0.856091, 0.097539],
        sun_keys: &[],
        ambient_keys: &[],
    };

    pub static DUNGEON_UNIT: DncLight = DncLight {
        model: "FDirectSun",
        light_type: 1,
        attenuation: [80.000000, 200.000000],
        sun_color: [1.000000, 0.258824, 0.192157],
        sun_intensity: 1.600000,
        ambient_color: [1.000000, 1.000000, 1.000000],
        ambient_intensity: 0.400000,
        sun_direction: [-0.558730, 0.773914, -0.298123],
        sun_keys: &[],
        ambient_keys: &[
            Key {
                at: 0.000000,
                rgb: [0.854902, 0.823529, 0.835294],
            },
            Key {
                at: 0.000000,
                rgb: [1.000000, 1.000000, 1.000000],
            },
            Key {
                at: 1.000000,
                rgb: [1.000000, 1.000000, 1.000000],
            },
        ],
    };

    pub static UNDERGROUND_TERRAIN: DncLight = DncLight {
        model: "FDirectSun",
        light_type: 1,
        attenuation: [80.000000, 200.000000],
        sun_color: [1.000000, 1.000000, 1.000000],
        sun_intensity: 1.300000,
        ambient_color: [1.000000, 1.000000, 1.000000],
        ambient_intensity: 0.300000,
        sun_direction: [0.507538, 0.856091, 0.097539],
        sun_keys: &[],
        ambient_keys: &[],
    };

    pub static UNDERGROUND_UNIT: DncLight = DncLight {
        model: "FDirectSun",
        light_type: 1,
        attenuation: [80.000000, 200.000000],
        sun_color: [0.192157, 0.945098, 0.741176],
        sun_intensity: 1.600000,
        ambient_color: [1.000000, 1.000000, 1.000000],
        ambient_intensity: 0.400000,
        sun_direction: [-0.611354, 0.733055, -0.298122],
        sun_keys: &[],
        ambient_keys: &[
            Key {
                at: 0.000000,
                rgb: [0.854902, 0.823529, 0.835294],
            },
            Key {
                at: 0.000000,
                rgb: [1.000000, 1.000000, 1.000000],
            },
            Key {
                at: 1.000000,
                rgb: [1.000000, 1.000000, 1.000000],
            },
        ],
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// test id: render.daynight.curve
    #[test]
    fn render_daynight_curve_dawn_brightens_dusk_darkens_night_has_floor() {
        let curve = DayNightCurve::default();
        // Dawn: the DNC track ramps between 05:20 and 06:00; sample the
        // transition so the test checks brightening rather than the plateau.
        let samples: Vec<f32> = [4.0, 4.5, 5.0, 5.5, 6.0]
            .iter()
            .map(|h| curve.sample(*h).terrain.luminance)
            .collect();
        for pair in samples.windows(2) {
            assert!(
                pair[1] > pair[0],
                "dawn must brighten monotonically: {samples:?}"
            );
        }
        // Dusk: 18:00 -> 24:00 darkens monotonically.
        let dusk: Vec<f32> = [18.0, 19.0, 20.0, 21.0, 22.0, 23.0, 23.99]
            .iter()
            .map(|h| curve.sample(*h).terrain.luminance)
            .collect();
        for pair in dusk.windows(2) {
            assert!(
                pair[1] < pair[0],
                "dusk must darken monotonically: {dusk:?}"
            );
        }
        let noon = curve.sample(12.0).terrain.luminance;
        let midnight = curve.sample(0.0).terrain.luminance;
        assert!(noon > midnight, "noon {noon} must beat midnight {midnight}");
        assert!(midnight > 0.05, "night must not be pure black: {midnight}");
        assert!(
            midnight > noon * 0.20,
            "night floor should stay above 20% of noon: {midnight} vs {noon}"
        );
        // Units use the unit DNC (stronger ambient) and must stay >= terrain.
        assert!(curve.sample(0.0).unit.luminance >= midnight);
        // The grade product must be finite and positive at every hour.
        for step in 0..240 {
            let sample = curve.sample(step as f32 / 10.0);
            assert!(
                sample
                    .terrain
                    .grade
                    .iter()
                    .all(|c| c.is_finite() && *c >= 0.0)
            );
            assert!(sample.terrain.grade[3] > 0.0);
            assert!(sample.fog_color.iter().all(|c| *c > 0.0));
        }
    }

    /// test id: render.daynight.clock
    #[test]
    fn render_daynight_clock_wc3_default_and_configured_rate() {
        // WC3 default: DayLength=480 real seconds per 24 game hours.
        let mut clock = DayNightClock::new();
        clock.advance_seconds(DEFAULT_DAY_LENGTH_SECONDS);
        assert!(
            (clock.time_of_day() - 0.0).abs() < 1e-3,
            "full day wraps to 00:00"
        );
        assert_eq!(clock.day(), 2, "the second day starts at 24:00");
        // 8 game hours per 160 real seconds (the default rate expressed as
        // "8 hours per configured real minutes", 2.6667 minutes).
        let mut clock = DayNightClock::new();
        clock.advance_seconds(DEFAULT_DAY_LENGTH_SECONDS / 3.0);
        assert!((clock.time_of_day() - 8.0).abs() < 1e-3, "8h after 160s");
        // Literal mission mapping: 8 game hours per 8 real minutes.
        let mut clock = DayNightClock::new().with_hours_per_real_minutes(8.0, 8.0);
        assert!((clock.seconds_per_day() - 1440.0).abs() < 1e-3);
        clock.advance_seconds(8.0 * 60.0);
        assert!((clock.time_of_day() - 8.0).abs() < 1e-3, "8h after 8 min");
        // Tick-driven clock agrees with the seconds path (30 Hz).
        let mut tick_clock = DayNightClock::new();
        tick_clock.advance_ticks(30 * 160);
        assert!((tick_clock.time_of_day() - 8.0).abs() < 1e-3);
        // Jass SetTimeOfDayScale doubles the rate.
        let mut scaled = DayNightClock::new().with_scale(2.0);
        scaled.advance_seconds(80.0);
        assert!((scaled.time_of_day() - 8.0).abs() < 1e-3);
    }

    #[test]
    fn dnc_parser_matches_every_embedded_preset() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_full/Environment/DNC");
        if !root.is_dir() {
            eprintln!("skipping: DNC fixtures missing");
            return;
        }
        for (rel, table) in [
            (
                "DNCLordaeron/DNCLordaeronTerrain/DNCLordaeronTerrain.mdx",
                Preset::Lordaeron.terrain(),
            ),
            (
                "DNCLordaeron/DNCLordaeronUnit/DNCLordaeronUnit.mdx",
                Preset::Lordaeron.unit(),
            ),
            (
                "DNCAshenvale/DNCAshenValeTerrain/DNCAshenValeTerrain.mdx",
                Preset::Ashenvale.terrain(),
            ),
            (
                "DNCAshenvale/DNCAshenValeUnit/DNCAshenValeUnit.mdx",
                Preset::Ashenvale.unit(),
            ),
            (
                "DNCFelwood/DNCFelWoodTerrain/DNCFelWoodTerrain.mdx",
                Preset::Felwood.terrain(),
            ),
            (
                "DNCFelwood/DNCFelWoodUnit/DNCFelWoodUnit.mdx",
                Preset::Felwood.unit(),
            ),
            (
                "DNCDalaran/DNCDalaranTerrain/DNCDalaranTerrain.mdx",
                Preset::Dalaran.terrain(),
            ),
            (
                "DNCDalaran/DNCDalaranUnit/DNCDalaranUnit.mdx",
                Preset::Dalaran.unit(),
            ),
            (
                "DNCDungeon/DNCDungeonTerrain/DNCDungeonTerrain.mdx",
                Preset::Dungeon.terrain(),
            ),
            (
                "DNCDungeon/DNCDungeonUnit/DNCDungeonUnit.mdx",
                Preset::Dungeon.unit(),
            ),
            (
                "DNCUnderground/DNCUndergroundTerrain/DNCUndergroundTerrain.mdx",
                Preset::Underground.terrain(),
            ),
            (
                "DNCUnderground/DNCUndergroundUnit/DNCUndergroundUnit.mdx",
                Preset::Underground.unit(),
            ),
        ] {
            let bytes = std::fs::read(root.join(rel)).expect("read DNC fixture");
            let parsed = parse_dnc_lights(&bytes).expect("parse DNC");
            let embedded = table.to_data();
            assert_eq!(parsed.model, embedded.model, "{rel}: model name");
            assert_eq!(parsed.light_type, embedded.light_type, "{rel}: light type");
            assert_eq!(
                parsed.sun_intensity, embedded.sun_intensity,
                "{rel}: sun intensity"
            );
            assert_eq!(
                parsed.ambient_intensity, embedded.ambient_intensity,
                "{rel}: ambient intensity"
            );
            assert_eq!(
                parsed.sun_keys.len(),
                embedded.sun_keys.len(),
                "{rel}: KLAC rows"
            );
            assert_eq!(
                parsed.ambient_keys.len(),
                embedded.ambient_keys.len(),
                "{rel}: KLBC rows"
            );
            for (a, b) in parsed.sun_keys.iter().zip(embedded.sun_keys.iter()) {
                assert!((a.at - b.at).abs() < 1e-4, "{rel}: KLAC time");
                for c in 0..3 {
                    assert!((a.rgb[c] - b.rgb[c]).abs() < 1e-5, "{rel}: KLAC colour");
                }
            }
            for (a, b) in parsed.ambient_keys.iter().zip(embedded.ambient_keys.iter()) {
                assert!((a.at - b.at).abs() < 1e-4, "{rel}: KLBC time");
                for c in 0..3 {
                    assert!((a.rgb[c] - b.rgb[c]).abs() < 1e-5, "{rel}: KLBC colour");
                }
            }
            for c in 0..3 {
                assert!(
                    (parsed.sun_direction[c] - embedded.sun_direction[c]).abs() < 1e-4,
                    "{rel}: sun direction"
                );
            }
        }
    }

    #[test]
    fn w3i_v18_has_no_fog_and_v28_fixture_parses() {
        let old = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_map/Human04/war3map.w3i");
        if let Ok(bytes) = std::fs::read(&old) {
            let map = MapLighting::from_w3i(&bytes).expect("parse v18 w3i");
            assert_eq!(map.version, 18);
            assert!(map.fog.is_none(), "v18 predates the fog block");
            assert!(map.light_environment.is_none());
        }
        let newer = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../donors/War3Net/tests/War3Net.TestTools.UnitTesting/TestData/MapFiles/DefaultMapFiles/war3map.w3i",
        );
        if let Ok(bytes) = std::fs::read(&newer) {
            let map = MapLighting::from_w3i(&bytes).expect("parse v28 w3i");
            assert_eq!(map.version, 28);
            assert!(
                map.light_environment.is_some(),
                "v25+ carries LightEnvironment"
            );
        }
    }

    #[test]
    fn phase_and_log_line() {
        assert!(!is_night(6.0));
        assert!(!is_night(17.99));
        assert!(is_night(18.0));
        assert!(is_night(0.0));
        assert_eq!(phase(5.5), Phase::Dawn);
        assert_eq!(phase(12.0), Phase::Day);
        assert_eq!(phase(18.5), Phase::Dusk);
        assert_eq!(phase(23.0), Phase::Night);
        assert_eq!(game_time_line(3, 6.5), "GAME_TIME day=3 hour=06.50");
    }

    /// `H2_DAYNIGHT=off|0` is the A/B switch; no other test reads this key.
    #[test]
    fn h2_daynight_off_switch() {
        // SAFETY: this is the only test that touches H2_DAYNIGHT.
        unsafe { std::env::set_var("H2_DAYNIGHT", "off") };
        assert!(!enabled(), "off must disable the cycle");
        unsafe { std::env::set_var("H2_DAYNIGHT", "0") };
        assert!(!enabled(), "0 must disable the cycle");
        unsafe { std::env::set_var("H2_DAYNIGHT", "on") };
        assert!(enabled(), "anything else keeps the cycle");
        unsafe { std::env::remove_var("H2_DAYNIGHT") };
        assert!(enabled(), "unset keeps the cycle");
    }

    /// `[TerrainLights]` mapping: every letter without its own row falls back
    /// to the Lordaeron model, so Human04 (`L`) resolves to the preset whose
    /// frames were captured.
    #[test]
    fn tileset_mapping_matches_world_edit_table() {
        assert_eq!(Preset::for_tileset('L'), Preset::Lordaeron);
        assert_eq!(Preset::for_tileset('F'), Preset::Lordaeron);
        assert_eq!(Preset::for_tileset('W'), Preset::Lordaeron);
        assert_eq!(Preset::for_tileset('B'), Preset::Lordaeron);
        assert_eq!(Preset::for_tileset('N'), Preset::Lordaeron);
        assert_eq!(Preset::for_tileset('Y'), Preset::Lordaeron);
        assert_eq!(Preset::for_tileset('V'), Preset::Lordaeron);
        assert_eq!(Preset::for_tileset('Q'), Preset::Lordaeron);
        assert_eq!(Preset::for_tileset('A'), Preset::Ashenvale);
        assert_eq!(Preset::for_tileset('C'), Preset::Felwood);
        assert_eq!(Preset::for_tileset('X'), Preset::Dalaran);
        assert_eq!(Preset::for_tileset('D'), Preset::Dungeon);
        assert_eq!(Preset::for_tileset('G'), Preset::Underground);
        assert_eq!(
            Preset::for_tileset('l'),
            Preset::Lordaeron,
            "case-insensitive"
        );
    }
}
