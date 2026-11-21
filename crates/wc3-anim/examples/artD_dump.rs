//! Agent D (all-layer push 2026-11-21): dump the port's per-sequence geoset
//! visibility for every MDX under a tree, so the independent Python parser
//! (`analysis/agent-anim/artD_crosscheck_20261121.py`) can compare it against
//! the shipped v800 bytes.
//!
//! Usage: `cargo run -p wc3-anim --example artD_dump -- analysis/war3_full3`
//!
//! Output (tab separated, one record per line):
//! ```text
//! M <relpath> <geosets> <sequences> <geoset_animations> <textures> <bones>
//! S <relpath> <seq_index> <start> <end> <flags> <alphas@start> <alphas@mid> <alphas@end>
//! E <relpath> <read|parse error>
//! ```
//! Alpha lists are comma separated debug floats (shortest round-trip form).

use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use wc3_anim::runtime::ModelRuntime;
use wc3_formats::mdx::MdxModel;

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mdx"))
        {
            out.push(path);
        }
    }
}

fn alphas(values: &[f32]) -> String {
    let mut out = String::new();
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("{value:?}"));
    }
    out
}

fn main() {
    let root_arg = std::env::args().nth(1).expect("tree root argument");
    let root = PathBuf::from(&root_arg);
    let mut files = Vec::new();
    collect(&root, &mut files);
    files.sort();

    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    for path in &files {
        let relative = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                writeln!(out, "E\t{relative}\tread:{error}").ok();
                continue;
            }
        };
        let model = match MdxModel::parse(&bytes) {
            Ok(model) => model,
            Err(error) => {
                writeln!(out, "E\t{relative}\tparse:{error:?}").ok();
                continue;
            }
        };
        let runtime = ModelRuntime::new(&model);
        writeln!(
            out,
            "M\t{relative}\t{}\t{}\t{}\t{}\t{}",
            model.geosets.len(),
            model.sequences.len(),
            model.geoset_animations.len(),
            model.textures.len(),
            model.bones.len()
        )
        .ok();
        for (index, sequence) in model.sequences.iter().enumerate() {
            // Shipped data carries intervals with `end < start` (a broken
            // exporter or an unused placeholder); the dump must not panic on
            // them, and the crate itself only ever sees these as plain times.
            let mid = sequence
                .start
                .saturating_add(sequence.end.saturating_sub(sequence.start) / 2);
            let at_start = runtime.alphas(&model, Some(sequence), sequence.start);
            let at_mid = runtime.alphas(&model, Some(sequence), mid);
            let at_end = runtime.alphas(&model, Some(sequence), sequence.end);
            writeln!(
                out,
                "S\t{relative}\t{index}\t{}\t{}\t{}\t{}\t{}\t{}",
                sequence.start,
                sequence.end,
                sequence.flags,
                alphas(&at_start),
                alphas(&at_mid),
                alphas(&at_end)
            )
            .ok();
        }
    }
    out.flush().ok();
}
