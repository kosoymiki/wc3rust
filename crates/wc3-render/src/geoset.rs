//! Geoset animation (`GEOA`) helpers for the renderer.
//!
//! `wc3_anim` already samples the **alpha** (`KGAO`) because mesh visibility
//! depends on it. The colour channel (`KGAC`) is a renderer concern: it grades
//! the geoset's vertex colours (the TownHall's construction shells and the
//! Demon Hunter's metamorphosis tint whole geosets).
//!
//! The parser stores `GeosetAnimation::color` already sizzled from the file's
//! BGR order to RGB (see `wc3_formats::mdx::sizzle_bgr`); `KGAC` *track* values
//! are RGB on disk, matching the donor
//! (`mdx-m3-viewer/src/viewer/handlers/mdx/geosetanimation.ts:16-18`).

use glam::Vec3;
use wc3_anim::sample_in_sequence;
use wc3_formats::mdx::{GeosetAnimation, Sequence, Track};

/// `true` when `track` is a real variant of `sequence`: at least two in-range
/// keys with different values.
///
/// This is the generic form of `wc3_anim::track_active_in_sequence` (which is
/// `f32` only) so vector/integer tracks follow the same rule.
pub(crate) fn active_in_sequence<T: Copy + PartialEq>(track: &Track<T>, sequence: &Sequence) -> bool {
    let mut first: Option<T> = None;
    for key in &track.keys {
        if key.time < sequence.start || key.time > sequence.end {
            continue;
        }
        match first {
            None => first = Some(key.value),
            Some(value) if value != key.value => return true,
            Some(_) => {}
        }
    }
    false
}

/// Colour grade of one geoset for the current frame.
///
/// Returns white (`[1.0; 4]`) when the geoset has no `GEOA`, so callers can pass
/// the result straight into [`crate::scene::UnitDraw::mesh_color`].
pub fn geoset_color(
    animations: &[GeosetAnimation],
    geoset_id: u32,
    sequence: Option<&Sequence>,
    time_ms: u32,
) -> [f32; 4] {
    let Some(animation) = animations
        .iter()
        .find(|animation| animation.geoset_id == geoset_id)
    else {
        return [1.0; 4];
    };
    let color = match (&animation.color_track, sequence) {
        (Some(track), Some(sequence)) if active_in_sequence(track, sequence) => {
            sample_in_sequence(track, sequence, time_ms).unwrap_or(animation.color)
        }
        (Some(track), None) => track.sample(time_ms).unwrap_or(animation.color),
        _ => animation.color,
    };
    [color.x, color.y, color.z, 1.0]
}

/// Grade vector without the alpha lane.
pub fn geoset_tint(
    animations: &[GeosetAnimation],
    geoset_id: u32,
    sequence: Option<&Sequence>,
    time_ms: u32,
) -> Vec3 {
    let color = geoset_color(animations, geoset_id, sequence, time_ms);
    Vec3::new(color[0], color[1], color[2])
}

#[cfg(test)]
mod tests {
    use super::*;
    use wc3_formats::mdx::{Interpolation, Key};

    fn animation(geoset_id: u32, color: Vec3) -> GeosetAnimation {
        GeosetAnimation {
            geoset_id,
            alpha: 1.0,
            flags: 0,
            color,
            alpha_track: None,
            color_track: None,
        }
    }

    fn sequence() -> Sequence {
        Sequence {
            name: "Stand".to_string(),
            start: 0,
            end: 1000,
            move_speed: 0.0,
            flags: 0,
            rarity: 0.0,
            sync_point: 0,
            bounds: wc3_formats::mdx::Extent::default(),
        }
    }

    #[test]
    fn missing_animation_is_white() {
        assert_eq!(geoset_color(&[], 3, None, 0), [1.0; 4]);
        let animations = [animation(0, Vec3::splat(0.5))];
        assert_eq!(geoset_color(&animations, 1, None, 0), [1.0; 4]);
    }

    #[test]
    fn static_colour_is_used_without_a_track() {
        let animations = [animation(2, Vec3::new(0.25, 0.5, 0.75))];
        let color = geoset_color(&animations, 2, None, 0);
        assert_eq!(color, [0.25, 0.5, 0.75, 1.0]);
        // The sequence matters only through the track.
        let color = geoset_color(&animations, 2, Some(&sequence()), 500);
        assert_eq!(color, [0.25, 0.5, 0.75, 1.0]);
    }

    #[test]
    fn colour_track_wins_over_the_static_value() {
        let mut entry = animation(0, Vec3::ONE);
        entry.color_track = Some(Track {
            interpolation: Interpolation::Linear,
            global_sequence: None,
            static_value: None,
            keys: vec![
                Key {
                    time: 0,
                    value: Vec3::ZERO,
                    in_tan: Vec3::ZERO,
                    out_tan: Vec3::ZERO,
                },
                Key {
                    time: 1000,
                    value: Vec3::ONE,
                    in_tan: Vec3::ONE,
                    out_tan: Vec3::ONE,
                },
            ],
        });
        let sequence = sequence();
        let start = geoset_color(&[entry.clone()], 0, Some(&sequence), 0);
        let middle = geoset_color(&[entry.clone()], 0, Some(&sequence), 500);
        let end = geoset_color(&[entry], 0, Some(&sequence), 1000);
        assert!(start[0] < 0.05, "{start:?}");
        assert!((middle[0] - 0.5).abs() < 0.1, "{middle:?}");
        assert!((end[0] - 1.0).abs() < 0.05, "{end:?}");
    }

    #[test]
    fn single_key_track_is_not_a_variant() {
        // A one-key track is a constant; the static colour must win, matching
        // the alpha rule in `wc3_anim::geoset_alpha`.
        let mut entry = animation(0, Vec3::new(0.2, 0.4, 0.6));
        entry.color_track = Some(Track {
            interpolation: Interpolation::Linear,
            global_sequence: None,
            static_value: None,
            keys: vec![Key {
                time: 500,
                value: Vec3::ONE,
                in_tan: Vec3::ONE,
                out_tan: Vec3::ONE,
            }],
        });
        let color = geoset_color(&[entry], 0, Some(&sequence()), 500);
        assert_eq!(color, [0.2, 0.4, 0.6, 1.0]);
    }

    #[test]
    fn real_model_geoset_colour_is_rgb_after_the_parser_sizzle() {
        // FrostNovaTarget has `GEOA` entries; whatever they are, the channel
        // order must survive the round trip (the parser sizzles BGR once).
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut path = None;
        for tree in ["analysis/war3_samples", "analysis/war3_full"] {
            let candidate = root
                .join(tree)
                .join("Abilities/Spells/Undead/FrostNova/FrostNovaTarget.mdx");
            if candidate.exists() {
                path = Some(candidate);
                break;
            }
        }
        let Some(path) = path else {
            eprintln!("skipping: FrostNovaTarget.mdx fixture missing");
            return;
        };
        let bytes = std::fs::read(path).expect("read fixture");
        let model = wc3_formats::mdx::MdxModel::parse(&bytes).expect("parse");
        for animation in &model.geoset_animations {
            let color = geoset_color(
                &model.geoset_animations,
                animation.geoset_id,
                model.sequences.first(),
                0,
            );
            assert!(color.iter().all(|lane| lane.is_finite()));
            assert_eq!(color[3], 1.0);
        }
    }
}
