//! artD_animation — Agent D animation layer evidence (ledger 588, art push 2026-11-21).
//!
//! Pins the per-sequence geoset visibility rule that fixes the "all geosets
//! drawn" defect measured in `analysis/agent-render/MODEL_LOD_REPORT.md` §3–§5:
//! a Peon with no running sequence draws 5 geosets / 6 draws (two carry
//! variants + gore), a Peon with a Stand sequence draws 2 geosets / 3 draws.
//! The build ghost is a building drawn through the same `Assets::mesh_alpha`
//! contract, so the second test pins the building-side visibility with and
//! without a running sequence.

use wc3_anim::runtime::{mesh_alphas, ModelRuntime};
use wc3_formats::mdx::MdxModel;

fn model_path(relative: &str) -> Option<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for tree in ["analysis/war3_samples", "analysis/war3_full", "analysis/war3_full3"] {
        let candidate = root.join(tree).join(relative);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn load(relative: &str) -> Option<MdxModel> {
    let bytes = std::fs::read(model_path(relative)?).expect("read MDX fixture");
    Some(MdxModel::parse(&bytes).expect("parse MDX fixture"))
}

fn sequence<'a>(model: &'a MdxModel, name: &str) -> &'a wc3_formats::mdx::Sequence {
    model
        .sequences
        .iter()
        .find(|candidate| candidate.name == name)
        .or_else(|| {
            model
                .sequences
                .iter()
                .find(|candidate| candidate.name.eq_ignore_ascii_case(name))
        })
        .unwrap_or_else(|| {
            panic!(
                "sequence {name:?} not in {:?}",
                model
                    .sequences
                    .iter()
                    .map(|sequence| sequence.name.as_str())
                    .collect::<Vec<_>>()
            )
        })
}

/// One draw per visible (geoset, material layer) pair — the renderer's rule
/// (`wc3-render/src/mesh.rs` draw loop) used to produce the report's draw
/// counts.
fn draw_count(model: &MdxModel, alphas: &[f32]) -> usize {
    alphas
        .iter()
        .enumerate()
        .filter(|(_, alpha)| **alpha > 0.001)
        .map(|(index, _)| {
            model
                .geosets
                .get(index)
                .and_then(|geoset| model.materials.get(geoset.material_id as usize))
                .map(|material| material.layers.len().max(1))
                .unwrap_or(1)
        })
        .sum()
}

fn visible(alphas: &[f32]) -> Vec<usize> {
    alphas
        .iter()
        .enumerate()
        .filter(|(_, alpha)| **alpha > 0.001)
        .map(|(index, _)| index)
        .collect()
}

/// Exact expected arrays from `MODEL_LOD_REPORT.md` §4 (Peon / `opeo`, 5
/// geosets: body, pack, gold-carry, lumber-carry, gore).
const PEON_NONE: [f32; 5] = [1.0, 1.0, 1.0, 1.0, 1.0];
const PEON_SEQUENCE: [f32; 5] = [1.0, 1.0, 0.0, 0.0, 0.0];

#[test]
fn artD_sequence_alphas() {
    let Some(model) = load("Units/Orc/Peon/peon.mdx") else {
        eprintln!("ARTD_SEQUENCE_ALPHAS SKIP: peon.mdx fixture absent");
        return;
    };
    assert_eq!(model.geosets.len(), 5, "Peon geoset count changed");
    let runtime = ModelRuntime::new(&model);
    let hidden = &runtime.helper_planes;

    // No running sequence: static GEOA alpha only, no per-sequence extent gate.
    // This is the defective state the game used to reach whenever a unit had no
    // Visual or its animator had not started a sequence.
    let none_at_start = mesh_alphas(&model, hidden, None, 0);
    assert_eq!(none_at_start, PEON_NONE, "Peon static (no-sequence) alphas");
    assert_eq!(visible(&none_at_start), vec![0, 1, 2, 3, 4]);
    assert_eq!(draw_count(&model, &none_at_start), 6, "report §3 'before'");

    // Every real sequence resolves to the same two-geoset Stand set: the extent
    // gate hides the two carry variants and the gore geoset.
    for name in ["Stand", "Walk", "Attack", "Death", "Decay Flesh"] {
        let sequence = sequence(&model, name);
        // The report's table is sampled at sequence.start: the two-geoset
        // Stand set hides both carry variants and the gore geoset there.
        let at_start = mesh_alphas(&model, hidden, Some(sequence), sequence.start);
        assert_eq!(at_start, PEON_SEQUENCE, "Peon seq={name} at start");
        assert_eq!(visible(&at_start), vec![0, 1], "Peon visible seq={name}");
        assert_eq!(draw_count(&model, &at_start), 3, "report §3 'after'");
        assert_eq!(runtime.alphas(&model, Some(sequence), sequence.start), at_start);

        for time in [(sequence.start + sequence.end) / 2, sequence.end] {
            let alphas = mesh_alphas(&model, hidden, Some(sequence), time);
            assert_eq!(runtime.alphas(&model, Some(sequence), time), alphas);
            if name == "Decay Flesh" {
                // Authored decay progression, not the all-geosets defect:
                // geoset 4 KGAO (43167,0.0)->(45533,1.0) reveals gore after
                // the report's start sample; geosets 0/1 KGAO
                // (43167,1.0)->(103167,0.0) fade body/pack out by the last
                // frame. flags=1 (non-looping) holds the last in-range key.
                let expected = if time == sequence.end {
                    [0.0, 0.0, 0.0, 0.0, 1.0]
                } else {
                    [1.0, 1.0, 0.0, 0.0, 1.0]
                };
                assert_eq!(alphas, expected, "Decay Flesh t={time}");
            } else {
                assert_eq!(alphas, PEON_SEQUENCE, "Peon seq={name} t={time}");
            }
            // Carry variants (geosets 2/3) are absent from all five sequences
            // by per-sequence extent and must stay hidden at every sample time.
            assert_eq!(&alphas[2..4], &[0.0, 0.0], "carry variant leaked");
        }
        eprintln!(
            "ARTD_SEQUENCE_ALPHAS seq={name:<12} alphas={:?} visible={:?} draws=3 PASS",
            at_start,
            visible(&at_start)
        );
    }
    eprintln!(
        "ARTD_SEQUENCE_ALPHAS seq=<none>    alphas={:?} visible={:?} draws=6 PASS",
        none_at_start,
        visible(&none_at_start)
    );
}

