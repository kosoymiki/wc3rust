//! Doodads (`Doodads\Doodads.slk`): environmental decoration.

use wc3_formats::gamedata::FourCc;

use crate::error::{DataError, Result};
use crate::table::{Table, TableKind};
use crate::GameData;

/// A doodad type.
#[derive(Clone, Copy)]
pub struct DoodadView<'a> {
    pub(crate) gd: &'a GameData,
    pub(crate) id: FourCc,
}

impl<'a> DoodadView<'a> {
    pub fn id(&self) -> FourCc {
        self.id
    }

    pub fn id_str(&self) -> String {
        self.id.as_str().into_owned()
    }

    fn table(&self) -> Result<&'a Table> {
        self.gd
            .table(TableKind::Doodads)
            .ok_or(DataError::MissingColumn {
                table: "Doodads".to_string(),
                column: "doodID".to_string(),
            })
    }

    fn cell(&self, column: &str) -> Option<&'a str> {
        self.gd
            .table(TableKind::Doodads)?
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

    pub fn name(&self) -> Option<&'a str> {
        self.cell("name")
    }

    pub fn comment(&self) -> Option<&'a str> {
        self.cell("comment")
    }

    /// `category` — `Z` (doodad), `D` (destructable), `P` (pathing blocker) as stored.
    pub fn category(&self) -> Option<&'a str> {
        self.cell("category")
    }

    pub fn tilesets(&self) -> Option<&'a str> {
        self.cell("tilesets")
    }

    /// `doodClass` — `A`/`B` class flags as stored.
    pub fn doodad_class(&self) -> Option<&'a str> {
        self.cell("doodClass")
    }

    /// `dir` — model directory as stored (`Doodads\Ashenvale\Plants`).
    pub fn model_dir(&self) -> Option<&'a str> {
        self.cell("dir")
    }

    /// `file` — model file name as stored (`AshenShrooms`, no extension).
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

    /// `numVar` — number of model variations.
    pub fn variations(&self) -> Result<Option<i32>> {
        self.num("numVar")
    }

    pub fn default_scale(&self) -> Result<Option<f32>> {
        self.real("defScale")
    }

    pub fn min_scale(&self) -> Result<Option<f32>> {
        self.real("minScale")
    }

    pub fn max_scale(&self) -> Result<Option<f32>> {
        self.real("maxScale")
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

    pub fn floats(&self) -> Result<bool> {
        Ok(self.flag("floats")?.unwrap_or(false))
    }

    pub fn fixed_rotation(&self) -> Result<bool> {
        Ok(self.flag("fixedRot")?.unwrap_or(false))
    }

    pub fn visible_in_fog(&self) -> Result<bool> {
        Ok(self.flag("showInFog")?.unwrap_or(false))
    }

    pub fn animate_in_fog(&self) -> Result<bool> {
        Ok(self.flag("animInFog")?.unwrap_or(false))
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

    pub fn shadow(&self) -> Option<&'a str> {
        self.cell("shadow")
    }

    /// `soundLoop` — looping ambient sound.
    pub fn sound_loop(&self) -> Option<&'a str> {
        self.cell("soundLoop")
    }

    /// Terrain deformation colour of vertex `index` (0..9): `(r, g, b)`.
    pub fn vertex_color(&self, index: usize) -> Result<Option<(i32, i32, i32)>> {
        if index > 9 {
            return Ok(None);
        }
        let number = format!("{:02}", index + 1);
        let red = self.num(&format!("vertR{number}"))?;
        let green = self.num(&format!("vertG{number}"))?;
        let blue = self.num(&format!("vertB{number}"))?;
        Ok(match (red, green, blue) {
            (Some(r), Some(g), Some(b)) => Some((r, g, b)),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemorySource;

    #[test]
    fn reads_doodad_fields() {
        let mut source = MemorySource::new();
        source.insert(
            "Doodads\\Doodads.slk",
            b"ID;PWXL;N;E\nB;Y2;X7;D0\n\
C;Y1;X1;K\"doodID\"\nC;X2;K\"category\"\nC;X3;K\"file\"\nC;X4;K\"name\"\nC;X5;K\"numVar\"\n\
C;X6;K\"defScale\"\nC;X7;K\"walkable\"\n\
C;Y2;X1;K\"APct\"\nC;X2;K\"Z\"\nC;X3;K\"Doodads\\\\Terrain\\\\PottedPlant\\\\PottedPlant\"\n\
C;X4;K\"Potted Plant\"\nC;X5;K1\nC;X6;K1.0\nC;X7;K0\n"
                .to_vec(),
        );
        let gd = GameData::load(&source).expect("load");
        let doodad = gd.doodad("APct").expect("doodad");
        assert_eq!(doodad.name(), Some("Potted Plant"));
        assert_eq!(doodad.category(), Some("Z"));
        assert_eq!(doodad.variations().unwrap(), Some(1));
        assert_eq!(doodad.walkable().unwrap(), false);
    }
}
