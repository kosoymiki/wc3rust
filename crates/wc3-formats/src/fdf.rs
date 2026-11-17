//! Warcraft III FDF (Frame Definition File) parser.
//!
//! FDF is the declarative UI language used by the game for every screen:
//! `Frame "KIND" "Name" { ... }`, nested `Texture`/`String`/`Backdrop`
//! declarations, `INHERITS` templates, `IncludeFile`, anchors, fonts and
//! texture rectangles. Values are fractions of the screen, which is exactly
//! what a phone port needs: the same definitions scale to any viewport.
//!
//! Syntax rules implemented here (verified against the retail corpus in
//! `analysis/war3_data`, 70 files / 1936 declarations / 6462 properties):
//!
//! * statements end at a newline; a block header ends at its `{`;
//! * only `Frame`, `String`, `Texture`, `Layer` and `StringList` open blocks;
//! * `IncludeFile` and `StringList` are document-level actions;
//! * `Frame "KIND" "NAME" [INHERITS [WITHCHILDREN] "BASE"] { ... }`;
//! * `Texture`/`String`/`Layer` take an optional name;
//! * property lines are `Key value[, value]*` (space or comma separated) and
//!   may be valueless flags (`DecorateFileNames,`);
//! * `//` line comments, `/* ... */` block comments and a UTF-8 BOM;
//! * repeated properties are preserved in source order (`SetPoint` is the
//!   important one: a frame can be anchored on several points).
//!
//! The parser is deliberately lossless: every property/value pair is kept in
//! [`FdfNode::props`] so the layout/render layer can consume new properties
//! without a parser change, while typed helpers cover the common ones.

use std::collections::HashMap;

use crate::FormatError;

#[derive(Debug, Clone, PartialEq)]
pub enum FdfValue {
    Ident(String),
    Str(String),
    Number(f32),
}

impl FdfValue {
    pub fn as_ident(&self) -> Option<&str> {
        match self {
            Self::Ident(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            Self::Str(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_number(&self) -> Option<f32> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FdfProp {
    pub key: String,
    pub values: Vec<FdfValue>,
}

impl FdfProp {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            values: Vec::new(),
        }
    }

    pub fn ident(&self, index: usize) -> Option<&str> {
        self.values.get(index).and_then(FdfValue::as_ident)
    }

    pub fn string(&self, index: usize) -> Option<&str> {
        self.values.get(index).and_then(FdfValue::as_string)
    }

    pub fn number(&self, index: usize) -> Option<f32> {
        self.values.get(index).and_then(FdfValue::as_number)
    }

    pub fn first_ident(&self) -> Option<&str> {
        self.values.iter().find_map(FdfValue::as_ident)
    }

    pub fn first_string(&self) -> Option<&str> {
        self.values.iter().find_map(FdfValue::as_string)
    }

    pub fn first_number(&self) -> Option<f32> {
        self.values.iter().find_map(FdfValue::as_number)
    }

    pub fn idents(&self) -> impl Iterator<Item = &str> + '_ {
        self.values.iter().filter_map(FdfValue::as_ident)
    }

    pub fn strings(&self) -> impl Iterator<Item = &str> + '_ {
        self.values.iter().filter_map(FdfValue::as_string)
    }

    pub fn numbers(&self) -> impl Iterator<Item = f32> + '_ {
        self.values.iter().filter_map(FdfValue::as_number)
    }

    /// A flag such as `DecorateFileNames,` has no values at all.
    pub fn is_flag(&self) -> bool {
        self.values.is_empty()
    }
}

/// One `SetPoint` property, already split into its five fields.
///
/// `target` is `None` when the FDF omits the frame name (or the property is
/// written with an empty name), which means "the parent frame" for nested
/// frames and "the screen" for a top-level frame.
#[derive(Debug, Clone, PartialEq)]
pub struct FdfSetPoint {
    pub point: String,
    pub target: Option<String>,
    pub target_point: String,
    pub x: f32,
    pub y: f32,
}

impl FdfSetPoint {
    pub fn parse(prop: &FdfProp) -> Option<Self> {
        let point = prop.ident(0)?.to_ascii_uppercase();
        let mut index = 1usize;
        // The target name is normally a string; accept an identifier too so a
        // malformed-but-readable file still lays out.
        let target = match prop.values.get(index) {
            Some(FdfValue::Str(name)) => {
                index += 1;
                Some(name.clone())
            }
            Some(FdfValue::Ident(name)) => {
                index += 1;
                Some(name.clone())
            }
            Some(FdfValue::Number(_)) => None,
            None => return None,
        };
        let target_point = prop.ident(index)?.to_ascii_uppercase();
        let x = prop.number(index + 1)?;
        let y = prop.number(index + 2)?;
        Some(Self {
            point,
            target: target.filter(|name| !name.is_empty()),
            target_point,
            x,
            y,
        })
    }

    /// The normalised horizontal fraction of the own point (0 = left edge,
    /// 0.5 = centre, 1 = right edge).
    pub fn point_x(&self) -> f32 {
        point_fraction(&self.point).0
    }

    /// The normalised vertical fraction of the own point (0 = top edge,
    /// 0.5 = centre, 1 = bottom edge).
    pub fn point_y(&self) -> f32 {
        point_fraction(&self.point).1
    }
}

/// `FrameFont`/`Font` reference: family name, size fraction of screen height
/// and the optional third argument used by some retail templates.
#[derive(Debug, Clone, PartialEq)]
pub struct FdfFontRef {
    pub name: String,
    pub size: f32,
    pub extra: Option<String>,
    /// `"FrameFont"` or `"Font"` — kept for diagnostics.
    pub property: &'static str,
}

#[derive(Debug, Clone, Default)]
pub struct FdfNode {
    /// `"Frame"` declarations store their first string here (`TEXT`,
    /// `BACKDROP`, ...); `String`/`Texture`/`Layer` store the keyword itself.
    pub kind: String,
    /// Empty string names are legal and common for decorative child frames.
    pub name: Option<String>,
    pub inherits: Option<String>,
    /// `INHERITS WITHCHILDREN` also copies the template's child frames.
    pub inherit_children: bool,
    pub props: Vec<FdfProp>,
    pub children: Vec<FdfNode>,
}

impl FdfNode {
    pub fn prop(&self, key: &str) -> Option<&FdfProp> {
        self.props
            .iter()
            .find(|prop| prop.key.eq_ignore_ascii_case(key))
    }

