//! Upgrades: costs per level plus up to four effects.
//!
//! `UpgradeData.slk` stores four effect slots (`effect1..4`, `base1..4`,
//! `mod1..4`) and per-level costs as `base + mod * (level - 1)` — the form the
//! shipped data uses (e.g. `Rhme`: `base1=1`, `mod1=1`, `maxlevel=3` gives +1/+2/+3
//! melee damage).

use wc3_formats::gamedata::FourCc;

use crate::error::{DataError, Result};
use crate::table::{Table, TableKind};
use crate::GameData;

/// One of the four effect slots of an upgrade.
#[derive(Debug, Clone, PartialEq)]
pub struct UpgradeEffect {
    /// 1-based slot (`effect1` … `effect4`).
    pub slot: u8,
    /// Effect code (`rarm`, `ratd`, `rhpo`, …); `None` when the slot is unused.
    pub code: Option<String>,
    /// Value at level 1 (fractional for some codes, e.g. `Rhar` armour 0.2).
    pub base: Option<f32>,
    /// Value added per additional level, or a multiplier for some codes.
    pub delta: Option<f32>,
    /// `UpgradeEffectMetaData.slk` description when the root ships it (TFT).
    pub comment: Option<String>,
    /// `UpgradeEffectMetaData.slk` data type (`int`, `real`, `unreal`, …).
    pub data_type: Option<String>,
}

impl UpgradeEffect {
    /// Raw effect value for `level`: `base + delta * (level - 1)`.
    ///
    /// Some effect codes treat `mod` as a factor rather than an additive step
    /// (`rhpo` uses `base`/`mod` as a fraction); the arithmetic is therefore
    /// exposed as stored and named [`UpgradeEffect::level_value`], not "total".
    pub fn level_value(&self, level: u32) -> Option<f32> {
        let base = self.base?;
        let delta = self.delta.unwrap_or(0.0);
        Some(base + delta * (level.saturating_sub(1) as f32))
    }
}

/// An upgrade.
#[derive(Clone, Copy)]
pub struct UpgradeView<'a> {
    pub(crate) gd: &'a GameData,
    pub(crate) id: FourCc,
}

impl<'a> UpgradeView<'a> {
    pub fn id(&self) -> FourCc {
        self.id
    }

    pub fn id_str(&self) -> String {
        self.id.as_str().into_owned()
    }

