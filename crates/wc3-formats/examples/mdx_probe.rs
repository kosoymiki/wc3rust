//! Print the chunk set and effect inventory of an MDX model.
//!
//! ```text
//! cargo run -p wc3-formats --example mdx_probe -- <model.mdx> [--json]
//! ```
//!
//! Used as evidence by the render/models donor intake: it reports every effect
//! chunk the parser now understands (`PRE2`/`PREM`/`RIBB`/`EVTS`/`ATCH`) with
//! the fields the renderer consumes, so a byte-level claim can be re-checked
//! without reading the source.

use std::env;
use std::fs;

use wc3_formats::mdx::{EventObjectKind, MdxModel, ParticleFilter};

fn main() {
    let mut json = false;
    let mut paths = Vec::new();
    for arg in env::args().skip(1) {
        if arg == "--json" {
            json = true;
        } else {
            paths.push(arg);
        }
    }
    if paths.is_empty() {
        eprintln!("usage: mdx_probe <model.mdx> [...] [--json]");
        std::process::exit(2);
    }
    let mut first = true;
    if json {
        print!("[");
    }
    for path in paths {
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!("{path}: {error}");
                continue;
            }
        };
        let model = match MdxModel::parse(&bytes) {
            Ok(model) => model,
            Err(error) => {
                eprintln!("{path}: parse failed: {error:?}");
                std::process::exit(1);
            }
        };
        if json {
            if !first {
                print!(",");
            }
            first = false;
            println!();
            print!(
                "{{\"path\":\"{}\",\"name\":\"{}\",\"sequences\":{},\"geosets\":{},\"materials\":{},\"textures\":{},\"pre2\":{},\"prem\":{},\"ribb\":{},\"evts\":{},\"atch\":{}",
                esc(&path),
                esc(&model.name),
                model.sequences.len(),
                model.geosets.len(),
                model.materials.len(),
                model.textures.len(),
                model.particle_emitters2.len(),
                model.particle_emitters.len(),
                model.ribbon_emitters.len(),
                model.event_objects.len(),
                model.attachments.len()
            );
            print!(",\"pre2_detail\":[");
            for (index, emitter) in model.particle_emitters2.iter().enumerate() {
                if index > 0 {
                    print!(",");
                }
                print!(
                    "{{\"name\":\"{}\",\"filter\":\"{:?}\",\"rows\":{},\"columns\":{},\"life\":{},\"rate\":{},\"priority\":{},\"replaceable\":{},\"texture\":{},\"tracks\":{}}}",
                    esc(&emitter.node.name),
                    emitter.filter(),
                    emitter.rows,
                    emitter.columns,
                    emitter.life_span,
                    emitter.emission_rate,
                    emitter.priority_plane,
                    emitter.replaceable_id,
                    emitter.texture_id,
                    track_count(emitter)
                );
            }
            print!("],\"ribb_detail\":[");
            for (index, ribbon) in model.ribbon_emitters.iter().enumerate() {
                if index > 0 {
                    print!(",");
                }
                print!(
                    "{{\"name\":\"{}\",\"height_above\":{},\"height_below\":{},\"life\":{},\"rate\":{},\"rows\":{},\"columns\":{},\"gravity\":{},\"visible_track\":{}}}",
                    esc(&ribbon.node.name),
                    ribbon.height_above,
                    ribbon.height_below,
                    ribbon.life_span,
                    ribbon.emission_rate,
                    ribbon.rows,
                    ribbon.columns,
                    ribbon.gravity,
                    ribbon.visibility_track.is_some() || ribbon.alpha_track.is_some()
                );
            }
            print!("],\"evts_detail\":[");
            for (index, event) in model.event_objects.iter().enumerate() {
                if index > 0 {
                    print!(",");
                }
                let (kind, id) = event.kind_and_id();
                let kind = match kind {
                    EventObjectKind::SpawnModel => "SPN",
                    EventObjectKind::Splat => "SPL",
                    EventObjectKind::UberSplat => "UBR",
                    EventObjectKind::Sound => "SND",
                    EventObjectKind::Unknown => "???",
                };
                print!(
                    "{{\"name\":\"{}\",\"kind\":\"{}\",\"id\":\"{}\",\"keys\":{},\"triggers\":{}}}",
                    esc(&event.node.name),
                    kind,
                    id,
                    event.track.frames.len(),
                    event.track.frames.iter().filter(|frame| **frame != 0).count()
                );
            }
            print!("],\"atch_detail\":[");
            for (index, attachment) in model.attachments.iter().enumerate() {
                if index > 0 {
                    print!(",");
                }
                print!(
                    "{{\"name\":\"{}\",\"path\":\"{}\",\"object_id\":{}}}",
                    esc(&attachment.node.name),
                    esc(&attachment.path),
                    attachment.node.object_id
                );
            }
            print!("]}}");
        } else {
            println!("## {path}");
            println!(
                "  name={} seqs={} geosets={} materials={} textures={}",
                model.name,
                model.sequences.len(),
                model.geosets.len(),
                model.materials.len(),
                model.textures.len()
            );
            println!(
                "  PRE2={} PREM={} RIBB={} EVTS={} ATCH={}",
                model.particle_emitters2.len(),
                model.particle_emitters.len(),
                model.ribbon_emitters.len(),
                model.event_objects.len(),
                model.attachments.len()
            );
            for emitter in &model.particle_emitters2 {
                println!(
                    "  PRE2 {:28} filter={:?} atlas={}x{} life={:.3} rate={:.1} prio={} repl={} tex={} tracks={}",
                    esc(&emitter.node.name),
                    emitter.filter(),
                    emitter.columns,
                    emitter.rows,
                    emitter.life_span,
                    emitter.emission_rate,
                    emitter.priority_plane,
                    emitter.replaceable_id,
                    emitter.texture_id,
                    track_count(emitter)
                );
            }
            for ribbon in &model.ribbon_emitters {
                println!(
                    "  RIBB {:28} above={:.1} below={:.1} life={:.3} rate={} atlas={}x{} gravity={:.2}",
                    esc(&ribbon.node.name),
                    ribbon.height_above,
                    ribbon.height_below,
                    ribbon.life_span,
                    ribbon.emission_rate,
                    ribbon.columns,
                    ribbon.rows,
                    ribbon.gravity
                );
            }
            for event in &model.event_objects {
                let (kind, id) = event.kind_and_id();
                println!(
                    "  EVTS {:28} kind={:?} id={} frames={:?}",
                    esc(&event.node.name),
                    kind,
                    id,
                    event.track.frames
                );
            }
            for attachment in &model.attachments {
                println!(
                    "  ATCH {:28} path={} object_id={}",
                    esc(&attachment.node.name), attachment.path, attachment.node.object_id
                );
            }
        }
    }
    if json {
        println!();
        println!("]");
    }
}

/// Minimal JSON string escape (quotes, backslashes, control chars).
fn esc(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn track_count(emitter: &wc3_formats::mdx::ParticleEmitter2) -> usize {
    [
        emitter.speed_track.is_some(),
        emitter.variation_track.is_some(),
        emitter.latitude_track.is_some(),
        emitter.gravity_track.is_some(),
        emitter.visibility_track.is_some(),
        emitter.emission_rate_track.is_some(),
        emitter.width_track.is_some(),
        emitter.length_track.is_some(),
    ]
    .iter()
    .filter(|present| **present)
    .count()
}

#[allow(dead_code)]
fn filter_name(filter: ParticleFilter) -> &'static str {
    match filter {
        ParticleFilter::Blend => "Blend",
        ParticleFilter::Additive => "Additive",
        ParticleFilter::Modulate => "Modulate",
        ParticleFilter::Modulate2x => "Modulate2x",
        ParticleFilter::AlphaKey => "AlphaKey",
    }
}