    /// Every property with this key, in source order (`SetPoint`, `Anchor`,
    /// `MenuItem`, ... can be repeated).
    pub fn props_named<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a FdfProp> + 'a {
        self.props
            .iter()
            .filter(move |prop| prop.key.eq_ignore_ascii_case(key))
    }

    pub fn has(&self, key: &str) -> bool {
        self.prop(key).is_some()
    }

    /// A property that is present without values (or with a nonzero value).
    pub fn flag(&self, key: &str) -> bool {
        match self.prop(key) {
            Some(prop) if prop.is_flag() => true,
            Some(prop) => prop.number(0).map(|value| value != 0.0).unwrap_or(true),
            None => false,
        }
    }

    pub fn number(&self, key: &str) -> Option<f32> {
        self.prop(key).and_then(|prop| prop.number(0))
    }

    pub fn string(&self, key: &str) -> Option<&str> {
        self.prop(key).and_then(|prop| prop.string(0))
    }

    pub fn ident(&self, key: &str) -> Option<&str> {
        self.prop(key).and_then(|prop| prop.ident(0))
    }

    pub fn width(&self) -> Option<f32> {
        self.number("Width")
    }

    pub fn height(&self) -> Option<f32> {
        self.number("Height")
    }

    /// `FrameFont "MasterFont",0.013,""` first, then the older
    /// `Font "MasterFont",0.01` spelling.
    pub fn font(&self) -> Option<FdfFontRef> {
        if let Some(prop) = self.prop("FrameFont") {
            let name = prop.string(0)?.to_string();
            let size = prop.number(1)?;
            return Some(FdfFontRef {
                name,
                size,
                extra: prop.string(2).map(str::to_string),
                property: "FrameFont",
            });
        }
        let prop = self.prop("Font")?;
        let name = prop.string(0)?.to_string();
        let size = prop.number(1)?;
        Some(FdfFontRef {
            name,
            size,
            extra: prop.string(2).map(str::to_string),
            property: "Font",
        })
    }

    pub fn font_color(&self) -> Option<[f32; 4]> {
        parse_color(self.prop("FontColor")?)
    }

    pub fn font_highlight_color(&self) -> Option<[f32; 4]> {
        parse_color(self.prop("FontHighlightColor")?)
    }

    pub fn font_disabled_color(&self) -> Option<[f32; 4]> {
        parse_color(self.prop("FontDisabledColor")?)
    }

    pub fn font_shadow_color(&self) -> Option<[f32; 4]> {
        parse_color(self.prop("FontShadowColor")?)
    }

    /// `FontShadowOffset x y` in screen fractions.
    pub fn font_shadow_offset(&self) -> Option<[f32; 2]> {
        let prop = self.prop("FontShadowOffset")?;
        Some([prop.number(0)?, prop.number(1)?])
    }

    pub fn font_justification_h(&self) -> Option<&str> {
        self.prop("FontJustificationH")
            .and_then(|prop| prop.ident(0).or_else(|| prop.string(0)))
    }

    pub fn font_justification_v(&self) -> Option<&str> {
        self.prop("FontJustificationV")
            .and_then(|prop| prop.ident(0).or_else(|| prop.string(0)))
    }

    pub fn font_flags(&self) -> Option<&str> {
        self.prop("FontFlags")
            .and_then(|prop| prop.ident(0).or_else(|| prop.string(0)))
    }

    /// Raw `TexCoord left,right,top,bottom` order, as written in FDF.
    pub fn tex_coord(&self) -> Option<[f32; 4]> {
        let prop = self.prop("TexCoord")?;
        Some([
            prop.number(0)?,
            prop.number(1)?,
            prop.number(2)?,
            prop.number(3)?,
        ])
    }

    /// The same rectangle in `[u0, v0, u1, v1]` order for a screen-space
    /// quad, where V grows downwards.
    pub fn texture_uv(&self) -> Option<[f32; 4]> {
        let t = self.tex_coord()?;
        Some([t[0], t[2], t[1], t[3]])
    }

    /// Old `Anchor POINT,x,y` shorthand. The engine treats it as a
    /// `SetPoint` of the same point on the parent frame.
    pub fn anchor(&self) -> Option<(String, f32, f32)> {
        let prop = self.prop("Anchor")?;
        Some((
            prop.ident(0)?.to_ascii_uppercase(),
            prop.number(1)?,
            prop.number(2)?,
        ))
    }

    pub fn set_points(&self) -> Vec<FdfSetPoint> {
        self.props_named("SetPoint")
            .filter_map(FdfSetPoint::parse)
            .collect()
    }

    pub fn has_set_all_points(&self) -> bool {
        self.has("SetAllPoints")
    }

    /// `SetAllPoints` optionally names the frame to stretch to.
    pub fn set_all_points_target(&self) -> Option<&str> {
        self.prop("SetAllPoints").and_then(|prop| prop.string(0))
    }

    pub fn text(&self) -> Option<&str> {
        self.string("Text")
    }

    pub fn button_text(&self) -> Option<&str> {
        self.string("ButtonText")
    }

    /// `NormalText "TextTemplate" "LOCALIZED_KEY"`.
    pub fn normal_text(&self) -> Option<(&str, &str)> {
        let prop = self.prop("NormalText")?;
        Some((prop.string(0)?, prop.string(1)?))
    }

    /// `DisabledText "TextTemplate" "LOCALIZED_KEY"`.
    pub fn disabled_text(&self) -> Option<(&str, &str)> {
        let prop = self.prop("DisabledText")?;
        Some((prop.string(0)?, prop.string(1)?))
    }

    /// `PushedText`/`ButtonPushedText` are uncommon but legal.
    pub fn pushed_text(&self) -> Option<(&str, &str)> {
        let prop = self.prop("PushedText")?;
        Some((prop.string(0)?, prop.string(1)?))
    }

    pub fn alpha_mode(&self) -> Option<&str> {
        self.prop("AlphaMode")
            .and_then(|prop| prop.ident(0).or_else(|| prop.string(0)))
    }

    pub fn is_kind(&self, kind: &str) -> bool {
        self.kind.eq_ignore_ascii_case(kind)
    }

    pub fn walk(&self, visit: &mut impl FnMut(&FdfNode)) {
        visit(self);
        for child in &self.children {
            child.walk(visit);
        }
    }

    /// `(declarations, properties)` including this node.
    pub fn count(&self) -> (usize, usize) {
        let mut nodes = 1;
        let mut props = self.props.len();
        for child in &self.children {
            let (child_nodes, child_props) = child.count();
            nodes += child_nodes;
            props += child_props;
        }
        (nodes, props)
    }

    /// The node and its descendants, with the effective
    /// `DecorateFileNames` context (the flag is inherited by child frames).
    pub fn walk_with_decoration(&self, inherited: bool, visit: &mut impl FnMut(&FdfNode, bool)) {
        let decorated = inherited || self.has("DecorateFileNames");
        visit(self, decorated);
        for child in &self.children {
            child.walk_with_decoration(decorated, visit);
        }
    }

    pub fn max_depth(&self) -> usize {
        1 + self
            .children
            .iter()
            .map(FdfNode::max_depth)
            .max()
            .unwrap_or(0)
    }

    /// Ordered `StringList` entries declared by this node.
    pub fn string_list_entries(&self, out: &mut Vec<(String, String)>) {
        if !self.is_kind("StringList") {
            return;
        }
        for prop in &self.props {
            if let Some(value) = prop.string(0) {
                out.push((prop.key.clone(), value.to_string()));
            }
        }
    }
}

/// FDF properties whose string value names an archive texture file. Inside a
/// `DecorateFileNames` context the value is a `war3skins.txt` key and the skin
/// owner resolves it; outside, it is a literal archive path.
pub const TEXTURE_FILE_PROPERTIES: &[&str] = &[
    "File",
    "BackdropBackground",
    "BackdropEdgeFile",
    "BackdropCornerFile",
    "BackdropLeftFile",
    "BackdropRightFile",
    "BackdropTopFile",
    "BackdropBottomFile",
    "HighlightAlphaFile",
    "StatusBarTexture",
];

