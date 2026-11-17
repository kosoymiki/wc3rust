//! Per-model animator registry keyed by model path or unit definition.
//!
//! The game used to own exactly four meshes (one per `UnitKind`). The parity
//! matrix (increment 1, "per-definition model loading") needs the runtime to
//! hold any number of models keyed by the definition's `model` path, reuse one
//! parsed `MdxModel` + `ModelRuntime` per key, and hand out independent
//! `Animator` instances (one per unit instance).
//!
//! ```text
//! let mut registry = AnimatorRegistry::new(256);
//! registry.load_path("units/human/footman/footman.mdx", path)?;
//! let mut animator = registry.animator("units/human/footman/footman.mdx", &stand)?;
//! animator.update(dt);
//! let bones = animator.world_matrices();
//! ```
//!
//! Keys are normalised (`normalize_key`): lower case, `\` -> `/`, so a definition
//! path, an MDX texture path and a filesystem path address the same entry.

use std::collections::{HashMap, VecDeque};
use std::path::Path;

use wc3_formats::mdx::{MdxModel, Sequence};

use crate::{Animator, ModelRuntime};

/// Semantic ability animation shared by hero and spellcaster models.
///
/// The same gameplay event is authored under different sequence names across
/// the shipped models (`Spell`, `Spell Throw`, `Spell Slam`, `Spell - 1`,
/// `Attack Walk Stand Spin` for Bladestorm, ...). Callers ask for the ability
/// kind; the registry owns the alias table so no consumer hard-codes a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbilitySequence {
    Cast,
    Channel,
    Attack,
    Victory,
    Death,
    Dissipate,
}

impl AbilitySequence {
    /// High-priority aliases (case-insensitive; a bare alias also matches
    /// numbered variants such as `Spell - 1` and `Attack - 2`).
    pub fn aliases(self) -> &'static [&'static str] {
        match self {
            Self::Cast => &["Spell", "Attack Walk Stand Spin", "Cast"],
            Self::Channel => &["Stand Channel", "Channel"],
            Self::Attack => &["Attack - 1", "Attack -1", "Attack"],
            Self::Victory => &["Stand Victory", "Victory"],
            Self::Death => &["Death"],
            Self::Dissipate => &["Dissipate"],
        }
    }

    /// Last-resort aliases used only when none of [`Self::aliases`] matched; a
    /// match here is reported with `fallback == true` so callers can decide
    /// whether the substitute is acceptable.
    pub fn fallback_aliases(self) -> &'static [&'static str] {
        match self {
            Self::Cast => &["Attack Slam", "Attack", "Stand Ready", "Stand"],
            Self::Channel => &["Spell", "Stand Ready", "Stand"],
            Self::Attack => &["Attack Slam", "Stand Ready", "Stand"],
            Self::Victory => &["Spell", "Attack Slam", "Stand"],
            Self::Death => &["Dissipate", "Stand"],
            Self::Dissipate => &["Death", "Stand"],
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Cast => "cast",
            Self::Channel => "channel",
            Self::Attack => "attack",
            Self::Victory => "victory",
            Self::Death => "death",
            Self::Dissipate => "dissipate",
        }
    }
}

/// Semantic projectile/missile animation. Missiles ship only `Stand`, `Birth`
/// and `Death` variants; flight uses `Stand`, impact uses `Death`, and the
/// one-frame `Birth` puff is the spawn effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectileSequence {
    Flight,
    Birth,
    Impact,
}

impl ProjectileSequence {
    pub fn aliases(self) -> &'static [&'static str] {
        match self {
            Self::Flight => &["Fly", "Stand"],
            Self::Birth => &["Birth"],
            Self::Impact => &["Death", "Explode", "Impact"],
        }
    }

    pub fn fallback_aliases(self) -> &'static [&'static str] {
        match self {
            Self::Flight => &["Birth", "Death"],
            Self::Birth => &["Stand"],
            Self::Impact => &["Birth", "Stand"],
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Flight => "flight",
            Self::Birth => "birth",
            Self::Impact => "impact",
        }
    }
}