    fn table(&self) -> Result<&'a Table> {
        self.gd
            .table(TableKind::UpgradeData)
            .ok_or(DataError::MissingColumn {
                table: "UpgradeData".to_string(),
                column: "upgradeid".to_string(),
            })
    }

    fn cell(&self, column: &str) -> Option<&'a str> {
        self.gd
            .table(TableKind::UpgradeData)?
            .text(self.id.as_str().as_ref(), column)
    }

    fn num(&self, column: &str) -> Result<Option<i32>> {
        self.table()?.i32_overlaid(self.id.as_str().as_ref(), column)
    }

    /// Fractional cell (upgrade effect base/mod values can be real, e.g. 0.2).
    fn real(&self, column: &str) -> Result<Option<f32>> {
        self.table()?.f32_overlaid(self.id.as_str().as_ref(), column)
    }

    fn flag(&self, column: &str) -> Result<Option<bool>> {
        self.table()?.bool(self.id.as_str().as_ref(), column)
    }

    // ---- text -----------------------------------------------------------

    pub fn name(&self) -> Option<String> {
        self.gd.localizer().name(self.id.as_str().as_ref())
    }

    pub fn tip(&self) -> Option<String> {
        self.gd.localizer().tip(self.id.as_str().as_ref())
    }

    pub fn ubertip(&self) -> Option<String> {
        self.gd.localizer().ubertip(self.id.as_str().as_ref())
    }

    pub fn description(&self) -> Option<String> {
        self.gd
            .localizer()
            .description(self.id.as_str().as_ref())
    }

    pub fn icon(&self) -> Option<String> {
        self.gd.localizer().icon(self.id.as_str().as_ref())
    }

    pub fn hotkey(&self) -> Option<String> {
        self.gd.localizer().hotkey(self.id.as_str().as_ref())
    }

    pub fn button_pos(&self) -> Option<(u32, u32)> {
        self.gd
            .localizer()
            .research_button_pos(self.id.as_str().as_ref())
            .or_else(|| self.gd.localizer().button_pos(self.id.as_str().as_ref()))
    }

    /// `Researchart` — the research-tab icon, falling back to `Art`.
    ///
    /// Retail race upgrades carry only `Art` (the effect icon); an object-data
    /// override or a hero ability may add `Researchart` for the button.
    pub fn research_icon(&self) -> Option<String> {
        self.gd
            .localizer()
            .research_icon(self.id.as_str().as_ref())
            .or_else(|| self.icon())
    }

    /// `Researchtip` — research-tab tooltip, falling back to `Tip`.
    pub fn research_tip(&self) -> Option<String> {
        self.gd
            .localizer()
            .research_tip(self.id.as_str().as_ref())
            .or_else(|| self.tip())
    }

    /// `Researchubertip` — research-tab extended tooltip, falling back to `Ubertip`.
    pub fn research_ubertip(&self) -> Option<String> {
        self.gd
            .localizer()
            .research_ubertip(self.id.as_str().as_ref())
            .or_else(|| self.ubertip())
    }

    /// `Researchhotkey` — research-tab hotkey, falling back to `Hotkey`.
    pub fn research_hotkey(&self) -> Option<String> {
        self.gd
            .localizer()
            .research_hotkey(self.id.as_str().as_ref())
            .or_else(|| self.hotkey())
    }

    /// Tech requirements (`Requires` plus the `Requires1..N` encoding).
    pub fn requires(&self) -> Vec<String> {
        self.gd.requirements(self.id.as_str().as_ref())
    }

    // ---- structure ------------------------------------------------------

    /// `comments` — editor comment, the shipped English label.
    pub fn comments(&self) -> Option<&'a str> {
        self.cell("comments")
    }

    /// `class` — upgrade family (`melee`, `ranged`, `armor`, `caster`, …).
    pub fn class(&self) -> Option<&'a str> {
        self.cell("class")
    }

    pub fn race(&self) -> Option<&'a str> {
        self.cell("race")
    }

    /// `maxlevel` — number of upgrade levels.
    pub fn max_level(&self) -> Result<Option<i32>> {
        self.num("maxlevel")
    }

    /// `inherit` — whether the upgrade is carried to upgraded units.
    pub fn inherit(&self) -> Result<bool> {
        Ok(self.flag("inherit")?.unwrap_or(false))
    }

    /// `used` — whether the upgrade is enabled in melee.
    pub fn used(&self) -> Result<bool> {
        Ok(self.flag("used")?.unwrap_or(false))
    }

    /// `flag` — raw flag bitset as stored.
    pub fn flag_bits(&self) -> Result<Option<i32>> {
        self.num("flag")
    }

    // ---- cost -----------------------------------------------------------

    pub fn gold_base(&self) -> Result<Option<i32>> {
        self.num("goldbase")
    }

    pub fn gold_per_level(&self) -> Result<Option<i32>> {
        self.num("goldmod")
    }

    pub fn lumber_base(&self) -> Result<Option<i32>> {
        self.num("lumberbase")
    }

    pub fn lumber_per_level(&self) -> Result<Option<i32>> {
        self.num("lumbermod")
    }

    pub fn time_base(&self) -> Result<Option<i32>> {
        self.num("timebase")
    }

    pub fn time_per_level(&self) -> Result<Option<i32>> {
        self.num("timemod")
    }

    /// Gold cost of `level`: `goldbase + goldmod * (level - 1)`.
    pub fn gold_cost(&self, level: u32) -> Result<Option<i32>> {
        Ok(level_cost(self.gold_base()?, self.gold_per_level()?, level))
    }

    /// Lumber cost of `level`.
    pub fn lumber_cost(&self, level: u32) -> Result<Option<i32>> {
        Ok(level_cost(
            self.lumber_base()?,
            self.lumber_per_level()?,
            level,
        ))
    }

    /// Research time of `level` in seconds.
    pub fn research_time(&self, level: u32) -> Result<Option<i32>> {
        Ok(level_cost(self.time_base()?, self.time_per_level()?, level))
    }

    // ---- effects --------------------------------------------------------

    /// All four effect slots (unused slots carry `code: None`).
    pub fn effects(&self) -> Result<Vec<UpgradeEffect>> {
        let mut out = Vec::with_capacity(4);
        for slot in 1..=4u8 {
            let code = self.cell(&format!("effect{slot}")).map(str::to_string);
            let base = self.real(&format!("base{slot}"))?;
            let delta = self.real(&format!("mod{slot}"))?;
            let (comment, data_type) = self
                .gd
                .upgrade_effect(code.as_deref())
                .map(|meta| (meta.0.clone(), meta.1.clone()))
                .unwrap_or((None, None));
            out.push(UpgradeEffect {
                slot,
                code,
                base,
                delta,
                comment,
                data_type,
            });
        }
        Ok(out)
    }

    /// Only the effects that name a code.
    pub fn active_effects(&self) -> Result<Vec<UpgradeEffect>> {
        Ok(self
            .effects()?
            .into_iter()
            .filter(|effect| effect.code.is_some())
            .collect())
    }

    /// Effect values for `level`, in slot order.
    pub fn effects_at(&self, level: u32) -> Result<Vec<(UpgradeEffect, Option<f32>)>> {
        Ok(self
            .active_effects()?
            .into_iter()
            .map(|effect| {
                let value = effect.level_value(level);
                (effect, value)
            })
            .collect())
    }
}