/// FDF properties that name a child `Texture` declaration (a template), not an
/// archive file. They must not be sent through `war3skins` decoration.
pub const TEXTURE_TEMPLATE_PROPERTIES: &[&str] = &[
    "NormalTexture",
    "PushedTexture",
    "DisabledTexture",
    "UseHighlight",
];

/// FDF properties whose string value names an MDX model (skin key when
/// decorated).
pub const MODEL_PROPERTIES: &[&str] = &["BackgroundArt"];

/// One texture/model reference found in a document, with the effective
/// `DecorateFileNames` context. `template_reference` is true for
/// `NormalTexture`/`PushedTexture`/`DisabledTexture`/`UseHighlight`, which
/// point at a `Texture` declaration instead of an archive file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FdfTextureRef {
    pub frame: String,
    pub property: String,
    pub value: String,
    pub decorated: bool,
    pub template_reference: bool,
}

/// Aggregate counts used by the canonical audit (`wc3-re::skin` / `wc3re scan`)
/// and by the FDF layer tests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FdfAudit {
    pub nodes: usize,
    pub props: usize,
    pub includes: usize,
    /// Nodes that declare `DecorateFileNames`.
    pub decorate_nodes: usize,
    /// Nodes that inherit a decoration context (self or ancestor).
    pub decorated_nodes: usize,
    pub texture_refs: usize,
    pub decorated_texture_refs: usize,
    pub template_texture_refs: usize,
    pub model_refs: usize,
    pub string_list_entries: usize,
    pub inherit_statements: usize,
    pub with_children: usize,
    pub font_refs: usize,
    pub text_values: usize,
    pub max_depth: usize,
}

#[derive(Debug, Clone, Default)]
pub struct FdfDocument {
    pub nodes: Vec<FdfNode>,
    pub includes: Vec<String>,
    /// Source path used by the UI loader for include de-duplication.
    pub includer: String,
}

impl FdfDocument {
    pub fn parse(text: &str) -> Result<Self, FormatError> {
        Self::parse_named(text, "")
    }

    pub fn parse_named(text: &str, includer: impl Into<String>) -> Result<Self, FormatError> {
        let tokens = lex(text)?;
        let mut parser = Parser {
            tokens,
            position: 0,
        };
        let mut document = FdfDocument {
            nodes: Vec::new(),
            includes: Vec::new(),
            includer: includer.into(),
        };
        parser.parse_document(&mut document)?;
        Ok(document)
    }

    /// Look up a top-level template/frame by name (first declaration wins;
    /// retail FDF aborts a file when a main frame name is redeclared).
    pub fn find(&self, name: &str) -> Option<&FdfNode> {
        self.nodes
            .iter()
            .find(|node| node.name.as_deref() == Some(name))
    }

    /// Look up a top-level node with an exact name; empty names are skipped.
    pub fn find_ci(&self, name: &str) -> Option<&FdfNode> {
        self.nodes.iter().find(|node| {
            node.name
                .as_deref()
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
        })
    }

    /// `(declarations, properties)` over the whole document.
    pub fn count(&self) -> (usize, usize) {
        let mut nodes = 0;
        let mut props = 0;
        for node in &self.nodes {
            let (node_count, prop_count) = node.count();
            nodes += node_count;
            props += prop_count;
        }
        (nodes, props)
    }

    pub fn walk(&self, visit: &mut impl FnMut(&FdfNode)) {
        for node in &self.nodes {
            node.walk(visit);
        }
    }

    /// Collect `StringList { KEY "value", ... }` entries into `out`, keeping
    /// declaration order and duplicate keys (the retail loader applies them in
    /// order; the canonical text view exposes last-wins plus `get_all`).
    pub fn collect_strings(&self, out: &mut Vec<(String, String)>) {
        self.string_list_entries_into(out);
    }

    pub fn string_list_entries(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        self.string_list_entries_into(&mut out);
        out
    }

    fn string_list_entries_into(&self, out: &mut Vec<(String, String)>) {
        self.walk(&mut |node| node.string_list_entries(out));
    }

    /// Every texture/model reference with its effective decoration context.
    pub fn texture_refs(&self) -> Vec<FdfTextureRef> {
        let mut out = Vec::new();
        for node in &self.nodes {
            node.walk_with_decoration(false, &mut |node, decorated| {
                let frame = node.name.clone().unwrap_or_default();
                for prop in &node.props {
                    let key = prop.key.as_str();
                    let template_reference = TEXTURE_TEMPLATE_PROPERTIES
                        .iter()
                        .any(|name| name.eq_ignore_ascii_case(key));
                    let is_texture = template_reference
                        || TEXTURE_FILE_PROPERTIES
                            .iter()
                            .any(|name| name.eq_ignore_ascii_case(key))
                        || MODEL_PROPERTIES
                            .iter()
                            .any(|name| name.eq_ignore_ascii_case(key));
                    if !is_texture {
                        continue;
                    }
                    let Some(value) = prop.string(0) else {
                        continue;
                    };
                    out.push(FdfTextureRef {
                        frame: frame.clone(),
                        property: key.to_string(),
                        value: value.to_string(),
                        decorated: decorated && !template_reference,
                        template_reference,
                    });
                }
            });
        }
        out
    }

    /// Every `FrameFont`/`Font` reference with the containing frame name.
    pub fn font_refs(&self) -> Vec<(String, FdfFontRef)> {
        let mut out = Vec::new();
        self.walk(&mut |node| {
            if let Some(font) = node.font() {
                out.push((node.name.clone().unwrap_or_default(), font));
            }
        });
        out
    }

    /// Every `Text` property value with the containing frame name.
    pub fn text_values(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        self.walk(&mut |node| {
            if let Some(text) = node.text() {
                out.push((node.name.clone().unwrap_or_default(), text.to_string()));
            }
        });
        out
    }

    /// Aggregate audit for this document. The canonical skin/text audits
    /// consume it instead of walking `FdfNode` fields themselves.
    pub fn audit(&self) -> FdfAudit {
        let (nodes, props) = self.count();
        let mut audit = FdfAudit {
            nodes,
            props,
            includes: self.includes.len(),
            string_list_entries: self.string_list_entries().len(),
            text_values: self.text_values().len(),
            font_refs: self.font_refs().len(),
            ..FdfAudit::default()
        };
        for node in &self.nodes {
            audit.max_depth = audit.max_depth.max(node.max_depth());
            node.walk_with_decoration(false, &mut |node, decorated| {
                if node.has("DecorateFileNames") {
                    audit.decorate_nodes += 1;
                }
                if decorated {
                    audit.decorated_nodes += 1;
                }
                if node.inherits.is_some() {
                    audit.inherit_statements += 1;
                }
                if node.inherit_children {
                    audit.with_children += 1;
                }
            });
        }
        for reference in self.texture_refs() {
            if reference.template_reference {
                audit.template_texture_refs += 1;
            } else if MODEL_PROPERTIES
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&reference.property))
            {
                audit.model_refs += 1;
            } else {
                audit.texture_refs += 1;
                if reference.decorated {
                    audit.decorated_texture_refs += 1;
                }
            }
        }
        audit
    }
}