#[test]
fn artD_ghost_sequence() {
    // The build ghost is a building rendered through the same mesh_alpha
    // contract (game.rs ghost draw). On this fallback install the selected
    // ghost def (hgtw) has no reachable model, but the visibility rule is
    // model-generic: pin it on the two shipped town halls. GreatHall is the
    // Orc hall the report measured (19 geosets, 17 visible with no sequence,
    // 3 geosets / 4 draws at Stand).
    let Some(great_hall) = load("Buildings/Orc/GreatHall/GreatHall.mdx") else {
        eprintln!("ARTD_GHOST_SEQUENCE SKIP: GreatHall.mdx fixture absent");
        return;
    };
    assert_eq!(great_hall.geosets.len(), 19, "GreatHall geoset count changed");
    let runtime = ModelRuntime::new(&great_hall);
    let hidden = &runtime.helper_planes;

    let none = mesh_alphas(&great_hall, hidden, None, 0);
    let none_visible = visible(&none);
    assert_eq!(none_visible.len(), 17, "report §3 GreatHall 'before': 17 visible");
    assert_eq!(draw_count(&great_hall, &none), 20, "report §3 GreatHall 'before' draws");

    let stand = sequence(&great_hall, "Stand");
    let stand_alphas = mesh_alphas(&great_hall, hidden, Some(stand), stand.start);
    let stand_visible = visible(&stand_alphas);
    assert_eq!(stand_visible.len(), 3, "report §3 GreatHall 'after': 3 geosets");
    assert_eq!(draw_count(&great_hall, &stand_alphas), 4, "report §3 'after' draws");
    assert_eq!(runtime.alphas(&great_hall, Some(stand), stand.start), stand_alphas);

    // The doubled-ghost failure mode: a geoset drawn with no sequence but
    // hidden at Stand (upgrade stages / rubble). With a running sequence none
    // of them may leak, at any sample time.
    let leaked: Vec<usize> = none_visible
        .iter()
        .copied()
        .filter(|index| !stand_visible.contains(index))
        .collect();
    assert!(!leaked.is_empty(), "fixture no longer distinguishes the defect");
    for time in [stand.start, (stand.start + stand.end) / 2, stand.end] {
        let alphas = mesh_alphas(&great_hall, hidden, Some(stand), time);
        for index in &leaked {
            assert!(
                alphas[*index] <= 0.001,
                "ghost geoset {index} leaked into Stand at t={time}"
            );
        }
    }
    eprintln!(
        "ARTD_GHOST_SEQUENCE model=GreatHall none_visible={none_visible:?} none_draws=20 stand_visible={stand_visible:?} stand_draws=4 leaked_hidden={leaked:?} PASS"
    );

    // Second building: TownHall Stand draws exactly geosets 11/13/14 (already
    // pinned in runtime.rs); its no-sequence frame draws every non-helper
    // geoset. Both paths go through the same default-sequence contract.
    let Some(town_hall) = load("Buildings/Human/TownHall/TownHall.mdx") else {
        eprintln!("ARTD_GHOST_SEQUENCE TownHall fixture absent — GreatHall evidence stands");
        return;
    };
    let town_runtime = ModelRuntime::new(&town_hall);
    let town_stand = sequence(&town_hall, "Stand");
    let town_alphas = mesh_alphas(&town_hall, &town_runtime.helper_planes, Some(town_stand), town_stand.start);
    assert_eq!(visible(&town_alphas), vec![11, 13, 14]);
    let town_none = mesh_alphas(&town_hall, &town_runtime.helper_planes, None, 0);
    assert!(
        visible(&town_none).len() > visible(&town_alphas).len(),
        "no-sequence TownHall must draw more than its Stand set"
    );
    eprintln!(
        "ARTD_GHOST_SEQUENCE model=TownHall stand_visible={:?} none_visible={:?} PASS",
        visible(&town_alphas),
        visible(&town_none)
    );
}
