//! Parse every extracted WC3 FDF and report node/property/include counts.
use std::path::{Path, PathBuf};
use wc3_formats::fdf::FdfDocument;

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("fdf")) == Some(true) {
                out.push(path);
            }
        }
    }
}

fn count(node: &wc3_formats::fdf::FdfNode) -> (usize, usize) {
    let mut nodes = 1;
    let mut props = node.props.len();
    for child in &node.children {
        let (n, p) = count(child);
        nodes += n;
        props += p;
    }
    (nodes, props)
}

fn main() {
    let root = std::env::args().nth(1).unwrap_or_else(|| "analysis/war3_data".into());
    let mut files = Vec::new();
    walk(Path::new(&root), &mut files);
    files.sort();
    let mut total_nodes = 0usize;
    let mut total_props = 0usize;
    let mut total_includes = 0usize;
    let mut failed = 0usize;
    for path in &files {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        match FdfDocument::parse(&text) {
            Ok(document) => {
                let mut nodes = 0;
                let mut props = 0;
                for node in &document.nodes {
                    let (n, p) = count(node);
                    nodes += n;
                    props += p;
                }
                total_nodes += nodes;
                total_props += props;
                total_includes += document.includes.len();
                println!(
                    "{} top={} nodes={} props={} includes={}",
                    path.strip_prefix(&root).unwrap_or(path).display(),
                    document.nodes.len(),
                    nodes,
                    props,
                    document.includes.len()
                );
            }
            Err(error) => {
                failed += 1;
                println!("FAIL {} {error}", path.display());
            }
        }
    }
    println!(
        "TOTAL files={} nodes={} props={} includes={} failed={}",
        files.len(),
        total_nodes,
        total_props,
        total_includes,
        failed
    );
}
