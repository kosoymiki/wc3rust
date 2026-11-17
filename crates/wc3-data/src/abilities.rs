//! Abilities: per-level metadata, target flags and tooltip resolution.
//!
//! `AbilityData.slk` stores up to four levels in parallel column families. RoC
//! writes `Cast1..3`/`Cool1..3`/`Data11..Data14` plus `*Check` columns, TFT writes
//! `Cast1..4`/`Cool1..4`/`DataA1..DataE4`. Level discovery therefore probes the
//! columns that actually exist instead of assuming a fixed count
//! (`observed`: RoC 3 levels, TFT 4 levels).

use wc3_formats::gamedata::{FieldTarget, FourCc, MetaKind};

use crate::enums::EnumValue;
use crate::error::Result;
use crate::table::{Table, TableKind};
use crate::GameData;

/// One level of an ability. Values are stored exactly as the sheets hold them;
/// "absent" cells (`-`, `_`, empty) become `None`.
#[derive(Debug, Clone, PartialEq)]
pub struct AbilityLevel {
    pub level: u32,
    /// `Cast{n}` as stored (the editor's cast time column, no conversion).
    pub cast: Option<f32>,
    /// `Dur{n}` — duration in seconds.
    pub duration: Option<f32>,
    /// `HeroDur{n}` — hero duration in seconds.
    pub hero_duration: Option<f32>,
    /// `Cool{n}` — cooldown in seconds.
    pub cooldown: Option<f32>,
    /// `Cost{n}` — mana cost.
    pub mana_cost: Option<i32>,
    /// `Area{n}` — area of effect.
    pub area: Option<f32>,
    /// `Rng{n}` — cast range.
    pub range: Option<f32>,
    /// `DataA`..`DataE` (TFT) or `Data1`..`Data4` (RoC) slots, verbatim.
    pub data: Vec<Option<String>>,
    /// Column names backing [`AbilityLevel::data`], so callers can name them.
    pub data_columns: Vec<String>,
    /// `UnitID{n}` — the unit an ability summons/creates, when used.
    pub unit_id: Option<String>,
}

/// A spell, aura, item ability or ability template.
#[derive(Clone, Copy)]
pub struct AbilityView<'a> {
    pub(crate) gd: &'a GameData,
    pub(crate) id: FourCc,
}

impl<'a> AbilityView<'a> {
    pub fn id(&self) -> FourCc {
        self.id
    }

    pub fn id_str(&self) -> String {
        self.id.as_str().into_owned()
    }

