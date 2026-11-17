//! Destructables (`DestructableData.slk`): trees, walls, bridges, debris.

use wc3_formats::gamedata::FourCc;

use crate::error::{DataError, Result};
use crate::table::{Table, TableKind};
use crate::GameData;

/// A destructable object type.
#[derive(Clone, Copy)]
pub struct DestructableView<'a> {
    pub(crate) gd: &'a GameData,
    pub(crate) id: FourCc,
}

impl<'a> DestructableView<'a> {
    pub fn id(&self) -> FourCc {
        self.id
    }

    pub fn id_str(&self) -> String {
        self.id.as_str().into_owned()
    }

    fn table(&self) -> Result<&'a Table> {
        self.gd
            .table(TableKind::DestructableData)
            .ok_or(DataError::MissingColumn {
                table: "DestructableData".to_string(),
                column: "DestructableID".to_string(),
            })
    }

    fn cell(&self, column: &str) -> Option<&'a str> {
        self.gd
            .table(TableKind::DestructableData)?
            .text(self.id.as_str().as_ref(), column)
    }

    fn num(&self, column: &str) -> Result<Option<i32>> {
        self.table()?.i32_overlaid(self.id.as_str().as_ref(), column)
    }

    fn real(&self, column: &str) -> Result<Option<f32>> {
        self.table()?.f32(self.id.as_str().as_ref(), column)
    }

    fn flag(&self, column: &str) -> Result<Option<bool>> {
        self.table()?.bool(self.id.as_str().as_ref(), column)
    }

    /// `name` — the SLK carries the English name directly (destructables are not
    /// covered by the profile text files).
    pub fn name(&self) -> Option<&'a str> {
        self.cell("name")
    }

    pub fn comment(&self) -> Option<&'a str> {
        self.cell("comment")
    }

    /// `category` — `B` (bridge/blocker) or `D` (destructable) as stored.
    pub fn category(&self) -> Option<&'a str> {
        self.cell("category")
    }

    /// `tilesets` — tilesets the object is valid on (`*` = all).
    pub fn tilesets(&self) -> Option<&'a str> {
        self.cell("tilesets")
    }

    /// `targType` — the gameplay class: `tree`, `wall`, `debris`, `decoration`, `bridge`.
    pub fn target_type(&self) -> Option<&'a str> {
        self.cell("targType")
    }

    /// `armor` — armour class (`Wood`, `Stone`).
    pub fn armor_type(&self) -> Option<&'a str> {
        self.cell("armor")
    }

    /// `dir` — model directory as stored (`Doodads\Terrain`).
    pub fn model_dir(&self) -> Option<&'a str> {
        self.cell("dir")
    }

    /// `file` — model file name as stored (`AshenTree`, no extension).
    pub fn model_name(&self) -> Option<&'a str> {
        self.cell("file")
    }

    /// Full model path: `dir` + `file` (the sheet splits them).
    pub fn model_path(&self) -> Option<String> {
        let name = self.model_name()?.trim();
        if name.is_empty() {
            return None;
        }
        match self.model_dir().map(str::trim).filter(|dir| !dir.is_empty()) {
            Some(dir) => Some(format!("{dir}\\{name}")),
            None => Some(name.to_string()),
        }
    }

    /// Plain model path (`dir\file\file.mdx`).
    pub fn model_file(&self) -> Option<String> {
        let dir = self.model_dir().map(str::trim).filter(|dir| !dir.is_empty());
        let name = self.model_name()?.trim();
        if name.is_empty() {
            return None;
        }
        let path = match dir {
            Some(dir) => format!("{dir}\\{name}\\{name}"),
            None => name.to_string(),
        };
        crate::art::normalize_model_path(&path)
    }

    /// Every model variation the sheet declares.
    ///
    /// Doodad/destructable models live in their own directory and carry a
    /// variation suffix (`Doodads\Ashenvale\Plants\AshenShrooms\AshenShrooms0.mdx`,
    /// `…1.mdx`, …), with `numVar` giving the count; some objects ship a single
    /// unsuffixed model. The plain path is tried first, then variations
    /// `0..numVar`.
    pub fn model_variants(&self) -> Vec<String> {
        let Some(base) = self.model_file() else {
            return Vec::new();
        };
        let mut out = vec![base.clone()];
        let count = self.variations().ok().flatten().unwrap_or(0).clamp(0, 64);
        let stem = base.strip_suffix(".mdx").unwrap_or(&base).to_string();
        for index in 0..count {
            let candidate = format!("{stem}{index}.mdx");
            if !out.contains(&candidate) {
                out.push(candidate);
            }
        }
        out
    }

    /// Whether any declared model variation exists in the archive.
    pub fn model_exists(&self) -> Option<bool> {
        let catalog = self.gd.catalog()?;
        let variants = self.model_variants();
        if variants.is_empty() {
            return None;
        }
        Some(
            variants
                .iter()
                .any(|variant| catalog.resolve_model(variant).is_some()),
        )
    }

    /// The first variation that resolves in the archive.
    pub fn resolved_model(&self) -> Option<String> {
        let catalog = self.gd.catalog()?;
        self.model_variants()
            .into_iter()
            .find_map(|variant| catalog.resolve_model(&variant))
    }

    /// `texFile` — replacement texture.
    pub fn texture(&self) -> Option<&'a str> {
        self.cell("texFile")
    }

    pub fn hp(&self) -> Result<Option<i32>> {
        self.num("HP")
    }

    /// `numVar` — number of model variations.
    pub fn variations(&self) -> Result<Option<i32>> {
        self.num("numVar")
    }

    pub fn radius(&self) -> Result<Option<f32>> {
        self.real("radius")
    }

    pub fn fog_radius(&self) -> Result<Option<f32>> {
        self.real("fogRadius")
    }

    pub fn fog_visibility(&self) -> Result<Option<i32>> {
        self.num("fogVis")
    }

    pub fn walkable(&self) -> Result<bool> {
        Ok(self.flag("walkable")?.unwrap_or(false))
    }

    pub fn on_cliffs(&self) -> Result<bool> {
        Ok(self.flag("onCliffs")?.unwrap_or(false))
    }

    pub fn on_water(&self) -> Result<bool> {
        Ok(self.flag("onWater")?.unwrap_or(false))
    }

    pub fn lightweight(&self) -> Result<bool> {
        Ok(self.flag("lightweight")?.unwrap_or(false))
    }

    pub fn fat_los(&self) -> Result<bool> {
        Ok(self.flag("fatLOS")?.unwrap_or(false))
    }

    pub fn show_in_minimap(&self) -> Result<bool> {
        Ok(self.flag("showInMM")?.unwrap_or(false))
    }

    /// Minimap colour as `(r, g, b)`.
    pub fn minimap_color(&self) -> Result<Option<(i32, i32, i32)>> {
        let red = self.num("MMRed")?;
        let green = self.num("MMGreen")?;
        let blue = self.num("MMBlue")?;
        Ok(match (red, green, blue) {
            (Some(r), Some(g), Some(b)) => Some((r, g, b)),
            _ => None,
        })
    }

    pub fn path_texture(&self) -> Option<&'a str> {
        self.cell("pathTex")
    }

    pub fn death_path_texture(&self) -> Option<&'a str> {
        self.cell("pathTexDeath")
    }

    pub fn death_sound(&self) -> Option<&'a str> {
        self.cell("deathSnd")
    }

    pub fn shadow(&self) -> Option<&'a str> {
        self.cell("shadow")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemorySource;

    fn source() -> MemorySource {
        let mut source = MemorySource::new();
        source.insert(
            "Units\\DestructableData.slk",
            b"ID;PWXL;N;E\nB;Y2;X8;D0\n\
C;Y1;X1;K\"DestructableID\"\nC;X2;K\"category\"\nC;X3;K\"targType\"\nC;X4;K\"armor\"\n\
C;X5;K\"file\"\nC;X6;K\"name\"\nC;X7;K\"HP\"\nC;X8;K\"numVar\"\n\
C;Y2;X1;K\"LTlt\"\nC;X2;K\"D\"\nC;X3;K\"tree\"\nC;X4;K\"Wood\"\n\
C;X5;K\"Doodads\\\\Terrain\\\\LordaeronSummer\\\\Trees\\\\LordaeronTree\\\\LordaeronTree\"\n\
C;X6;K\"Summer Tree Wall\"\nC;X7;K50\nC;X8;K10\n"
                .to_vec(),
        );
        source
    }

    #[test]
    fn reads_destructable_fields() {
        let gd = GameData::load(&source()).expect("load");
        let object = gd.destructable("LTlt").expect("destructable");
        assert_eq!(object.target_type(), Some("tree"));
        assert_eq!(object.armor_type(), Some("Wood"));
        assert_eq!(object.hp().unwrap(), Some(50));
        assert_eq!(object.variations().unwrap(), Some(10));
        assert!(object
            .model_file()
            .unwrap()
            .to_ascii_lowercase()
            .ends_with(".mdx"));
    }
}
