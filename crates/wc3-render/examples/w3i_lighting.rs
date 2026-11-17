//! Print the lighting/fog block of a `war3map.w3i`.
//!
//! ```text
//! scripts/h2-cargo run -p wc3-render --example w3i_lighting -- <war3map.w3i>
//! ```
//!
//! Diagnostic for LEDGER 476: the renderer consumes `MapLighting` at startup, so
//! this prints exactly what the frame will use (version, light environment,
//! fog colour/density/distance) without booting the game.

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: w3i_lighting <war3map.w3i>");
        std::process::exit(2);
    });
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("read {path}: {error}");
            std::process::exit(1);
        }
    };
    match wc3_render::daynight::MapLighting::from_w3i(&bytes) {
        Ok(map) => {
            println!("version={}", map.version);
            println!(
                "light_environment={}",
                map.light_environment
                    .map(|letter| letter.to_string())
                    .unwrap_or_else(|| "-".to_string())
            );
            match map.fog {
                Some(fog) => println!(
                    "fog colour={:.4},{:.4},{:.4} density={:.4} start={:.1} end={:.1}",
                    fog.base_color[0], fog.base_color[1], fog.base_color[2], fog.density,
                    fog.start, fog.end
                ),
                None => println!("fog none"),
            }
            if let Some(letter) = map.light_environment {
                let preset = wc3_render::daynight::Preset::for_tileset(letter);
                println!("preset={}", preset.name());
            }
        }
        Err(error) => {
            eprintln!("parse {path}: {error}");
            std::process::exit(1);
        }
    }
}