/// Resolved sequence plus how it was found.
#[derive(Debug, Clone)]
pub struct SequenceMatch {
    pub sequence: Sequence,
    /// Alias that matched (after the exact pass and the numbered-variant pass).
    pub matched_alias: Option<&'static str>,
    /// `true` when only a fallback alias matched, i.e. the model does not
    /// author the requested animation under a canonical name.
    pub fallback: bool,
}

impl SequenceMatch {
    /// Exact/numbered alias match (not a fallback).
    pub fn is_exact(&self) -> bool {
        self.matched_alias.is_some() && !self.fallback
    }
}

/// Does `name` match `alias` either exactly or as a numbered/variant prefix
/// (`Spell` also matches `Spell - 1`, `Spell Throw`, `Spell Slam`)?
fn alias_matches(name: &str, alias: &str) -> bool {
    if name.len() < alias.len() {
        return false;
    }
    let (head, tail) = name.split_at(alias.len());
    if !head.eq_ignore_ascii_case(alias) {
        return false;
    }
    match tail.chars().next() {
        None => true,
        Some(c) => {
            c == ' '
                || c == '-'
                || c == '_'
                || c == '('
                || c == ')'
                || c == '.'
                || c.is_ascii_digit()
        }
    }
}

/// Find the first playable sequence matching `aliases`: exact names first,
/// then numbered/variant prefixes, preserving alias priority in both passes.
fn find_sequence<'a>(
    sequences: &'a [Sequence],
    aliases: &'static [&'static str],
) -> Option<(&'a Sequence, &'static str)> {
    for alias in aliases {
        if let Some(sequence) = sequences
            .iter()
            .find(|sequence| sequence.end > sequence.start && sequence.name.eq_ignore_ascii_case(alias))
        {
            return Some((sequence, alias));
        }
    }
    for alias in aliases {
        if let Some(sequence) = sequences
            .iter()
            .find(|sequence| sequence.end > sequence.start && alias_matches(&sequence.name, alias))
        {
            return Some((sequence, alias));
        }
    }
    None
}

/// Gameplay animation state the engine drives every unit/building through.
///
/// Unlike [`AbilitySequence`], state resolution is **strict**: a state either
/// maps to an authored MDX sequence or it is reported missing. There is no
/// silent fallback to another sequence (the old game code fell back to
/// sequence index 0, which is `Death` on many models).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameplayState {
    Idle,
    Walk,
    Attack,
    Death,
    Cast,
    Channel,
    /// Worker constructing/repairing a building (`Stand Work`).
    Build,
    /// Building under construction (`Birth`), played once and held.
    Birth,
}

impl GameplayState {
    /// Canonical resolution order for this state (case-insensitive; a bare
    /// alias also matches numbered variants such as `Stand - 1`).
    pub fn aliases(self) -> &'static [&'static str] {
        match self {
            // "Stand Ready" is the alert stance; idle is the base Stand
            // (numbered variants like "Stand - 1" also match the prefix).
            Self::Idle => &["Stand", "Stand Ready"],
            Self::Walk => &["Walk"],
            Self::Attack => &["Attack - 1", "Attack -1", "Attack"],
            Self::Death => &["Death"],
            // Bladestorm is the Blademaster's cast animation.
            Self::Cast => &["Spell", "Attack Walk Stand Spin", "Cast"],
            Self::Channel => &["Stand Channel", "Channel"],
            Self::Build => &["Stand Work", "Build"],
            Self::Birth => &["Birth"],
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Walk => "walk",
            Self::Attack => "attack",
            Self::Death => "death",
            Self::Cast => "cast",
            Self::Channel => "channel",
            Self::Build => "build",
            Self::Birth => "birth",
        }
    }

    /// States whose MDX sequence is non-looping (`flags & 1`): play once and
    /// hold the last frame. Used for the death window and construction birth.
    pub fn one_shot(self) -> bool {
        matches!(self, Self::Death | Self::Birth)
    }

    pub const ALL: [GameplayState; 8] = [
        GameplayState::Idle,
        GameplayState::Walk,
        GameplayState::Attack,
        GameplayState::Death,
        GameplayState::Cast,
        GameplayState::Channel,
        GameplayState::Build,
        GameplayState::Birth,
    ];
}