/// Ordered FDF game-string table with retail resolution rules.
///
/// `StringList` blocks are parsed by the FDF document model (this module), so
/// the text view has exactly one parser and keeps declaration order and
/// duplicate keys. `COLON_`/`KEY_` fallbacks, `TRIGSTR_n` substitution and
/// escape decoding live here as well; `wc3-re::txt` re-exports this type as its
/// canonical ordered text view and the HUD aliases it as `Localization`.
#[derive(Debug, Clone, Default)]
pub struct FdfText {
    entries: Vec<(String, String)>,
    index: HashMap<String, Vec<usize>>,
    wts: HashMap<u32, String>,
}

impl FdfText {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_document(document: &FdfDocument) -> Self {
        let mut text = Self::new();
        text.merge_document(document);
        text
    }

    pub fn from_documents<'a>(documents: impl IntoIterator<Item = &'a FdfDocument>) -> Self {
        let mut text = Self::new();
        for document in documents {
            text.merge_document(document);
        }
        text
    }

    pub fn parse_text(text: &str, name: impl Into<String>) -> Result<Self, FormatError> {
        Ok(Self::from_document(&FdfDocument::parse_named(text, name)?))
    }

    pub fn merge_document(&mut self, document: &FdfDocument) -> usize {
        let entries = document.string_list_entries();
        let count = entries.len();
        for (key, value) in entries {
            self.insert(key, value);
        }
        count
    }

    pub fn parse_string_list(&mut self, document: &FdfDocument) -> usize {
        self.merge_document(document)
    }

    pub fn parse_string_list_text(&mut self, text: &str) -> Result<usize, FormatError> {
        let document = FdfDocument::parse(text)?;
        Ok(self.merge_document(&document))
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let position = self.entries.len();
        self.entries.push((key.clone(), value.into()));
        self.index.entry(key).or_default().push(position);
    }

    pub fn merge<I, K, V>(&mut self, entries: I)
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (key, value) in entries {
            self.insert(key, value);
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Last value for a key (exact, then ASCII-uppercase fallback).
    pub fn get(&self, key: &str) -> Option<&str> {
        if let Some(indices) = self.index.get(key) {
            return indices.last().map(|&index| self.entries[index].1.as_str());
        }
        self.index
            .get(&key.to_ascii_uppercase())
            .and_then(|indices| indices.last())
            .map(|&index| self.entries[index].1.as_str())
    }

    pub fn get_all(&self, key: &str) -> Vec<&str> {
        self.index
            .get(key)
            .or_else(|| self.index.get(&key.to_ascii_uppercase()))
            .map(|indices| {
                indices
                    .iter()
                    .map(|&index| self.entries[index].1.as_str())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(key, _)| key.as_str())
    }

    /// Retail key fallback: exact, then `COLON_X` -> `X`, `KEY_X` -> `X`.
    pub fn resolve(&self, key: &str) -> Option<&str> {
        if let Some(value) = self.get(key) {
            return Some(value);
        }
        if let Some(base) = key.strip_prefix("COLON_") {
            return self.get(base);
        }
        if let Some(base) = key.strip_prefix("KEY_") {
            return self.get(base);
        }
        None
    }

    pub fn resolve_owned(&self, key: &str) -> Option<String> {
        if let Some(value) = self.get(key) {
            return Some(value.to_string());
        }
        if let Some(base) = key.strip_prefix("COLON_")
            && let Some(value) = self.get(base)
        {
            return Some(if value.ends_with(':') {
                value.to_string()
            } else {
                format!("{value}:")
            });
        }
        if let Some(base) = key.strip_prefix("KEY_") {
            return self.get(base).map(str::to_string);
        }
        None
    }

    /// Resolve an FDF `Text` value: `TRIGSTR_n`, a game-string key, or the
    /// literal string. Escape codes are preserved for the shaper.
    pub fn text(&self, raw: &str) -> String {
        let raw = raw.trim_matches('\u{feff}');
        if raw.is_empty() {
            return String::new();
        }
        if let Some(rest) = raw.strip_prefix("TRIGSTR_")
            && let Ok(id) = rest.trim().parse::<u32>()
            && let Some(value) = self.wts.get(&id)
        {
            return value.clone();
        }
        self.resolve_owned(raw).unwrap_or_else(|| raw.to_string())
    }

    pub fn plain(&self, raw: &str) -> String {
        strip_escape_codes(&self.text(raw))
    }

    /// Merge a `war3map.wts` table through the canonical
    /// `wc3-formats::gamedata::Wts` parser. Returns the number of entries.
    pub fn parse_wts(&mut self, text: &str) -> usize {
        // `wc3-formats::gamedata::Wts` is the canonical parser; retail WTS
        // files sometimes carry an inline comment on the `STRING n` line,
        // which the strict parser rejects. Normalise only that line shape and
        // keep the block body untouched.
        let mut cleaned = String::with_capacity(text.len());
        for line in text.lines() {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("STRING ") {
                let digits: String = rest
                    .chars()
                    .take_while(|character| character.is_ascii_digit())
                    .collect();
                if !digits.is_empty() {
                    cleaned.push_str("STRING ");
                    cleaned.push_str(&digits);
                    cleaned.push('\n');
                    continue;
                }
            }
            cleaned.push_str(line);
            cleaned.push('\n');
        }
        let Ok(wts) = crate::gamedata::Wts::parse(cleaned.as_bytes()) else {
            return 0;
        };
        let mut count = 0;
        for (id, value) in wts.iter() {
            self.wts.insert(id, value.to_string());
            count += 1;
        }
        count
    }

    pub fn wts(&self, id: u32) -> Option<&str> {
        self.wts.get(&id).map(String::as_str)
    }

    pub fn wts_len(&self) -> usize {
        self.wts.len()
    }
}

/// Remove WC3 colour/newline escape codes (`|cAARRGGBB`, `|r`, `|n`, `||`).
pub fn strip_escape_codes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '|' {
            out.push(character);
            continue;
        }
        match chars.next() {
            Some('n') | Some('N') => out.push('\n'),
            Some('c') | Some('C') => {
                for _ in 0..8 {
                    chars.next();
                }
            }
            Some('r') | Some('R') => {}
            Some('|') => out.push('|'),
            Some(other) => {
                out.push('|');
                out.push(other);
            }
            None => out.push('|'),
        }
    }
    out
}

fn parse_color(prop: &FdfProp) -> Option<[f32; 4]> {
    let r = prop.number(0)?;
    let g = prop.number(1)?;
    let b = prop.number(2)?;
    let a = prop.number(3).unwrap_or(1.0);
    Some([r, g, b, a])
}

