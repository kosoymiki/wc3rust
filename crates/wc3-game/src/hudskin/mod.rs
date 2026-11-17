//! `hudskin` — the fantasy-atlas HUD skin, split one module per HUD region.
//!
//! The atlas (`ui_atlas.png` + JSON) is sliced into 24 named elements in
//! `analysis/h2/atlas/slices.json`; `ROLES` below is that same order, so the
//! runtime never needs the JSON: `Assets::ui_atlas()` exposes the rects and
//! insets in this order.
//!
//! Contract for every module in this directory: take a target `Rect` on screen,
//! emit quads into the existing `UiBatch`, and never sample outside the element's
//! atlas rect. Nine-patch geometry lives here (`Skin::nine`), so a module only
//! decides *which* role and *where*, never how UVs are computed.

use crate::hud::{Rect, UiBatch};

/// Element roles, in atlas-manifest order (`analysis/h2/atlas/slices.json`).
pub const ROLES: [&str; 24] = [
    "console_left_cap", "console_right_cap", "topbar_center", "dialog_panel", "resource_bar", "topbar_left", "portrait_frame", "card_plate", "thin_bar", "tooltip_box", "tile_normal", "tile_hover", "tile_pressed", "tile_gold", "tile_dark", "tile_empty", "slot_gold", "slot_dark", "hp_bar", "list_panel", "button_normal", "dialog_wide", "menu_button_pressed", "wide_panel",
];

pub mod tests_support;
pub mod console;
pub mod topbar;
pub mod tiles;
pub mod pills;
pub mod minimap_frame;
pub mod portrait;
pub mod stats;
pub mod tooltip;
pub mod army;
pub mod dialogs;
pub mod pack;
pub mod soft_raster;

/// One loaded skin: the atlas texture index plus the pixel rect/insets of every
/// role. `None` when the install has no atlas (the HUD then keeps the FDF art).
#[derive(Clone, Debug)]
pub struct Skin {
    /// External texture index (`UiBatch::textured_quad`).
    pub texture: usize,
    pub width: f32,
    pub height: f32,
    /// Atlas pixel rect per `ROLES` index.
    pub rects: [[f32; 4]; 24],
    /// Nine-patch insets per `ROLES` index (left, top, right, bottom) in atlas px.
    pub insets: [[f32; 4]; 24],
    /// Per-role texture for skins whose roles are separate PNGs (the final
    /// pack). Empty means every role samples [`Self::texture`].
    pub role_textures: Vec<usize>,
    /// Per-role source size in pixels for `role_textures`. Empty means
    /// [`Self::width`] x [`Self::height`].
    pub role_sizes: Vec<[f32; 2]>,
    /// Per-role source asset name (pack skins), for diagnostics and the
    /// element table. Empty for atlas skins.
    pub role_names: Vec<String>,
    /// The loaded final pack when this skin came from it. The pack carries the
    /// composition contract (design height, safe areas, card/grid cells, fonts)
    /// and the named-asset draw helpers.
    pub pack: Option<pack::HudPack>,
}

/// Roles in `ROLES` order -> asset names in the final pack.
///
/// Every name is a manifest `assets[].name`; `Skin::from_pack` fails (and logs)
/// when one is absent rather than silently substituting a different element.
pub const PACK_ROLE_ASSETS: [&str; 24] = [
    "console_left_cap_normal_256x288",
    "console_right_cap_normal_256x288",
    "topbar_plate_normal_720x32",
    "menu_panel_normal_512x384",
    "resource_pill_normal_236x32",
    "topbar_plate_normal_720x32",
    "portrait_frame_normal_128",
    "menu_panel_normal_512x384",
    "build_progress_normal_64x8",
    "tooltip_panel_normal_384x256",
    "button_border_normal_64",
    "button_glow_64",
    "button_border_pressed_64",
    "button_border_autocast_64",
    "button_border_disabled_64",
    "button_border_empty_64",
    "control_group_button_active_44",
    "control_group_button_inactive_44",
    "build_progress_normal_64x8",
    "selection_panel_normal_240x180",
    "menu_button_normal_256x64",
    "score_panel_normal_1024x512",
    "menu_button_pressed_256x64",
    "loading_frame_normal_1024x512",
];

impl Skin {
    pub fn role_index(role: &str) -> Option<usize> {
        ROLES.iter().position(|name| *name == role)
    }

    /// Texture index for a role: the per-role PNG on a pack skin, the atlas
    /// otherwise.
    pub fn texture_of(&self, index: usize) -> usize {
        self.role_textures.get(index).copied().unwrap_or(self.texture)
    }

    /// Source size for a role (pixels), used to build UVs.
    pub fn size_of(&self, index: usize) -> [f32; 2] {
        self.role_sizes
            .get(index)
            .copied()
            .unwrap_or([self.width, self.height])
    }

