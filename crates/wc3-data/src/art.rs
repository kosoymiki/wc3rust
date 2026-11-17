//! Model path helpers.
//!
//! The shipped tables store model paths with `.mdx` omitted
//! (`units\nightelf\Archer\Archer`) or with a different extension
//! (`treasurechest.mdl` for items, which is the ground model). Normalisation
//! therefore only completes a *missing* extension — it never rewrites one.

/// Convert a stored model path to the archive form (`\` separators, extension).
///
/// - empty → `None`
/// - no extension at all → `.mdx` is appended (the only model extension the
///   tables ever omit)
/// - `.mdl` and other extensions are preserved verbatim
pub fn normalize_model_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut path = trimmed.replace('/', "\\");
    let has_extension = path
        .rsplit('\\')
        .next()
        .map(|name| name.rsplit_once('.').is_some())
        .unwrap_or(false);
    if !has_extension {
        path.push_str(".mdx");
    }
    Some(path)
}

/// Whether a profile `Art` value is a texture path rather than a command alias.
///
/// The shipped files mix both: `ReplaceableTextures\\CommandButtons\\BTNFootman.blp`
/// is a path, while `CommandMove` / `CommandHarvest` name engine command icons
/// (`analysis/war3_data/Units/CommandFunc.txt` maps `[CmdMove]` -> `CommandMove`).
pub fn is_icon_path(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }
    let has_separator = value.contains('\\') || value.contains('/');
    let has_extension = value
        .rsplit('.')
        .next()
        .is_some_and(|ext| ext.len() <= 5 && !ext.is_empty());
    has_separator && has_extension
}

/// Conventional portrait model for a unit model (`<model>_Portrait.mdx`).
pub fn portrait_path(model: &str) -> Option<String> {
    let lowered = model.to_ascii_lowercase();
    let stem = lowered.strip_suffix(".mdx").unwrap_or(&lowered);
    Some(format!("{stem}_Portrait.mdx"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completes_missing_extension_only() {
        assert_eq!(
            normalize_model_path("units\\human\\Footman\\Footman").as_deref(),
            Some("units\\human\\Footman\\Footman.mdx")
        );
        assert_eq!(
            normalize_model_path("Objects\\InventoryItems\\TreasureChest\\treasurechest.mdl")
                .as_deref(),
            Some("Objects\\InventoryItems\\TreasureChest\\treasurechest.mdl")
        );
        assert_eq!(normalize_model_path("  "), None);
    }

    #[test]
    fn portrait_suffix() {
        assert_eq!(
            portrait_path("units\\human\\footman\\footman.mdx").as_deref(),
            Some("units\\human\\footman\\footman_Portrait.mdx")
        );
    }
}