/// Normalised point name -> `(x, y)` fractions inside a rectangle.
pub fn point_fraction(point: &str) -> (f32, f32) {
    match point.to_ascii_uppercase().as_str() {
        "TOPLEFT" => (0.0, 0.0),
        "TOP" | "TOPCENTER" => (0.5, 0.0),
        "TOPRIGHT" => (1.0, 0.0),
        "LEFT" | "MIDLEFT" => (0.0, 0.5),
        "CENTER" | "MIDCENTER" => (0.5, 0.5),
        "RIGHT" | "MIDRIGHT" => (1.0, 0.5),
        "BOTTOMLEFT" => (0.0, 1.0),
        "BOTTOM" | "BOTTOMCENTER" => (0.5, 1.0),
        "BOTTOMRIGHT" => (1.0, 1.0),
        _ => (0.0, 0.0),
    }
}

fn is_block_keyword(keyword: &str) -> bool {
    matches!(
        keyword.to_ascii_uppercase().as_str(),
        "FRAME" | "STRING" | "TEXTURE" | "LAYER" | "STRINGLIST"
    )
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    Str(String),
    Number(f32),
    LBrace,
    RBrace,
    Comma,
    Newline,
}

fn lex(text: &str) -> Result<Vec<Token>, FormatError> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        index = 3;
    }
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b' ' | b'\t' | b'\r' => index += 1,
            b'\n' => {
                tokens.push(Token::Newline);
                index += 1;
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index += 2;
                let mut closed = false;
                while index + 1 < bytes.len() {
                    if bytes[index] == b'*' && bytes[index + 1] == b'/' {
                        index += 2;
                        closed = true;
                        break;
                    }
                    index += 1;
                }
                if !closed {
                    return Err(FormatError::Corrupt("unterminated FDF block comment"));
                }
            }
            b'{' => {
                tokens.push(Token::LBrace);
                index += 1;
            }
            b'}' => {
                tokens.push(Token::RBrace);
                index += 1;
            }
            b',' => {
                tokens.push(Token::Comma);
                index += 1;
            }
            b'"' => {
                index += 1;
                let mut value = String::new();
                let mut closed = false;
                while index < bytes.len() {
                    let current = bytes[index];
                    if current == b'"' {
                        index += 1;
                        closed = true;
                        break;
                    }
                    if current == b'\\' {
                        match bytes.get(index + 1) {
                            Some(b'"') => {
                                value.push('"');
                                index += 2;
                                continue;
                            }
                            Some(b'\\') => {
                                value.push('\\');
                                index += 2;
                                continue;
                            }
                            _ => {
                                // FDF paths use a single backslash; it is not an
                                // escape character. Keep it verbatim.
                                value.push('\\');
                                index += 1;
                                continue;
                            }
                        }
                    }
                    // Copy one UTF-8 scalar (localised StringLists contain
                    // Cyrillic and CJK text).
                    let rest = &text[index..];
                    let character = rest
                        .chars()
                        .next()
                        .ok_or(FormatError::Corrupt("invalid UTF-8 in FDF string"))?;
                    value.push(character);
                    index += character.len_utf8();
                }
                if !closed {
                    return Err(FormatError::Corrupt("unterminated FDF string"));
                }
                tokens.push(Token::Str(value));
            }
            b'-' | b'+' | b'.' | b'0'..=b'9' => {
                let start = index;
                if bytes[index] == b'-' || bytes[index] == b'+' {
                    index += 1;
                }
                let mut digits = false;
                while index < bytes.len() && bytes[index].is_ascii_digit() {
                    index += 1;
                    digits = true;
                }
                if index < bytes.len() && bytes[index] == b'.' {
                    index += 1;
                    while index < bytes.len() && bytes[index].is_ascii_digit() {
                        index += 1;
                        digits = true;
                    }
                }
                if !digits {
                    return Err(FormatError::Corrupt("invalid FDF number"));
                }
                if index < bytes.len() && (bytes[index] == b'e' || bytes[index] == b'E') {
                    let exponent = index;
                    index += 1;
                    if index < bytes.len() && (bytes[index] == b'-' || bytes[index] == b'+') {
                        index += 1;
                    }
                    let mut exponent_digits = false;
                    while index < bytes.len() && bytes[index].is_ascii_digit() {
                        index += 1;
                        exponent_digits = true;
                    }
                    if !exponent_digits {
                        index = exponent;
                    }
                }
                if index < bytes.len() && (bytes[index] == b'f' || bytes[index] == b'F') {
                    index += 1;
                }
                let number_text = text[start..index].trim_end_matches(['f', 'F']);
                let number = number_text
                    .parse::<f32>()
                    .map_err(|_| FormatError::Corrupt("invalid FDF number"))?;
                tokens.push(Token::Number(number));
            }
            _ => {
                let start = index;
                let character = text[index..]
                    .chars()
                    .next()
                    .ok_or(FormatError::Corrupt("invalid FDF token"))?;
                if !(character.is_ascii_alphabetic() || character == '_') {
                    return Err(FormatError::Corrupt("unexpected byte in FDF"));
                }
                index += character.len_utf8();
                while index < bytes.len() {
                    let current = bytes[index];
                    if current.is_ascii_alphanumeric()
                        || current == b'_'
                        || current == b'.'
                        || current == b'\\'
                    {
                        index += 1;
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Ident(text[start..index].to_string()));
            }
        }
    }
    Ok(tokens)
}

struct Statement {
    /// Token index of the first header token.
    start: usize,
    /// `Some(index)` when the statement opens a block.
    brace: Option<usize>,
    /// Index of the terminating newline/`}`/EOF (exclusive of the terminator).
    end: usize,
}

struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    fn at_end(&self) -> bool {
        self.position >= self.tokens.len()
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn peek_at(&self, index: usize) -> Option<&Token> {
        self.tokens.get(index)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.position).cloned();
        if token.is_some() {
            self.position += 1;
        }
        token
    }

    fn skip_separators(&mut self) {
        while matches!(self.peek(), Some(Token::Newline) | Some(Token::Comma)) {
            self.position += 1;
        }
    }

    fn parse_document(&mut self, document: &mut FdfDocument) -> Result<(), FormatError> {
        loop {
            self.skip_separators();
            if self.at_end() {
                return Ok(());
            }
            match self.peek() {
                Some(Token::RBrace) => {
                    return Err(FormatError::Corrupt("unexpected '}' at FDF document level"));
                }
                Some(Token::Ident(_)) => {}
                _ => {
                    return Err(FormatError::Corrupt(
                        "unexpected token at FDF document level",
                    ));
                }
            }
            let statement = self.scan_statement();
            // A block statement always carries its `{`; the include branch and
            // the node branch below are the only users, so bind it once. Every
            // token slice goes through `get` — a malformed brace position must
            // return the typed error, never slice out of range.
            let Some(brace) = statement.brace else {
                // `IncludeFile "path"` is the only non-block document action.
                if self.is_include(&statement) {
                    let path = self.include_path(&statement)?;
                    document.includes.push(path);
                    self.position = statement.end;
                    continue;
                }
                return Err(FormatError::Corrupt("expected FDF declaration"));
            };
            if self.is_include(&statement) {
                let path = self.include_path(&statement)?;
                document.includes.push(path);
                self.position = brace.saturating_add(1);
                self.skip_block()?;
                continue;
            }
            let header = self
                .tokens
                .get(statement.start..brace)
                .ok_or(FormatError::Corrupt("FDF declaration header out of range"))?
                .to_vec();
            let mut node = header_to_node(&header)?;
            self.position = brace.saturating_add(1);
            self.parse_body(&mut node, document)?;
            document.nodes.push(node);
        }
    }

    fn is_include(&self, statement: &Statement) -> bool {
        matches!(
            self.peek_at(statement.start),
            Some(Token::Ident(keyword)) if keyword.eq_ignore_ascii_case("IncludeFile")
        )
    }

    fn include_path(&self, statement: &Statement) -> Result<String, FormatError> {
        for index in statement.start + 1..statement.end {
            if let Some(Token::Str(path)) = self.peek_at(index) {
                return Ok(path.replace('/', "\\"));
            }
        }
        Err(FormatError::Corrupt("IncludeFile without a path"))
    }

    /// Skip a block that was opened but is not represented as a node.
    fn skip_block(&mut self) -> Result<(), FormatError> {
        let mut depth = 1usize;
        while let Some(token) = self.next() {
            match token {
                Token::LBrace => depth += 1,
                Token::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
        Err(FormatError::Corrupt("unterminated FDF block"))
    }

    /// Scan the statement starting at `position` without consuming it.
    ///
    /// A statement is a block declaration when a `{` is reached before the
    /// end of the line. A `{` on the following line is accepted only when the
    /// header starts with a known block keyword, so a valueless flag followed
    /// by a `Texture {` on the next line stays a property.
    ///
    /// FDF also allows several comma-terminated actions on one line
    /// (`Width 0.25, Height 0.25,`). A comma followed by a CamelCase action
    /// name starts the next action; action names are CamelCase while value
    /// identifiers are upper case (`TOPLEFT`, `JUSTIFYLEFT`, `ADD`), which is
    /// what makes the split unambiguous without an arity table.
    fn scan_statement(&self) -> Statement {
        let start = self.position;
        let line_end = self.line_end(start);
        if let Some(brace) = self.first_brace(start, line_end) {
            return Statement {
                start,
                brace: Some(brace),
                end: brace,
            };
        }
        let mut look = line_end;
        while matches!(self.peek_at(look), Some(Token::Newline)) {
            look += 1;
        }
        if matches!(self.peek_at(look), Some(Token::LBrace))
            && self.header_is_block_keyword(start, line_end)
        {
            return Statement {
                start,
                brace: Some(look),
                end: line_end,
            };
        }
        let mut index = start + 1;
        while index < line_end {
            if matches!(self.peek_at(index), Some(Token::Comma)) {
                let mut next = index + 1;
                while matches!(self.peek_at(next), Some(Token::Comma)) {
                    next += 1;
                }
                if let Some(Token::Ident(name)) = self.peek_at(next)
                    && name.chars().any(|character| character.is_ascii_lowercase())
                {
                    return Statement {
                        start,
                        brace: None,
                        end: index,
                    };
                }
            }
            index += 1;
        }
        Statement {
            start,
            brace: None,
            end: line_end,
        }
    }

    fn line_end(&self, start: usize) -> usize {
        let mut index = start;
        while let Some(token) = self.tokens.get(index) {
            match token {
                Token::Newline | Token::RBrace => return index,
                _ => index += 1,
            }
        }
        index
    }

    fn first_brace(&self, start: usize, end: usize) -> Option<usize> {
        (start..end).find(|&index| matches!(self.tokens.get(index), Some(Token::LBrace)))
    }

    fn header_is_block_keyword(&self, start: usize, end: usize) -> bool {
        match self.peek_at(start) {
            Some(Token::Ident(keyword)) if is_block_keyword(keyword) => {
                // A property whose key happens to match a block keyword (none
                // in the retail corpus) still must not swallow a later block;
                // only treat it as a header when the line has no values that
                // are not name/inherits tokens.
                let mut strings = 0usize;
                let after_keyword = start.saturating_add(1);
                let body = self
                    .tokens
                    .get(after_keyword..end)
                    .unwrap_or_default();
                for token in body {
                    match token {
                        Token::Str(_) => strings += 1,
                        Token::Ident(ident) => {
                            if !ident.eq_ignore_ascii_case("INHERITS")
                                && !ident.eq_ignore_ascii_case("WITHCHILDREN")
                            {
                                return false;
                            }
                        }
                        Token::Comma => {}
                        _ => return false,
                    }
                }
                strings <= 3
            }
            _ => false,
        }
    }

    fn parse_body(
        &mut self,
        node: &mut FdfNode,
        document: &mut FdfDocument,
    ) -> Result<(), FormatError> {
        loop {
            self.skip_separators();
            match self.peek() {
                None => return Err(FormatError::Corrupt("unterminated FDF block")),
                Some(Token::RBrace) => {
                    self.position += 1;
                    return Ok(());
                }
                Some(Token::Ident(_)) => {}
                Some(Token::Comma) => {
                    self.position += 1;
                    continue;
                }
                _ => return Err(FormatError::Corrupt("unexpected FDF token in block")),
            }
            let statement = self.scan_statement();
            if self.is_include(&statement) {
                let path = self.include_path(&statement)?;
                document.includes.push(path);
                if let Some(brace) = statement.brace {
                    self.position = brace + 1;
                    self.skip_block()?;
                } else {
                    self.position = statement.end;
                }
                continue;
            }
            match statement.brace {
                Some(brace) => {
                    let header = self
                        .tokens
                        .get(statement.start..brace)
                        .ok_or(FormatError::Corrupt("FDF child header out of range"))?
                        .to_vec();
                    let mut child = header_to_node(&header)?;
                    self.position = brace.saturating_add(1);
                    self.parse_body(&mut child, document)?;
                    node.children.push(child);
                }
                None => {
                    let header = self
                        .tokens
                        .get(statement.start..statement.end)
                        .ok_or(FormatError::Corrupt("FDF property out of range"))?
                        .to_vec();
                    let prop = property_from_tokens(&header)?;
                    self.position = statement.end;
                    node.props.push(prop);
                }
            }
        }
    }
}

fn header_to_node(header: &[Token]) -> Result<FdfNode, FormatError> {
    let mut tokens: Vec<&Token> = header
        .iter()
        .filter(|token| !matches!(token, Token::Comma | Token::Newline))
        .collect();
    if tokens.is_empty() {
        return Err(FormatError::Corrupt("empty FDF declaration"));
    }
    let keyword = match tokens.remove(0) {
        Token::Ident(keyword) => keyword.clone(),
        _ => return Err(FormatError::Corrupt("FDF declaration without a keyword")),
    };
    if keyword.eq_ignore_ascii_case("IncludeFile") {
        return Err(FormatError::Corrupt(
            "IncludeFile is not a frame declaration",
        ));
    }
    let is_frame = keyword.eq_ignore_ascii_case("Frame");
    let mut kind = keyword.clone();
    let mut name = None;
    if is_frame {
        match tokens.first() {
            Some(Token::Str(value)) => {
                kind = value.clone();
                tokens.remove(0);
            }
            _ => return Err(FormatError::Corrupt("Frame declaration without a kind")),
        }
        match tokens.first() {
            Some(Token::Str(value)) => {
                name = Some(value.clone());
                tokens.remove(0);
            }
            _ => return Err(FormatError::Corrupt("Frame declaration without a name")),
        }
    } else if let Some(Token::Str(value)) = tokens.first() {
        name = Some(value.clone());
        tokens.remove(0);
    }
    let mut inherits = None;
    let mut inherit_children = false;
    let mut index = 0usize;
    while index < tokens.len() {
        match tokens[index] {
            Token::Ident(ident) if ident.eq_ignore_ascii_case("INHERITS") => {
                index += 1;
                if let Some(Token::Ident(flag)) = tokens.get(index)
                    && flag.eq_ignore_ascii_case("WITHCHILDREN")
                {
                    inherit_children = true;
                    index += 1;
                }
                match tokens.get(index) {
                    Some(Token::Str(base)) => {
                        inherits = Some((*base).clone());
                        index += 1;
                    }
                    _ => return Err(FormatError::Corrupt("INHERITS without a template name")),
                }
            }
            _ => index += 1,
        }
    }
    Ok(FdfNode {
        kind,
        name,
        inherits,
        inherit_children,
        props: Vec::new(),
        children: Vec::new(),
    })
}

fn property_from_tokens(header: &[Token]) -> Result<FdfProp, FormatError> {
    let mut tokens = header
        .iter()
        .filter(|token| !matches!(token, Token::Comma | Token::Newline));
    let key = match tokens.next() {
        Some(Token::Ident(key)) => key.clone(),
        _ => return Err(FormatError::Corrupt("FDF property without a key")),
    };
    let mut values = Vec::new();
    for token in tokens {
        match token {
            Token::Ident(value) => values.push(FdfValue::Ident(value.clone())),
            Token::Str(value) => values.push(FdfValue::Str(value.clone())),
            Token::Number(value) => values.push(FdfValue::Number(*value)),
            Token::LBrace | Token::RBrace => {
                return Err(FormatError::Corrupt("brace inside an FDF property"));
            }
            Token::Comma | Token::Newline => {}
        }
    }
    Ok(FdfProp { key, values })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_console_ui() {
        let text = r#"
            Frame "SIMPLEFRAME" "ConsoleUI" {
                DecorateFileNames,
                Texture {
                    File "ConsoleTexture01",
                    Width 0.256,
                    Height 0.032,
                    TexCoord 0, 1, 0, 0.125,
                    Anchor TOPLEFT,0,0,
                }
            }
        "#;
        let document = FdfDocument::parse(text).expect("parse");
        let frame = document.find("ConsoleUI").expect("frame");
        assert_eq!(frame.kind, "SIMPLEFRAME");
        assert!(frame.flag("DecorateFileNames"));
        assert_eq!(
            frame.children.len(),
            1,
            "DecorateFileNames must not swallow the Texture"
        );
        let texture = &frame.children[0];
        assert_eq!(texture.kind, "Texture");
        assert_eq!(texture.string("File"), Some("ConsoleTexture01"));
        assert_eq!(texture.width(), Some(0.256));
        assert_eq!(texture.tex_coord(), Some([0.0, 1.0, 0.0, 0.125]));
        assert_eq!(texture.texture_uv(), Some([0.0, 0.0, 1.0, 0.125]));
        assert_eq!(texture.anchor(), Some(("TOPLEFT".to_string(), 0.0, 0.0)));
    }

    #[test]
    fn parses_inherits_withchildren_and_multiple_setpoints() {
        let text = r#"
            Frame "TEXT" "Base" {
                FrameFont "MasterFont", 0.013, "",
                Text "Hello",
            }
            Frame "TEXT" "Child" INHERITS WITHCHILDREN "Base" {
                SetPoint TOPLEFT, "Base", TOPLEFT, 0.0, 0.0,
                SetPoint BOTTOMRIGHT, "Other", BOTTOMRIGHT, 0.0, -0.0015,
            }
        "#;
        let document = FdfDocument::parse(text).expect("parse");
        let child = document.find("Child").expect("child");
        assert_eq!(child.inherits.as_deref(), Some("Base"));
        assert!(child.inherit_children);
        let points = child.set_points();
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].point, "TOPLEFT");
        assert_eq!(points[0].target.as_deref(), Some("Base"));
        assert_eq!(points[1].point, "BOTTOMRIGHT");
        assert_eq!(points[1].target_point, "BOTTOMRIGHT");
        assert!((points[1].y - -0.0015).abs() < 1e-6);
        let base_font = document.find("Base").unwrap().font().expect("font");
        assert_eq!(base_font.name, "MasterFont");
        assert_eq!(base_font.property, "FrameFont");
        assert_eq!(base_font.extra.as_deref(), Some(""));
        assert!(
            child.font().is_none(),
            "parser keeps inheritance unresolved"
        );
    }

    #[test]
    fn comments_bom_and_strings() {
        let text = "\u{feff}// header\n/* block\n comment */\n\
            String \"A\" { Text \"a\\\"b\", }\n\
            String \"B\" { Text \"UI\\\\FrameDef\\\\x.fdf\", }";
        let document = FdfDocument::parse(text).expect("parse");
        assert_eq!(document.nodes.len(), 2);
        assert_eq!(document.find("A").unwrap().text(), Some("a\"b"));
        assert_eq!(
            document.find("B").unwrap().text(),
            Some("UI\\FrameDef\\x.fdf")
        );
    }

    #[test]
    fn numbers_and_flags() {
        let text = r#"
            Frame "SIMPLEFRAME" "X" {
                Width -0.0015f,
                Height .5,
                SetAllPoints,
                Alpha 1e-2f,
            }
        "#;
        let document = FdfDocument::parse(text).expect("parse");
        let node = document.find("X").unwrap();
        assert!((node.width().unwrap() + 0.0015).abs() < 1e-6);
        assert_eq!(node.height(), Some(0.5));
        assert!(node.has_set_all_points());
        assert!((node.number("Alpha").unwrap() - 0.01).abs() < 1e-6);
    }

    #[test]
    fn empty_names_and_layers() {
        let text = r#"
            Frame "FRAME" "Root" {
                Layer "ARTWORK" {
                    Texture {
                        File "x.blp",
                    }
                    String "" {
                        Text "hi",
                    }
                }
            }
        "#;
        let document = FdfDocument::parse(text).expect("parse");
        let root = document.find("Root").unwrap();
        let layer = &root.children[0];
        assert_eq!(layer.kind, "Layer");
        assert_eq!(layer.name.as_deref(), Some("ARTWORK"));
        assert_eq!(layer.children.len(), 2);
        assert_eq!(layer.children[1].name.as_deref(), Some(""));
    }

    #[test]
    fn string_list_entries() {
        let text = r#"
            StringList {
                ACCEPT "Accept",
                COLON_GOLD "Gold:",
                MULTILINE "line|nline",
            }
        "#;
        let document = FdfDocument::parse(text).expect("parse");
        let mut entries = Vec::new();
        document.collect_strings(&mut entries);
        assert_eq!(entries.len(), 3);
        assert!(entries.contains(&("COLON_GOLD".to_string(), "Gold:".to_string())));
    }

    #[test]
    fn include_and_counts() {
        let text = r#"
            IncludeFile "UI\FrameDef\Glue\StandardTemplates.fdf",
            Frame "FRAME" "A" {
                Frame "TEXT" "B" { Text "b", }
            }
        "#;
        let document = FdfDocument::parse(text).expect("parse");
        assert_eq!(
            document.includes,
            vec!["UI\\FrameDef\\Glue\\StandardTemplates.fdf".to_string()]
        );
        assert_eq!(document.count(), (2, 1));
    }

    #[test]
    fn rejects_unterminated_blocks_and_strings() {
        assert!(FdfDocument::parse("Frame \"TEXT\" \"A\" { ").is_err());
        assert!(FdfDocument::parse("String \"A\" { Text \"x }").is_err());
        assert!(FdfDocument::parse("/* unterminated").is_err());
    }

    #[test]
    fn multiple_actions_on_one_line() {
        // The FDF grammar allows comma-separated actions on one line; action
        // names are CamelCase while value identifiers are upper case.
        let text = r#"
            Frame "TEXT" "A" {
                Width 0.25, Height 0.5,
                SetPoint TOPLEFT, "Base", TOPLEFT, 0.0, 0.0, SetPoint BOTTOMRIGHT, "Base", BOTTOMRIGHT, 0.0, 0.0,
                FontColor 1 1 1 1, FontJustificationH JUSTIFYCENTER, FontJustificationV JUSTIFYMIDDLE,
                DecorateFileNames, SetAllPoints,
            }
        "#;
        let document = FdfDocument::parse(text).expect("parse");
        let node = document.find("A").unwrap();
        assert_eq!(node.width(), Some(0.25));
        assert_eq!(node.height(), Some(0.5));
        assert_eq!(node.set_points().len(), 2);
        assert_eq!(node.font_color(), Some([1.0, 1.0, 1.0, 1.0]));
        assert_eq!(node.font_justification_h(), Some("JUSTIFYCENTER"));
        assert_eq!(node.font_justification_v(), Some("JUSTIFYMIDDLE"));
        assert!(node.flag("DecorateFileNames"));
        assert!(node.has_set_all_points());
    }

    #[test]
    fn audit_collects_decoration_strings_and_fonts() {
        let text = r#"
            Frame "FRAME" "Root" {
                DecorateFileNames,
                Frame "BACKDROP" "Panel" {
                    BackdropBackground "PanelBackground",
                    NormalTexture "PanelNormal",
                    Frame "TEXT" "Label" {
                        FrameFont "MasterFont", 0.012, "",
                        Text "COLON_GOLD",
                    }
                }
                Frame "SPRITE" "Logo" {
                    BackgroundArt "LogoArt",
                }
            }
            StringList {
                GOLD "Gold",
                COLON_GOLD "Gold:",
                DUP "first",
                DUP "second",
            }
        "#;
        let document = FdfDocument::parse(text).expect("parse");
        let audit = document.audit();
        assert_eq!(audit.nodes, 5, "root + panel + label + logo + stringlist");
        assert!(audit.props >= 7);
        assert_eq!(audit.decorate_nodes, 1);
        assert_eq!(audit.string_list_entries, 4, "duplicates preserved");
        assert_eq!(audit.font_refs, 1);
        assert_eq!(audit.text_values, 1);
        assert_eq!(audit.texture_refs, 1, "backdrop background");
        assert_eq!(audit.decorated_texture_refs, 1);
        assert_eq!(audit.template_texture_refs, 1, "NormalTexture is a template");
        assert_eq!(audit.model_refs, 1);

        let refs = document.texture_refs();
        let background = refs
            .iter()
            .find(|reference| reference.property == "BackdropBackground")
            .expect("backdrop ref");
        assert_eq!(background.value, "PanelBackground");
        assert!(background.decorated);
        assert!(!background.template_reference);
        let normal = refs
            .iter()
            .find(|reference| reference.property == "NormalTexture")
            .expect("template ref");
        assert!(!normal.decorated);
        assert!(normal.template_reference);

        let strings = document.string_list_entries();
        assert_eq!(
            strings,
            vec![
                ("GOLD".to_string(), "Gold".to_string()),
                ("COLON_GOLD".to_string(), "Gold:".to_string()),
                ("DUP".to_string(), "first".to_string()),
                ("DUP".to_string(), "second".to_string()),
            ]
        );
    }

    #[test]
    fn fdf_text_resolves_keys_wts_and_duplicates() {
        let document = FdfDocument::parse(
            r#"
            StringList {
                GOLD "Gold",
                COLON_GOLD "Gold:",
                DUP "first",
                DUP "second",
            }
        "#,
        )
        .expect("parse");
        let mut text = FdfText::from_document(&document);
        assert_eq!(text.len(), 4);
        assert_eq!(text.get("DUP"), Some("second"));
        assert_eq!(text.get_all("DUP"), vec!["first", "second"]);
        assert_eq!(text.resolve("COLON_GOLD"), Some("Gold:"));
        assert_eq!(text.resolve_owned("COLON_MISSING"), None);
        assert_eq!(text.parse_wts("STRING 1\n{\nHello |nworld\n}\nSTRING 2 // note\n{\nSecond\n}\n"), 2);
        assert_eq!(text.text("TRIGSTR_1"), "Hello |nworld");
        assert_eq!(text.text("TRIGSTR_9"), "TRIGSTR_9");
        assert_eq!(text.text("LITERAL"), "LITERAL");
        assert_eq!(text.plain("|cffffcc00Gold|r"), "Gold");
    }

    #[test]
    fn corpus_71_documents_match_audit() {
        use std::path::{Path, PathBuf};

        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("fdf"))
                {
                    out.push(path);
                }
            }
        }

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/war3_full3");
        if !root.is_dir() {
            eprintln!("skip: analysis/war3_full3 missing");
            return;
        }
        let mut files = Vec::new();
        walk(&root, &mut files);
        files.sort();
        assert_eq!(files.len(), 71, "canonical FDF corpus size");

        let mut nodes = 0usize;
        let mut props = 0usize;
        let mut includes = 0usize;
        let mut inherits = 0usize;
        let mut with_children = 0usize;
        let mut strings = 0usize;
        for path in &files {
            let the_text = std::fs::read_to_string(path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            let document = FdfDocument::parse_named(&the_text, path.display().to_string())
                .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
            let audit = document.audit();
            nodes += audit.nodes;
            props += audit.props;
            includes += audit.includes;
            inherits += audit.inherit_statements;
            with_children += audit.with_children;
            strings += audit.string_list_entries;
        }
        assert_eq!(nodes, 1956, "declarations");
        assert_eq!(props, 6514, "properties");
        assert_eq!(includes, 60, "IncludeFile");
        assert_eq!(inherits, 1304, "INHERITS");
        assert_eq!(with_children, 388, "INHERITS WITHCHILDREN");
        assert_eq!(strings, 874, "StringList entries");
    }

    #[test]
    fn newlines_terminate_properties() {
        // FDF is newline-sensitive: `Width` alone is a flag and the next-line
        // number is a stray token, which retail FDF treats as a parse error.
        let text = "Frame \"SIMPLEFRAME\" \"A\" {\n  Width\n  0.5,\n}\n";
        assert!(FdfDocument::parse(text).is_err());
    }
}