    /// Build the nine-patch skin from the final pack: one PNG per role, the
    /// manifest rect and insets, and the pack itself for the composition and
    /// named-asset draw path.
    pub fn from_pack(pack: &pack::HudPack) -> Option<Skin> {
        let roles = ROLES.len();
        let mut rects = [[0.0; 4]; 24];
        let mut insets = [[0.0; 4]; 24];
        let mut role_textures = Vec::with_capacity(roles);
        let mut role_sizes = Vec::with_capacity(roles);
        let mut role_names = Vec::with_capacity(roles);
        for (slot, name) in PACK_ROLE_ASSETS.iter().enumerate() {
            let Some(asset) = pack.asset(name) else {
                crate::dev_log!("HUD_PACK role_missing={name} role={}", ROLES[slot]);
                return None;
            };
            rects[slot] = asset.rect();
            insets[slot] = asset.insets;
            role_textures.push(asset.texture);
            role_sizes.push(asset.size());
            role_names.push((*name).to_string());
        }
        Some(Skin {
            texture: role_textures[0],
            width: role_sizes[0][0],
            height: role_sizes[0][1],
            rects,
            insets,
            role_textures,
            role_sizes,
            role_names,
            pack: Some(pack.clone()),
        })
    }

    /// Emit `role` as a nine-patch into `rect`. Corners keep their atlas pixel
    /// size, edges stretch, the centre stretches.
    pub fn nine(&self, batch: &mut UiBatch, role: &str, rect: Rect, tint: [f32; 4]) -> bool {
        self.nine_scaled(batch, role, rect, tint, 1.0)
    }

    /// Nine-patch with `art_scale`: the ornament grows with the HUD scale, so a
    /// 12 px atlas bevel stays 12 px on screen at scale 1 and 13.2 px at 1.1
    /// (a fixed inset shrank the border relative to the panel at every zoom).
    pub fn nine_scaled(
        &self,
        batch: &mut UiBatch,
        role: &str,
        rect: Rect,
        tint: [f32; 4],
        art_scale: f32,
    ) -> bool {
        let Some(index) = Self::role_index(role) else {
            return false;
        };
        let [ax, ay, aw, ah] = self.rects[index];
        let [width, height] = self.size_of(index);
        let texture = self.texture_of(index);
        // A large piece of art in a thin bar is legitimate (130 px art in a
        // 14 px bar = 0.108), so the floor only guards divide-by-zero-ish input.
        let scale = art_scale.max(0.01);
        let [l, t, r, b] = self.insets[index].map(|inset| inset * scale);
        // Degenerate targets: fall back to a single stretched quad.
        if rect.w <= l + r + 1.0 || rect.h <= t + b + 1.0 {
            return self.stretch(batch, role, rect, tint);
        }
        let [al, at, ar, ab] = self.insets[index];
        let sx = [0.0, al / aw, (aw - ar).max(al) / aw, 1.0];
        let sy = [0.0, at / ah, (ah - ab).max(at) / ah, 1.0];
        let dx = [rect.x, rect.x + l, rect.right() - r, rect.right()];
        let dy = [rect.y, rect.y + t, rect.bottom() - b, rect.bottom()];
        for row in 0..3 {
            for col in 0..3 {
                let cell = Rect::new(dx[col], dy[row], dx[col + 1] - dx[col], dy[row + 1] - dy[row]);
                if cell.w <= 0.0 || cell.h <= 0.0 {
                    continue;
                }
                let u0 = (ax + sx[col] * aw) / width;
                let u1 = (ax + sx[col + 1] * aw) / width;
                let v0 = (ay + sy[row] * ah) / height;
                let v1 = (ay + sy[row + 1] * ah) / height;
                batch.textured_quad(texture, cell, [u0, v0, u1, v1], tint);
            }
        }
        true
    }

    /// One stretched quad of `role`, optionally mirrored horizontally. WC3's
    /// console/bar end ornaments are the same art flipped on the right side, so
    /// the skin offers the flip instead of every module re-implementing it.
    pub fn stretch_mirror(
        &self,
        batch: &mut UiBatch,
        role: &str,
        rect: Rect,
        tint: [f32; 4],
        flip_u: bool,
    ) -> bool {
        let Some(index) = Self::role_index(role) else {
            return false;
        };
        let [ax, ay, aw, ah] = self.rects[index];
        let [width, height] = self.size_of(index);
        let mut uv = [
            ax / width,
            ay / height,
            (ax + aw) / width,
            (ay + ah) / height,
        ];
        if flip_u {
            uv.swap(0, 2);
        }
        batch.textured_quad(self.texture_of(index), rect, uv, tint);
        true
    }