/// Result of resolving one [`GameplayState`] for a model.
#[derive(Debug, Clone)]
pub struct StateResolution {
    pub state: GameplayState,
    /// `None` means the model does not author this state (reported, not
    /// substituted).
    pub sequence: Option<Sequence>,
}

impl StateResolution {
    pub fn name(&self) -> Option<&str> {
        self.sequence.as_ref().map(|sequence| sequence.name.as_str())
    }

    pub fn missing(&self) -> bool {
        self.sequence.is_none()
    }
}

/// Idle selection: the base stand pose, not a situational variant.
/// Preference: exact `Stand` -> a `Stand*` name without variant markers
/// (ready/defend/victory/work/...) -> any `Stand*`. File order alone is not
/// enough: several models author `Stand Ready` before `Stand - 1`.
fn find_idle_sequence(sequences: &[Sequence]) -> Option<Sequence> {
    let playable = |sequence: &&Sequence| sequence.end > sequence.start;
    if let Some(sequence) = sequences
        .iter()
        .find(|sequence| playable(&sequence) && sequence.name.eq_ignore_ascii_case("Stand"))
    {
        return Some(sequence.clone());
    }
    const VARIANTS: [&str; 12] = [
        "ready", "defend", "victory", "work", "gold", "lumber", "upgrade", "alternate",
        "morph", "birth", "death", "decay",
    ];
    if let Some(sequence) = sequences.iter().find(|sequence| {
        playable(&sequence)
            && alias_matches(&sequence.name, "Stand")
            && !VARIANTS
                .iter()
                .any(|marker| sequence.name.to_ascii_lowercase().contains(marker))
    }) {
        return Some(sequence.clone());
    }
    sequences
        .iter()
        .find(|sequence| playable(&sequence) && alias_matches(&sequence.name, "Stand"))
        .cloned()
}

/// Build selection: the construction `Stand Work`, not a resource-gathering
/// variant (`Stand Work Gold` / `Stand Work Lumber`).
fn find_build_sequence(sequences: &[Sequence]) -> Option<Sequence> {
    let playable = |sequence: &&Sequence| sequence.end > sequence.start;
    let work = |sequence: &&Sequence| {
        playable(sequence) && alias_matches(&sequence.name, "Stand Work")
    };
    sequences
        .iter()
        .find(|sequence| {
            work(sequence)
                && !sequence.name.to_ascii_lowercase().contains("gold")
                && !sequence.name.to_ascii_lowercase().contains("lumber")
        })
        .or_else(|| {
            sequences
                .iter()
                .find(|sequence| playable(sequence) && alias_matches(&sequence.name, "Build"))
        })
        .or_else(|| sequences.iter().find(work))
        .cloned()
}

fn state_sequence_of(sequences: &[Sequence], state: GameplayState) -> Option<Sequence> {
    match state {
        GameplayState::Idle => find_idle_sequence(sequences),
        GameplayState::Build => find_build_sequence(sequences),
        other => find_sequence(sequences, other.aliases()).map(|(sequence, _)| sequence.clone()),
    }
}

/// Canonical registry key: lower case, forward slashes, no trailing NULs.
pub fn normalize_key(path: &str) -> String {
    path.trim_end_matches('\0')
        .replace('\\', "/")
        .to_ascii_lowercase()
}

struct Entry {
    model: MdxModel,
    runtime: ModelRuntime,
}

/// LRU registry of parsed models plus their precomputed runtime state.
pub struct AnimatorRegistry {
    capacity: usize,
    entries: HashMap<String, Entry>,
    /// Most-recently-used first.
    order: VecDeque<String>,
    evictions: usize,
}

impl AnimatorRegistry {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: HashMap::new(),
            order: VecDeque::new(),
            evictions: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn evictions(&self) -> usize {
        self.evictions
    }

    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(&normalize_key(key))
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// Insert an already parsed model under `key` (e.g. the definition's
    /// `model` path). Returns `true` when an entry was evicted to make room.
    pub fn insert(&mut self, key: &str, model: MdxModel) -> bool {
        let key = normalize_key(key);
        let runtime = ModelRuntime::new(&model);
        self.entries.insert(key.clone(), Entry { model, runtime });
        self.touch(&key);
        self.evict_if_needed()
    }