    fn table(&self) -> Option<&'a Table> {
        self.gd.table(TableKind::AbilityData)
    }

    fn cell(&self, column: &str) -> Option<&'a str> {
        self.table()?.text(self.id.as_str().as_ref(), column)
    }

    fn num(&self, column: &str) -> Result<Option<i32>> {
        match self.table() {
            Some(table) => table.i32_overlaid(self.id.as_str().as_ref(), column),
            None => Ok(None),
        }
    }

    fn real(&self, column: &str) -> Result<Option<f32>> {
        match self.table() {
            Some(table) => table.f32(self.id.as_str().as_ref(), column),
            None => Ok(None),
        }
    }

    fn flag(&self, column: &str) -> Result<Option<bool>> {
        match self.table() {
            Some(table) => table.bool(self.id.as_str().as_ref(), column),
            None => Ok(None),
        }
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

    pub fn icon(&self) -> Option<String> {
        self.gd.localizer().icon(self.id.as_str().as_ref())
    }

    pub fn hotkey(&self) -> Option<String> {
        self.gd.localizer().hotkey(self.id.as_str().as_ref())
    }

    pub fn unhotkey(&self) -> Option<String> {
        self.gd
            .localizer()
            .text(self.id.as_str().as_ref(), "Unhotkey")
    }

    pub fn research_hotkey(&self) -> Option<String> {
        self.gd
            .localizer()
            .text(self.id.as_str().as_ref(), "Researchhotkey")
    }

    pub fn button_pos(&self) -> Option<(u32, u32)> {
        self.gd.localizer().button_pos(self.id.as_str().as_ref())
    }

    pub fn research_button_pos(&self) -> Option<(u32, u32)> {
        self.gd
            .localizer()
            .research_button_pos(self.id.as_str().as_ref())
    }

    pub fn editor_suffix(&self) -> Option<String> {
        self.gd
            .localizer()
            .text(self.id.as_str().as_ref(), "EditorSuffix")
    }

    pub fn research_tip(&self) -> Option<String> {
        self.gd
            .localizer()
            .text(self.id.as_str().as_ref(), "Researchtip")
    }

    pub fn research_ubertip(&self) -> Option<String> {
        self.gd
            .localizer()
            .text(self.id.as_str().as_ref(), "Researchubertip")
    }

    /// `Researchart` — the learn-skill button icon, falling back to `Art`.
    ///
    /// Passive hero abilities ship `Art=PASBTN*.blp` (the field icon) plus
    /// `Researchart=BTN*.blp` (the learn button), e.g. `[AHab]` Brilliance Aura.
    pub fn research_icon(&self) -> Option<String> {
        self.gd
            .localizer()
            .research_icon(self.id.as_str().as_ref())
            .or_else(|| self.icon())
    }

    // ---- inheritance ----------------------------------------------------

    /// `code` — the base ability this row inherits unset text from.
    ///
    /// Retail stores variants as rows that only carry the fields that differ:
    /// `[AIa1]` (code `AIab`) has `Art` and `EditorSuffix` but no `Name`, while
    /// `[AIab]` has `Name` and no `Art`. Reading either row alone is what makes
    /// a card guess, so the `resolved_*` accessors follow this link.
    ///
    /// The base may be a profile-only row (measured: `AIab` has no
    /// `AbilityData` row at all, only a text section), which is why the
    /// fallbacks read the localiser directly instead of requiring a sheet row.
    pub fn base_code(&self) -> Option<&'a str> {
        let code = self.code()?.trim();
        if code.is_empty() || code.eq_ignore_ascii_case(self.id.as_str().as_ref()) {
            return None;
        }
        Some(code)
    }

    /// The base ability's sheet row, when the base has one.
    pub fn base(&self) -> Option<AbilityView<'a>> {
        self.gd.ability(self.base_code()?)
    }

    /// `Name`, falling back to the base `code` section.
    pub fn resolved_name(&self) -> Option<String> {
        self.name().or_else(|| {
            let code = self.base_code()?;
            self.gd.localizer().name(code)
        })
    }

    /// `Art`, falling back to the base `code` section.
    pub fn resolved_icon(&self) -> Option<String> {
        self.icon().or_else(|| {
            let code = self.base_code()?;
            self.gd.localizer().icon(code)
        })
    }

    /// `Hotkey`, falling back to the base `code` section.
    pub fn resolved_hotkey(&self) -> Option<String> {
        self.hotkey().or_else(|| {
            let code = self.base_code()?;
            self.gd.localizer().hotkey(code)
        })
    }

    /// `Tip` of `level`, falling back to the base `code` section.
    pub fn resolved_tip(&self, level: u32) -> Option<String> {
        self.tip_level(level).or_else(|| {
            let code = self.base_code()?;
            self.gd.localizer().text_level(code, "Tip", level)
        })
    }

    /// `Ubertip` of `level` with `<id,Field>` tokens expanded, falling back to
    /// the base `code` section.
    pub fn resolved_tooltip(&self, level: u32) -> Option<String> {
        self.resolved_ubertip_level(level).or_else(|| {
            let code = self.base_code()?;
            let text = self.gd.localizer().text_level(code, "Ubertip", level)?;
            Some(self.resolve_tokens(&text))
        })
    }

    /// `Ubertip` of `level` (per-level quoted lists are common).
    pub fn ubertip_level(&self, level: u32) -> Option<String> {
        self.gd
            .localizer()
            .text_level(self.id.as_str().as_ref(), "Ubertip", level)
    }

    /// `Tip` of `level`.
    pub fn tip_level(&self, level: u32) -> Option<String> {
        self.gd
            .localizer()
            .text_level(self.id.as_str().as_ref(), "Tip", level)
    }

    /// Per-level tooltip with `<Ability,Field>` references replaced.
    pub fn resolved_ubertip_level(&self, level: u32) -> Option<String> {
        let text = self.ubertip_level(level)?;
        Some(self.resolve_tokens(&text))
    }

    /// `ubertip` with `<Ability,Field>` references replaced by real values.
    ///
    /// The token form is the one the shipped tooltips use, e.g. `<AIrt,Data11>`
    /// in `analysis/war3_data/Units/ItemStrings.txt`.
    pub fn resolved_ubertip(&self) -> Option<String> {
        let text = self.ubertip()?;
        Some(self.resolve_tokens(&text))
    }

    /// Replace `<target,field>` tokens; unknown tokens are left intact.
    pub fn resolve_tokens(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find('<') {
            let Some(end) = rest[start..].find('>') else {
                break;
            };
            out.push_str(&rest[..start]);
            let token = &rest[start + 1..start + end];
            match token.split_once(',') {
                Some((target, field)) => match self.lookup_reference(target.trim(), field.trim()) {
                    Some(value) => out.push_str(&value),
                    None => {
                        out.push('<');
                        out.push_str(token);
                        out.push('>');
                    }
                },
                None => {
                    out.push('<');
                    out.push_str(token);
                    out.push('>');
                }
            }
            rest = &rest[start + end + 1..];
        }
        out.push_str(rest);
        out
    }

    fn lookup_reference(&self, target: &str, field: &str) -> Option<String> {
        let table = self.gd.table(TableKind::AbilityData)?;
        if target.is_empty() || !table.contains(target) {
            return None;
        }
        if let Some(value) = table.text(target, field) {
            return Some(value.to_string());
        }
        let meta = self.gd.meta(MetaKind::Ability)?;
        if let FieldTarget::Column { column, .. } =
            wc3_formats::gamedata::resolve_field(Some(meta), field, 1, 0)
        {
            if let Some(value) = table.text(target, &column) {
                return Some(value.to_string());
            }
        }
        None
    }

    // ---- structure ------------------------------------------------------

    /// `code` — the base ability this entry inherits from.
    pub fn code(&self) -> Option<&'a str> {
        self.cell("code")
    }

    /// `uberAlias` (RoC column) — ultimate grouping.
    pub fn uber_alias(&self) -> Option<&'a str> {
        self.cell("uberAlias")
    }

    /// `comments` — the editor comment, the shipped English label.
    pub fn comments(&self) -> Option<&'a str> {
        self.cell("comments")
    }

    pub fn is_hero_ability(&self) -> Result<bool> {
        Ok(self.flag("hero")?.unwrap_or(false))
    }

    pub fn is_item_ability(&self) -> Result<bool> {
        Ok(self.flag("item")?.unwrap_or(false))
    }

    pub fn use_in_editor(&self) -> Result<bool> {
        Ok(self.flag("useInEditor")?.unwrap_or(false))
    }

    /// `levels` — number of levels the entry declares.
    pub fn declared_levels(&self) -> Result<Option<i32>> {
        self.num("levels")
    }

    /// `reqLevel` — hero level required.
    pub fn required_level(&self) -> Result<Option<i32>> {
        self.num("reqLevel")
    }

    /// `levelSkip` — levels gained per hero level (TFT column).
    pub fn level_skip(&self) -> Result<Option<i32>> {
        self.num("levelSkip")
    }

    /// `priority` — AI priority (TFT column).
    pub fn priority(&self) -> Result<Option<i32>> {
        self.num("priority")
    }

    pub fn race(&self) -> Option<&'a str> {
        self.cell("race")
    }

    pub fn sort(&self) -> Option<&'a str> {
        self.cell("sort")
    }

    /// `targs` — legal targets (`air`, `ground`, `enemy`, `neutral`, …).
    pub fn target_flags(&self) -> Vec<&'a str> {
        self.table()
            .map(|table| table.list(self.id.as_str().as_ref(), "targs"))
            .unwrap_or_default()
    }

    pub fn targets_air(&self) -> bool {
        self.has_target("air")
    }

    pub fn targets_ground(&self) -> bool {
        self.has_target("ground")
    }

    pub fn targets_enemies(&self) -> bool {
        self.has_target("enemy")
    }

    pub fn targets_allies(&self) -> bool {
        self.has_target("friend") || self.has_target("self")
    }

    pub fn targets_structures(&self) -> bool {
        self.has_target("structure")
    }

    pub fn has_target(&self, flag: &str) -> bool {
        self.target_flags()
            .iter()
            .any(|value| value.eq_ignore_ascii_case(flag))
    }

    /// Highest level for which level columns exist in this data root.
    pub fn max_level(&self) -> u32 {
        let Some(table) = self.table() else {
            return 0;
        };
        let mut levels = 0;
        for level in 1..=4u32 {
            let probes = [
                format!("cool{level}"),
                format!("cost{level}"),
                format!("cast{level}"),
                format!("dur{level}"),
            ];
            if probes.iter().any(|probe| table.has_column(probe)) {
                levels = level;
            }
        }
        levels
    }

    /// Column names holding the `DataA`..`DataE` slots for `level`.
    pub fn data_columns(&self, level: u32) -> Vec<String> {
        let Some(table) = self.table() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for slot in 0..5u32 {
            let letter = (b'a' + slot as u8) as char;
            // TFT names the slots `DataA1`..`DataE4` (letter, then level); RoC
            // names them `Data11`..`Data34` (level, then slot). Both are probed.
            let candidates = [
                format!("data{letter}{level}"),
                format!("data{level}{}", slot + 1),
            ];
            if let Some(column) = candidates
                .iter()
                .find(|candidate| table.has_column(candidate))
            {
                out.push(column.clone());
            }
        }
        out
    }

    /// Parsed level, `None` when the level exceeds this root's column families.
    pub fn level(&self, level: u32) -> Result<Option<AbilityLevel>> {
        if level == 0 || level > self.max_level() {
            return Ok(None);
        }
        let suffix = level.to_string();
        let data_columns = self.data_columns(level);
        let mut data = Vec::with_capacity(data_columns.len());
        for column in &data_columns {
            data.push(self.cell(column).map(str::to_string));
        }
        Ok(Some(AbilityLevel {
            level,
            cast: self.real(&format!("Cast{suffix}"))?,
            duration: self.real(&format!("Dur{suffix}"))?,
            hero_duration: self.real(&format!("HeroDur{suffix}"))?,
            cooldown: self.real(&format!("Cool{suffix}"))?,
            mana_cost: self.num(&format!("Cost{suffix}"))?,
            area: self.real(&format!("Area{suffix}"))?,
            range: self.real(&format!("Rng{suffix}"))?,
            data,
            data_columns,
            unit_id: self.cell(&format!("UnitID{suffix}")).map(str::to_string),
        }))
    }

    pub fn levels(&self) -> Result<Vec<AbilityLevel>> {
        let mut out = Vec::new();
        for level in 1..=self.max_level() {
            if let Some(parsed) = self.level(level)? {
                out.push(parsed);
            }
        }
        Ok(out)
    }

    /// Cooldown of `level` in seconds.
    pub fn cooldown(&self, level: u32) -> Result<Option<f32>> {
        Ok(self.level(level)?.and_then(|level| level.cooldown))
    }

    /// Mana cost of `level`.
    pub fn mana_cost(&self, level: u32) -> Result<Option<i32>> {
        Ok(self.level(level)?.and_then(|level| level.mana_cost))
    }

    /// Area of effect of `level`.
    pub fn area(&self, level: u32) -> Result<Option<f32>> {
        Ok(self.level(level)?.and_then(|level| level.area))
    }

    /// Cast range of `level`.
    pub fn range(&self, level: u32) -> Result<Option<f32>> {
        Ok(self.level(level)?.and_then(|level| level.range))
    }

    /// Raw `Data` slot value for `level` and 1-based `slot` (`DataA` = 1).
    pub fn data(&self, level: u32, slot: u32) -> Result<Option<String>> {
        let Some(level) = self.level(level)? else {
            return Ok(None);
        };
        Ok(level
            .data
            .get(slot.saturating_sub(1) as usize)
            .cloned()
            .flatten())
    }

    /// `weaponType`-style enumerator lookup, for metadata-typed ability fields.
    pub fn decode_enum(&self, type_name: &str, value: &str) -> Option<&EnumValue> {
        self.gd.enums()?.decode(type_name, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemorySource;

    fn source() -> MemorySource {
        let mut source = MemorySource::new();
        source.insert(
            "Units\\AbilityData.slk",
            b"ID;PWXL;N;E\nB;Y3;X12;D0\n\
C;Y1;X1;K\"alias\"\nC;X2;K\"code\"\nC;X3;K\"comments\"\nC;X4;K\"hero\"\nC;X5;K\"item\"\n\
C;X6;K\"levels\"\nC;X7;K\"targs\"\nC;X8;K\"Cool1\"\nC;X9;K\"Cost1\"\nC;X10;K\"DataA1\"\n\
C;X11;K\"Cool2\"\nC;X12;K\"Cost2\"\n\
C;Y2;X1;K\"AHbz\"\nC;X2;K\"AHbz\"\nC;X3;K\"Blizzard\"\nC;X4;K1\nC;X5;K0\nC;X6;K2\n\
C;X7;K\"ground,enemy,neutral\"\nC;X8;K6\nC;X9;K75\nC;X10;K6\nC;X11;K6\nC;X12;K75\n\
C;Y3;X1;K\"Adef\"\nC;X2;K\"Adef\"\nC;X3;K\"Defend\"\nC;X4;K0\nC;X5;K0\nC;X6;K1\n\
C;X7;K\"_\"\nC;X8;K0\nC;X9;K0\nC;X10;K0\nC;X11;K-\nC;X12;K-\n"
                .to_vec(),
        );
        source
    }

    #[test]
    fn reads_levels_and_targets() {
        let gd = GameData::load(&source()).expect("load");
        let ability = gd.ability("AHbz").expect("ability");
        assert_eq!(ability.max_level(), 2);
        assert_eq!(ability.declared_levels().unwrap(), Some(2));
        let level = ability.level(1).unwrap().expect("level 1");
        assert_eq!(level.cooldown, Some(6.0));
        assert_eq!(level.mana_cost, Some(75));
        assert_eq!(level.data, vec![Some("6".to_string())]);
        assert_eq!(level.data_columns, vec!["dataa1".to_string()]);
        assert!(ability.targets_ground());
        assert!(ability.targets_enemies());
        assert!(!ability.targets_air());
        assert_eq!(ability.level(3).unwrap(), None);
        assert_eq!(ability.cooldown(2).unwrap(), Some(6.0));
    }

    #[test]
    fn resolves_tooltip_tokens_from_other_abilities() {
        let mut source = source();
        source.insert(
            "Units\\HumanAbilityStrings.txt",
            "[AHbz]\nUbertip=Deals <AHbz,dataa1> damage\n".as_bytes().to_vec(),
        );
        let gd = GameData::load(&source).expect("load");
        let ability = gd.ability("AHbz").expect("ability");
        assert_eq!(
            ability.resolved_ubertip().as_deref(),
            Some("Deals 6 damage")
        );
    }

    /// Retail stores an ability variant as a row that only carries what differs;
    /// the label lives on the base `code` row, which may be profile-only.
    #[test]
    fn variants_inherit_the_base_row_text() {
        let mut source = source();
        source.insert(
            "Units\\AbilityData.slk",
            b"ID;PWXL;N;E\nB;Y3;X6;D0\n\
C;Y1;X1;K\"alias\"\nC;X2;K\"code\"\nC;X3;K\"comments\"\nC;X4;K\"hero\"\nC;X5;K\"item\"\nC;X6;K\"levels\"\n\
C;Y2;X1;K\"AIab\"\nC;X2;K\"AIab\"\nC;X3;K\"Item Stat Bonus\"\nC;X4;K0\nC;X5;K1\nC;X6;K1\n\
C;Y3;X1;K\"AIa1\"\nC;X2;K\"AIab\"\nC;X3;K\"Item Stat Bonus +1\"\nC;X4;K0\nC;X5;K1\nC;X6;K1\n"
                .to_vec(),
        );
        source.insert(
            "Units\\ItemAbilityFunc.txt",
            "[AIab]\n// base row has no Art\n[AIa1]\nArt=BTNRingPurple.blp\n".as_bytes().to_vec(),
        );
        source.insert(
            "Units\\ItemAbilityStrings.txt",
            "[AIab]\nName=Item: hero stat bonus\n[AIa1]\nEditorSuffix=(agility +1)\n"
                .as_bytes()
                .to_vec(),
        );
        let gd = GameData::load(&source).expect("load");
        let variant = gd.ability("AIa1").expect("variant");
        assert!(variant.name().is_none(), "variant must not carry its own Name");
        assert_eq!(
            variant.base_code(),
            Some("AIab"),
            "code link must be exposed"
        );
        assert_eq!(
            variant.resolved_name().as_deref(),
            Some("Item: hero stat bonus")
        );
        // The art stays the variant's own.
        assert_eq!(
            variant.resolved_icon().as_deref(),
            Some("BTNRingPurple.blp")
        );
        // The unified lookup exposes the inherited label.
        let info = gd.object("AIa1").expect("object info");
        assert_eq!(info.name.as_deref(), Some("Item: hero stat bonus"));
        assert_eq!(info.icon.as_deref(), Some("BTNRingPurple.blp"));
        assert_eq!(variant.editor_suffix().as_deref(), Some("(agility +1)"));
    }

    /// A base that names itself must not recurse.
    #[test]
    fn self_referential_code_does_not_recurse() {
        let mut source = source();
        source.insert(
            "Units\\HumanAbilityStrings.txt",
            "[AHbz]\nName=Blizzard\n".as_bytes().to_vec(),
        );
        let gd = GameData::load(&source).expect("load");
        let ability = gd.ability("AHbz").expect("ability");
        assert_eq!(ability.code(), Some("AHbz"));
        assert!(ability.base_code().is_none());
        assert!(ability.base().is_none());
        assert_eq!(ability.resolved_name().as_deref(), Some("Blizzard"));
    }
}
