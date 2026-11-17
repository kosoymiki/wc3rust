//! `assets.anim.skinning`: the exported GLBs carry real skins (joints, inverse
//! bind matrices) and sampled animation tracks, verified on a representative
//! set that includes every class of in-game model (melee, ranged, caster hero,
//! melee hero, building, projectile). The numeric round-trip render proof is
//! produced by `analysis/agent-anim/gltf-verify/render_glb.py`; this test pins
//! the artifacts it depends on.

use std::path::{Path, PathBuf};

fn gltf_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../analysis/agent-anim/gallery/gltf-roster")
}

fn glb(relative: &str) -> PathBuf {
    gltf_dir().join(relative).with_extension("glb")
}

/// Animation entries are serialized as `{"name":"<clip>","samplers":...}`.
fn animation_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = text;
    while let Some(position) = rest.find("\"name\":\"") {
        rest = &rest[position + 8..];
        let Some(end) = rest.find('"') else { break };
        let name = &rest[..end];
        if rest[end..].starts_with("\",\"samplers\"") {
            names.push(name.to_string());
        }
        rest = &rest[end..];
    }
    names
}

#[test]
fn assets_anim_skinning_representative_glbs_have_skins_and_clips() {
    let cases: &[(&str, &[&str])] = &[
        ("Units/Human/Footman/Footman.mdx", &["Stand", "Walk", "Attack", "Death"]),
        ("Units/NightElf/Archer/Archer.mdx", &["Walk", "Attack", "Death"]),
        (
            "Units/Human/HeroArchMage/HeroArchMage.mdx",
            &["Stand", "Walk", "Attack", "Death", "Spell", "Stand Channel"],
        ),
        (
            "Units/Orc/HeroBladeMaster/HeroBladeMaster.mdx",
            &["Stand", "Attack Walk Stand Spin", "Death"],
        ),
        ("Buildings/Human/TownHall/TownHall.mdx", &["Stand", "Death"]),
        ("Abilities/Weapons/FireBallMissile/FireBallMissile.mdx", &["Stand", "Death"]),
    ];
    let mut checked = 0usize;
    for (model, required_clips) in cases {
        let path = glb(model);
        if !path.is_file() {
            panic!(
                "missing GLB {} (run the exporter CLI before this gate)",
                path.display()
            );
        }
        let bytes = std::fs::read(&path).expect("read GLB");
        assert_eq!(&bytes[..4], b"glTF", "not a GLB: {model}");
        let text = String::from_utf8_lossy(&bytes);
        // Structural presence of the glTF skin/animation sections. Exact
        // numeric sampling is verified by the independent Python renderer.
        assert!(text.contains("\"skins\""), "{model}: no skins section");
        assert!(text.contains("\"inverseBindMatrices\""), "{model}: no inverse bind matrices");
        assert!(text.contains("\"animations\""), "{model}: no animations section");
        assert!(text.contains("\"JOINTS_0\""), "{model}: no joint attribute");
        assert!(text.contains("\"WEIGHTS_0\""), "{model}: no weight attribute");
        let clips = animation_names(&text);
        for required in *required_clips {
            let needle = required.to_ascii_lowercase();
            assert!(
                clips
                    .iter()
                    .any(|clip| clip.to_ascii_lowercase().contains(&needle)),
                "{model}: authored clip {required:?} missing from the GLB (clips: {clips:?})"
            );
            checked += 1;
        }
    }
    assert!(checked >= 20, "expected broad clip coverage, got {checked}");
    eprintln!("skinning: {checked} authored clips present across representative GLBs");
}