    /// Parse `path` and insert it under `key`.
    pub fn load_path(
        &mut self,
        key: &str,
        path: &Path,
    ) -> Result<(), wc3_formats::FormatError> {
        let bytes = std::fs::read(path).map_err(|_| wc3_formats::FormatError::TooShort)?;
        let model = MdxModel::parse(&bytes)?;
        self.insert(key, model);
        Ok(())
    }

    /// Insert with a freshly read file, keyed by the path itself.
    pub fn load(&mut self, path: &Path) -> Result<(), wc3_formats::FormatError> {
        let key = path.to_string_lossy().into_owned();
        self.load_path(&key, path)
    }

    /// Independent animator for one instance, already playing `sequence`.
    ///
    /// The registry keeps one parsed model per key; every call returns a fresh
    /// `Animator` so units sharing a definition animate independently.
    pub fn animator(&mut self, key: &str, sequence: &Sequence) -> Option<Animator> {
        let key = normalize_key(key);
        self.entries.get(&key)?;
        self.touch(&key);
        let entry = self.entries.get(&key)?;
        let mut animator = Animator::from_model(&entry.model);
        animator.play(sequence);
        Some(animator)
    }

    /// Borrow the parsed model + its runtime (skeleton, skins, visibility).
    pub fn with_runtime<R>(
        &mut self,
        key: &str,
        f: impl FnOnce(&MdxModel, &ModelRuntime) -> R,
    ) -> Option<R> {
        let key = normalize_key(key);
        let entry = self.entries.get(&key)?;
        let value = f(&entry.model, &entry.runtime);
        self.touch(&key);
        Some(value)
    }

    /// Sequence lookup by name for a registered model.
    pub fn sequence(&self, key: &str, name: &str) -> Option<Sequence> {
        let key = normalize_key(key);
        let entry = self.entries.get(&key)?;
        entry
            .model
            .sequences
            .iter()
            .find(|sequence| sequence.name.eq_ignore_ascii_case(name))
            .cloned()
    }