fn level_cost(base: Option<i32>, per_level: Option<i32>, level: u32) -> Option<i32> {
    let base = base?;
    let per_level = per_level.unwrap_or(0);
    Some(base + per_level * (level.saturating_sub(1) as i32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemorySource;

    fn source() -> MemorySource {
        let mut source = MemorySource::new();
        source.insert(
            "Units\\UpgradeData.slk",
            b"ID;PWXL;N;E\nB;Y2;X15;D0\n\
C;Y1;X1;K\"upgradeid\"\nC;X2;K\"comments\"\nC;X3;K\"class\"\nC;X4;K\"race\"\nC;X5;K\"used\"\n\
C;X6;K\"maxlevel\"\nC;X7;K\"goldbase\"\nC;X8;K\"goldmod\"\nC;X9;K\"lumberbase\"\nC;X10;K\"lumbermod\"\n\
C;X11;K\"timebase\"\nC;X12;K\"timemod\"\nC;X13;K\"effect1\"\nC;X14;K\"base1\"\nC;X15;K\"mod1\"\n\
C;Y2;X1;K\"Rhme\"\nC;X2;K\"human melee attack\"\nC;X3;K\"melee\"\nC;X4;K\"human\"\nC;X5;K1\n\
C;X6;K3\nC;X7;K100\nC;X8;K150\nC;X9;K50\nC;X10;K50\nC;X11;K60\nC;X12;K15\n\
C;X13;K\"ratd\"\nC;X14;K1\nC;X15;K1\n"
                .to_vec(),
        );
        source
    }

    #[test]
    fn computes_level_costs_and_effects() {
        let gd = GameData::load(&source()).expect("load");
        let upgrade = gd.upgrade("Rhme").expect("upgrade");
        assert_eq!(upgrade.max_level().unwrap(), Some(3));
        assert_eq!(upgrade.gold_cost(1).unwrap(), Some(100));
        assert_eq!(upgrade.gold_cost(3).unwrap(), Some(400));
        assert_eq!(upgrade.lumber_cost(2).unwrap(), Some(100));
        assert_eq!(upgrade.research_time(3).unwrap(), Some(90));
        let effects = upgrade.active_effects().unwrap();
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].code.as_deref(), Some("ratd"));
        assert_eq!(effects[0].level_value(1), Some(1.0));
        assert_eq!(effects[0].level_value(3), Some(3.0));
    }

    /// Retail writes upgrade requirements as `Requirescount` + `Requires1..N`
    /// with an empty `Requires=`, which the old reader dropped entirely.
    #[test]
    fn reads_the_numbered_requires_encoding() {
        let mut source = source();
        source.insert(
            "Units\\HumanUpgradeFunc.txt",
            "[Rhme]\nArt=BTNSteelMelee.blp\nHotkey=S,S,S\nButtonpos=0,0\n\
Requirescount=3\nRequires=\nRequires1=hkee\nRequires2=hcas\n"
                .as_bytes()
                .to_vec(),
        );
        source.insert(
            "Units\\HumanUpgradeStrings.txt",
            "[Rhme]\nName=Iron Swords\nTip=Iron Swords\nUbertip=More damage\n".as_bytes().to_vec(),
        );
        let gd = GameData::load(&source).expect("load");
        let upgrade = gd.upgrade("Rhme").expect("upgrade");
        assert_eq!(upgrade.requires(), ["hkee", "hcas"]);
        assert_eq!(gd.requirements("Rhme"), ["hkee", "hcas"]);
        // Research-card fields fall back to the plain keys when the retail file
        // ships no `Research*` variant (race upgrades never do).
        assert_eq!(upgrade.research_icon().as_deref(), Some("BTNSteelMelee.blp"));
        assert_eq!(upgrade.research_hotkey().as_deref(), Some("S,S,S"));
        assert_eq!(upgrade.research_tip().as_deref(), Some("Iron Swords"));
        assert_eq!(upgrade.research_ubertip().as_deref(), Some("More damage"));

        let entry = gd.research_entry("Rhme").expect("research entry");
        assert_eq!(entry.id, "Rhme");
        assert_eq!(entry.icon.as_deref(), Some("BTNSteelMelee.blp"));
        assert_eq!(entry.gold, Some(100));
        assert_eq!(entry.requires, ["hkee", "hcas"]);
        assert_eq!(entry.requires_names, ["hkee", "hcas"]);
        assert_eq!(entry.kind, wc3_formats::gamedata::ObjectKind::Upgrade);
    }

    /// A `Researchart`/`Researchtip` override wins over the plain keys.
    #[test]
    fn research_variants_beat_the_plain_keys() {
        let mut source = source();
        source.insert(
            "Units\\HumanUpgradeFunc.txt",
            "[Rhme]\nArt=BTNSteelMelee.blp\nResearchart=BTNSteelMeleeResearch.blp\n".as_bytes().to_vec(),
        );
        source.insert(
            "Units\\HumanUpgradeStrings.txt",
            "[Rhme]\nName=Iron Swords\nTip=Iron Swords\nResearchtip=Research iron swords\n".as_bytes().to_vec(),
        );
        let gd = GameData::load(&source).expect("load");
        let upgrade = gd.upgrade("Rhme").expect("upgrade");
        assert_eq!(
            upgrade.research_icon().as_deref(),
            Some("BTNSteelMeleeResearch.blp")
        );
        assert_eq!(
            upgrade.research_tip().as_deref(),
            Some("Research iron swords")
        );
    }
}