    /// One stretched quad of `role` (bars, fills, single-piece ornaments).
    pub fn stretch(&self, batch: &mut UiBatch, role: &str, rect: Rect, tint: [f32; 4]) -> bool {
        let Some(index) = Self::role_index(role) else {
            return false;
        };
        let [ax, ay, aw, ah] = self.rects[index];
        let [width, height] = self.size_of(index);
        let uv = [
            ax / width,
            ay / height,
            (ax + aw) / width,
            (ay + ah) / height,
        ];
        batch.textured_quad(self.texture_of(index), rect, uv, tint);
        true
    }
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    use crate::hudskin::tests_support::sample_skin;

    /// `art_scale` grows the ornament with the HUD: at 2x the corner blocks are
    /// twice as large while the atlas UV corner stays exactly the same texel.
    #[test]
    fn nine_scaled_grows_corners_not_uvs() {
        let skin = sample_skin();
        let rect = Rect::new(0.0, 0.0, 400.0, 200.0);
        let mut one = UiBatch::default();
        let mut two = UiBatch::default();
        assert!(skin.nine(&mut one, "card_plate", rect, [1.0; 4]));
        assert!(skin.nine_scaled(&mut two, "card_plate", rect, [1.0; 4], 2.0));
        // Nine quads either way; `UiBatch` merges them into one draw, so the
        // quad count is the vertex count / 4.
        assert_eq!(one.textured.len(), 1, "one merged draw");
        assert_eq!(one.textured[0].vertices.len(), 9 * 6, "nine-patch: 9 quads x 2 triangles");
        assert_eq!(two.textured[0].vertices.len(), 9 * 6);
        // Top-left corner destination is 2x wider at art_scale 2.
        let w1 = quads_extent(&one, 0);
        let w2 = quads_extent(&two, 0);
        assert!(
            (w2.0 - w1.0 * 2.0).abs() <= 1.5 && (w2.1 - w1.1 * 2.0).abs() <= 1.5,
            "corner must scale: {w1:?} vs {w2:?}"
        );
        // The UV rectangle of that corner is identical (same atlas texels).
        let uv1 = one.textured[0].vertices[0].uv;
        let uv2 = two.textured[0].vertices[0].uv;
        for (a, b) in uv1.iter().zip(uv2.iter()) {
            assert!((a - b).abs() < 1e-6, "corner UV must not change: {uv1:?} vs {uv2:?}");
        }
    }

    fn quads_extent(batch: &UiBatch, quad_index: usize) -> (f32, f32) {
        let draw = &batch.textured[0];
        let quad = &draw.vertices[quad_index * 6..quad_index * 6 + 6];
        let xs: Vec<f32> = quad.iter().map(|v| v.pos[0]).collect();
        let ys: Vec<f32> = quad.iter().map(|v| v.pos[1]).collect();
        (
            xs.iter().cloned().fold(f32::MIN, f32::max) - xs.iter().cloned().fold(f32::MAX, f32::min),
            ys.iter().cloned().fold(f32::MIN, f32::max) - ys.iter().cloned().fold(f32::MAX, f32::min),
        )
    }
}

#[cfg(test)]
mod mirror_tests {
    use super::*;
    use crate::hudskin::tests_support::sample_skin;

    #[test]
    fn mirrored_ornament_flips_u_only() {
        let skin = sample_skin();
        let rect = Rect::new(10.0, 20.0, 80.0, 40.0);
        let mut plain = UiBatch::default();
        let mut flipped = UiBatch::default();
        assert!(skin.stretch(&mut plain, "topbar_left", rect, [1.0; 4]));
        assert!(skin.stretch_mirror(&mut flipped, "topbar_left", rect, [1.0; 4], true));
        // Vertices carry per-vertex (u, v): a horizontal mirror reflects U about
        // the element's centre while V stays untouched.
        let plain_u: Vec<f32> = plain.textured[0].vertices.iter().map(|v| v.uv[0]).collect();
        let flip_u: Vec<f32> = flipped.textured[0].vertices.iter().map(|v| v.uv[0]).collect();
        let lo = plain_u.iter().cloned().fold(f32::MAX, f32::min);
        let hi = plain_u.iter().cloned().fold(f32::MIN, f32::max);
        for (a, b) in plain_u.iter().zip(flip_u.iter()) {
            assert!(
                ((lo + hi) - (a + b)).abs() < 1e-6,
                "U must mirror about the centre: {a} vs {b} (range {lo}..{hi})"
            );
        }
        for (a, b) in plain.textured[0].vertices.iter().zip(flipped.textured[0].vertices.iter()) {
            assert!((a.uv[1] - b.uv[1]).abs() < 1e-6, "V must not change");
        }
        // The destination rect is identical: mirroring is a UV trick, not a move.
        for (p, q) in plain.textured[0].vertices.iter().zip(flipped.textured[0].vertices.iter()) {
            assert_eq!(p.pos, q.pos);
        }
    }
}