    /// Alias-table lookup: exact alias matches win, then numbered variants
    /// (`Spell` -> `Spell - 1`), then the kind's fallback aliases.
    pub fn match_sequence(&self, key: &str, aliases: &'static [&'static str]) -> Option<SequenceMatch> {
        let key = normalize_key(key);
        let entry = self.entries.get(&key)?;
        if let Some((sequence, matched_alias)) = find_sequence(&entry.model.sequences, aliases) {
            return Some(SequenceMatch {
                sequence: sequence.clone(),
                matched_alias: Some(matched_alias),
                fallback: false,
            });
        }
        None
    }

    /// Resolve a hero ability animation (cast/channel/attack/victory/death/
    /// dissipate) for a registered model. Exact and numbered matches are
    /// preferred; when the model does not author the animation under any
    /// canonical name the kind's fallback aliases are tried and the result is
    /// still returned with `fallback == true`.
    pub fn ability_sequence(&self, key: &str, kind: AbilitySequence) -> Option<SequenceMatch> {
        if let Some(found) = self.match_sequence(key, kind.aliases()) {
            return Some(found);
        }
        let key = normalize_key(key);
        let entry = self.entries.get(&key)?;
        let (sequence, matched_alias) = find_sequence(&entry.model.sequences, kind.fallback_aliases())?;
        Some(SequenceMatch {
            sequence: sequence.clone(),
            matched_alias: Some(matched_alias),
            fallback: true,
        })
    }

    /// Independent animator already playing the ability sequence.
    pub fn ability_animator(
        &mut self,
        key: &str,
        kind: AbilitySequence,
    ) -> Option<(SequenceMatch, Animator)> {
        let matched = self.ability_sequence(key, kind)?;
        let animator = self.animator(key, &matched.sequence)?;
        Some((matched, animator))
    }

    /// Resolve a projectile/missile animation (flight/birth/impact). The same
    /// fallback rule as [`Self::ability_sequence`] applies.
    pub fn projectile_sequence(
        &self,
        key: &str,
        kind: ProjectileSequence,
    ) -> Option<SequenceMatch> {
        if let Some(found) = self.match_sequence(key, kind.aliases()) {
            return Some(found);
        }
        let key = normalize_key(key);
        let entry = self.entries.get(&key)?;
        let (sequence, matched_alias) =
            find_sequence(&entry.model.sequences, kind.fallback_aliases())?;
        Some(SequenceMatch {
            sequence: sequence.clone(),
            matched_alias: Some(matched_alias),
            fallback: true,
        })
    }

    /// Independent animator already playing the projectile sequence.
    pub fn projectile_animator(
        &mut self,
        key: &str,
        kind: ProjectileSequence,
    ) -> Option<(SequenceMatch, Animator)> {
        let matched = self.projectile_sequence(key, kind)?;
        let animator = self.animator(key, &matched.sequence)?;
        Some((matched, animator))
    }

    /// Strict gameplay-state lookup (idle/walk/attack/death/cast/channel).
    /// Only authored aliases match; a missing state returns `None` so the
    /// caller can report it instead of silently playing another sequence.
    pub fn state_sequence(&self, key: &str, state: GameplayState) -> Option<SequenceMatch> {
        if matches!(state, GameplayState::Idle | GameplayState::Build) {
            let key = normalize_key(key);
            let entry = self.entries.get(&key)?;
            let sequence = state_sequence_of(&entry.model.sequences, state)?;
            return Some(SequenceMatch {
                sequence,
                matched_alias: state.aliases().first().copied(),
                fallback: false,
            });
        }
        self.match_sequence(key, state.aliases())
    }

    /// Independent animator already playing the gameplay-state sequence.
    pub fn state_animator(
        &mut self,
        key: &str,
        state: GameplayState,
    ) -> Option<(SequenceMatch, Animator)> {
        let matched = self.state_sequence(key, state)?;
        let animator = self.animator(key, &matched.sequence)?;
        Some((matched, animator))
    }

    /// Resolve every gameplay state for one model; missing states are explicit.
    pub fn state_resolutions(&self, key: &str) -> Option<Vec<StateResolution>> {
        let key = normalize_key(key);
        let entry = self.entries.get(&key)?;
        Some(
            GameplayState::ALL
                .iter()
                .map(|state| StateResolution {
                    state: *state,
                    sequence: state_sequence_of(&entry.model.sequences, *state),
                })
                .collect(),
        )
    }

    /// Every playable sequence name in file order (diagnostics/tooling).
    pub fn sequence_names(&self, key: &str) -> Option<Vec<String>> {
        let key = normalize_key(key);
        let entry = self.entries.get(&key)?;
        Some(
            entry
                .model
                .sequences
                .iter()
                .filter(|sequence| sequence.end > sequence.start)
                .map(|sequence| sequence.name.clone())
                .collect(),
        )
    }

    fn touch(&mut self, key: &str) {
        self.order.retain(|candidate| candidate != key);
        self.order.push_front(key.to_string());
    }

    fn evict_if_needed(&mut self) -> bool {
        let mut evicted = false;
        while self.entries.len() > self.capacity {
            let Some(victim) = self.order.pop_back() else {
                break;
            };
            if self.entries.remove(&victim).is_some() {
                self.evictions += 1;
                evicted = true;
            }
        }
        evicted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model_path(relative: &str) -> Option<std::path::PathBuf> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for tree in ["analysis/war3_samples", "analysis/war3_full"] {
            let candidate = root.join(tree).join(relative);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        None
    }

    #[test]
    fn keys_normalise_paths_and_defs() {
        assert_eq!(
            normalize_key("Units\\Human\\Footman\\Footman.mdx"),
            "units/human/footman/footman.mdx"
        );
        assert_eq!(
            normalize_key("UNITS/HUMAN/FOOTMAN/FOOTMAN.MDX\0"),
            "units/human/footman/footman.mdx"
        );
    }

    fn seq(name: &str, start: u32, end: u32) -> Sequence {
        Sequence {
            name: name.to_string(),
            start,
            end,
            move_speed: 0.0,
            flags: 0,
            rarity: 0.0,
            sync_point: 0,
            bounds: Default::default(),
        }
    }

    #[test]
    fn idle_selection_prefers_base_stand_over_ready_and_variants() {
        let list = vec![
            seq("Stand Ready", 0, 100),
            seq("Stand Victory", 200, 300),
            seq("Stand - 1", 400, 500),
        ];
        assert_eq!(find_idle_sequence(&list).expect("idle").name, "Stand - 1");
        let exact = vec![seq("Stand Ready", 0, 100), seq("Stand", 200, 300)];
        assert_eq!(find_idle_sequence(&exact).expect("idle").name, "Stand");
        let only_variants = vec![seq("Stand Ready", 0, 100)];
        assert_eq!(
            find_idle_sequence(&only_variants).expect("idle fallback").name,
            "Stand Ready"
        );
    }

    #[test]
    fn build_selection_prefers_construction_work_over_resource_variants() {
        let list = vec![
            seq("Stand Work Gold", 0, 100),
            seq("Stand Work", 200, 300),
        ];
        assert_eq!(find_build_sequence(&list).expect("build").name, "Stand Work");
        let gold_only = vec![seq("Stand Work Gold", 0, 100)];
        assert_eq!(
            find_build_sequence(&gold_only).expect("build fallback").name,
            "Stand Work Gold"
        );
        assert!(find_build_sequence(&[seq("Walk", 0, 100)]).is_none());
    }

    #[test]
    fn alias_matching_accepts_numbered_variants_only_at_separator() {
        assert!(alias_matches("Spell", "Spell"));
        assert!(alias_matches("Spell Throw", "Spell"));
        assert!(alias_matches("Spell Slam", "Spell"));
        assert!(alias_matches("Spell - 1", "Spell"));
        assert!(alias_matches("Attack -1", "Attack"));
        assert!(alias_matches("Stand Channel", "Stand Channel"));
        assert!(alias_matches("death", "Death"));
        assert!(!alias_matches("Spellbound", "Spell"));
        assert!(!alias_matches("Spel", "Spell"));
    }

    #[test]
    fn fallback_is_reported_when_a_kind_is_not_authored() {
        let Some(blade_master) = model_path("Units/Orc/HeroBladeMaster/HeroBladeMaster.mdx") else {
            eprintln!("skipping: BladeMaster fixture missing");
            return;
        };
        let Some(demon_hunter) = model_path("Units/NightElf/HeroDemonHunter/HeroDemonHunter.mdx")
        else {
            eprintln!("skipping: DemonHunter fixture missing");
            return;
        };
        let mut registry = AnimatorRegistry::new(8);
        registry.load(&blade_master).expect("load blade master");
        registry.load(&demon_hunter).expect("load demon hunter");
        let blade_key = blade_master.to_string_lossy().into_owned();
        let hunter_key = demon_hunter.to_string_lossy().into_owned();

        // Bladestorm is authored as `Attack Walk Stand Spin`, an exact cast alias.
        let cast = registry
            .ability_sequence(&blade_key, AbilitySequence::Cast)
            .expect("blade master cast");
        assert!(cast.is_exact(), "cast must not fall back: {:?}", cast.sequence.name);
        assert_eq!(cast.sequence.name, "Attack Walk Stand Spin");
        // No `Stand Channel` on the Blademaster: the substitute is flagged.
        let channel = registry
            .ability_sequence(&blade_key, AbilitySequence::Channel)
            .expect("blade master channel fallback");
        assert!(channel.fallback, "channel must be a reported fallback");
        // The Demon Hunter authors both canonically.
        let hunter_cast = registry
            .ability_sequence(&hunter_key, AbilitySequence::Cast)
            .expect("demon hunter cast");
        assert!(hunter_cast.is_exact());
        assert_eq!(hunter_cast.sequence.name, "Spell");
        let hunter_channel = registry
            .ability_sequence(&hunter_key, AbilitySequence::Channel)
            .expect("demon hunter channel");
        assert!(hunter_channel.is_exact());
        assert_eq!(hunter_channel.sequence.name, "Stand Channel");

        // Projectiles: a missile has `Stand` (flight) and `Death` (impact).
        if let Some(missile) = model_path("Abilities/Weapons/Arrow/ArrowMissile.mdx") {
            let mut registry = AnimatorRegistry::new(4);
            registry.load(&missile).expect("load arrow missile");
            let key = missile.to_string_lossy().into_owned();
            let flight = registry
                .projectile_sequence(&key, ProjectileSequence::Flight)
                .expect("flight");
            assert!(flight.is_exact());
            assert_eq!(flight.sequence.name, "Stand");
            let impact = registry
                .projectile_sequence(&key, ProjectileSequence::Impact)
                .expect("impact");
            assert!(impact.is_exact());
            assert_eq!(impact.sequence.name, "Death");
        } else {
            eprintln!("skipping: ArrowMissile fixture missing");
        }
    }

    #[test]
    fn one_parsed_model_serves_independent_animators() {
        let Some(path) = model_path("Units/Human/Footman/Footman.mdx") else {
            eprintln!("skipping: Footman fixture missing");
            return;
        };
        let mut registry = AnimatorRegistry::new(8);
        // Definitions address models by their MDX path, not the filesystem path.
        let key = "Units\\Human\\Footman\\Footman.mdx";
        registry.load_path(key, &path).expect("load footman");
        assert!(registry.contains(key));
        let stand = registry.sequence(key, "Stand - 1").expect("stand");
        let mut a = registry.animator(key, &stand).expect("animator a");
        let mut b = registry.animator(key, &stand).expect("animator b");
        // Independent clocks: advancing one must not move the other.
        for _ in 0..20 {
            a.update(1.0 / 30.0);
        }
        let a_bones = a.world_matrices();
        let b_bones = b.world_matrices();
        let delta = a_bones
            .iter()
            .zip(b_bones.iter())
            .map(|(x, y)| {
                x.to_cols_array()
                    .iter()
                    .zip(y.to_cols_array().iter())
                    .map(|(p, q)| (p - q).abs())
                    .fold(0.0f32, f32::max)
            })
            .fold(0.0f32, f32::max);
        assert!(delta > 1e-3, "animators share state (delta {delta})");
        // Reuse: still one entry after two animator calls.
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn capacity_evicts_least_recently_used() {
        let Some(footman) = model_path("Units/Human/Footman/Footman.mdx") else {
            eprintln!("skipping: fixtures missing");
            return;
        };
        let Some(archer) = model_path("Units/NightElf/Archer/Archer.mdx") else {
            eprintln!("skipping: fixtures missing");
            return;
        };
        let mut registry = AnimatorRegistry::new(1);
        registry.load(&footman).expect("load footman");
        registry.load(&archer).expect("load archer");
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.evictions(), 1);
        assert!(registry.contains(archer.to_string_lossy().as_ref()));
        assert!(!registry.contains(footman.to_string_lossy().as_ref()));
        // Touching a key keeps it resident.
        let key = archer.to_string_lossy().into_owned();
        let _ = registry.with_runtime(&key, |model, runtime| (model.bones.len(), runtime.geoset_count));
        registry.load(&footman).expect("load footman again");
        assert_eq!(registry.len(), 1);
        assert!(registry.contains(footman.to_string_lossy().as_ref()));
    }

    #[test]
    fn runtime_lookup_exposes_skeleton_and_visibility() {
        let Some(path) = model_path("Buildings/Human/TownHall/TownHall.mdx") else {
            eprintln!("skipping: TownHall fixture missing");
            return;
        };
        let mut registry = AnimatorRegistry::new(4);
        registry.load(&path).expect("load townhall");
        let key = path.to_string_lossy().into_owned();
        let stand = registry.sequence(&key, "Stand").expect("stand");
        let (geosets, visible, moving) = registry
            .with_runtime(&key, |model, runtime| {
                let start = runtime.visible(model, &stand, stand.start);
                (runtime.geoset_count, start, runtime.skeleton.bone_count)
            })
            .expect("runtime");
        assert_eq!(geosets, 36);
        assert_eq!(visible, vec![11, 13, 14]);
        assert_eq!(moving, 47);
    }
}
