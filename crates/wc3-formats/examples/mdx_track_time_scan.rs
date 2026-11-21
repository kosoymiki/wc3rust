//! Scan every track in a model for keyframe times that are negative when read
//! as the engine's signed `int32` (`u32` >= `0x8000_0000`).
//!
//! Evidence tool for the `mdx_track_unsorted` diagnostic: some shipped models
//! store keys at negative times (for example `-133` = `0xFFFF_FF7B`). This
//! prints each such track with its owner (node name / geoset / layer) so the
//! corpus claim is a census over files, not an anecdote.
//!
//! Usage: `cargo run -p wc3-formats --example mdx_track_time_scan -- FILE...`

use std::path::PathBuf;

use wc3_formats::mdx::{Interpolation, MdxModel, Node, Track};

const SIGN_BIT: u32 = 0x8000_0000;

fn scan_track<T>(model: &str, owner: &str, kind: &str, track: &Track<T>) -> bool {
    let negative: Vec<i32> = track
        .keys
        .iter()
        .map(|key| key.time)
        .filter(|time| *time >= SIGN_BIT)
        .map(|time| time as i32)
        .collect();
    if negative.is_empty() {
        return false;
    }
    let sorted_signed = track
        .keys
        .windows(2)
        .all(|pair| (pair[0].time as i32) <= (pair[1].time as i32));
    let interp = match track.interpolation {
        Interpolation::None => "none",
        Interpolation::Linear => "linear",
        Interpolation::Hermite => "hermite",
        Interpolation::Bezier => "bezier",
    };
    println!(
        "{model} owner={owner:?} track={kind} keys={} negative={negative:?} sorted_signed={sorted_signed} interp={interp} times={:?}",
        track.keys.len(),
        track
            .keys
            .iter()
            .map(|key| key.time as i32)
            .take(20)
            .collect::<Vec<_>>(),
    );
    true
}

fn scan_opt<T>(model: &str, owner: &str, kind: &str, track: &Option<Track<T>>) -> bool {
    match track {
        Some(track) => scan_track(model, owner, kind, track),
        None => false,
    }
}

fn scan_node(model: &str, node: &Node) -> bool {
    let mut found = scan_track(model, &node.name, "translation", &node.translation);
    found |= scan_track(model, &node.name, "rotation", &node.rotation);
    found |= scan_track(model, &node.name, "scaling", &node.scaling);
    found
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paths: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    if paths.is_empty() {
        return Err("usage: mdx_track_time_scan FILE...".into());
    }
    let mut files_with_negative = 0usize;
    let mut files = 0usize;
    for path in &paths {
        files += 1;
        let bytes = std::fs::read(path)?;
        let model = MdxModel::parse(&bytes)?;
        let label = path.display().to_string();
        let mut found = false;
        for bone in &model.bones {
            found |= scan_node(&label, &bone.node);
        }
        for helper in &model.helpers {
            found |= scan_node(&label, helper);
        }
        for attachment in &model.attachments {
            found |= scan_node(&label, &attachment.node);
        }
        for event in &model.events {
            found |= scan_node(&label, event);
        }
        for material in &model.materials {
            for layer in &material.layers {
                let owner = format!("material{} layer", material.priority);
                found |= scan_opt(&label, &owner, "alpha", &layer.alpha_track);
                found |= scan_opt(&label, &owner, "texture_id", &layer.texture_id_track);
            }
        }
        for animation in &model.geoset_animations {
            let owner = format!("geoset{}", animation.geoset_id);
            found |= scan_opt(&label, &owner, "alpha", &animation.alpha_track);
            found |= scan_opt(&label, &owner, "color", &animation.color_track);
        }
        for emitter in &model.particle_emitters {
            found |= scan_node(&label, &emitter.node);
            let owner = format!("prem {}", emitter.node.name);
            found |= scan_opt(&label, &owner, "emission_rate", &emitter.emission_rate_track);
            found |= scan_opt(&label, &owner, "gravity", &emitter.gravity_track);
            found |= scan_opt(&label, &owner, "longitude", &emitter.longitude_track);
            found |= scan_opt(&label, &owner, "latitude", &emitter.latitude_track);
            found |= scan_opt(&label, &owner, "life_span", &emitter.life_span_track);
            found |= scan_opt(&label, &owner, "speed", &emitter.speed_track);
            found |= scan_opt(&label, &owner, "visibility", &emitter.visibility_track);
        }
        for emitter in &model.particle_emitters2 {
            found |= scan_node(&label, &emitter.node);
            let owner = format!("pre2 {}", emitter.node.name);
            found |= scan_opt(&label, &owner, "speed", &emitter.speed_track);
            found |= scan_opt(&label, &owner, "variation", &emitter.variation_track);
            found |= scan_opt(&label, &owner, "latitude", &emitter.latitude_track);
            found |= scan_opt(&label, &owner, "gravity", &emitter.gravity_track);
            found |= scan_opt(&label, &owner, "visibility", &emitter.visibility_track);
            found |= scan_opt(&label, &owner, "emission_rate", &emitter.emission_rate_track);
            found |= scan_opt(&label, &owner, "width", &emitter.width_track);
            found |= scan_opt(&label, &owner, "length", &emitter.length_track);
        }
        for ribbon in &model.ribbon_emitters {
            found |= scan_node(&label, &ribbon.node);
            let owner = format!("ribb {}", ribbon.node.name);
            found |= scan_opt(&label, &owner, "height_above", &ribbon.height_above_track);
            found |= scan_opt(&label, &owner, "height_below", &ribbon.height_below_track);
            found |= scan_opt(&label, &owner, "alpha", &ribbon.alpha_track);
            found |= scan_opt(&label, &owner, "color", &ribbon.color_track);
            found |= scan_opt(&label, &owner, "texture_slot", &ribbon.texture_slot_track);
            found |= scan_opt(&label, &owner, "visibility", &ribbon.visibility_track);
        }
        if found {
            files_with_negative += 1;
        }
    }
    eprintln!("files={files} files_with_negative_keys={files_with_negative}");
    Ok(())
}
