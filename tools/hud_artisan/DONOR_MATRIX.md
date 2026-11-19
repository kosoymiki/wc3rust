# HUD Artisan — donor matrix and implementation decisions

Research snapshot: 2026-11-19. This is a curated engineering matrix, not a claim of exhaustive GitHub indexing. Links are upstream references; no third-party code is copied by this document. License labels below are the upstream repository's declared license or the status observed in its public project metadata. Re-check the exact LICENSE file and every asset/model license at the commit you intend to use.

## Decision summary

| Area | Preferred donor/reference | License posture | Decision for HUD Artisan |
|---|---|---|---|
| End-to-end asset pipeline | [stable-diff-game-asset-generator](https://github.com/JoeyHammoth/stable-diff-game-asset-generator) | MIT (upstream declares) | **Adopt architecture**: declarative jobs, renderer boundary, deterministic postprocess, pack/provenance contracts. Do not copy wholesale; audit files and retain notices. |
| Local-first brain/muscle split | [AssetsMaker](https://github.com/caioross/AssetsMaker) | License not verified in this pass | **Architecture reference only**: OpenCode plans and reviews; local renderer produces pixels. No code reuse until LICENSE is verified. |
| Offline post-processing / alpha / palette / QA | [AssetForge](https://github.com/doriansao/AssetForge) | MIT code reported; weights and LoRAs have separate terms | **Adopt measured QA concepts**; never infer model license from repository license. |
| Asset family / frame consistency | [sprite-gen](https://github.com/aldegad/sprite-gen) | Apache-2.0 (upstream declares) | **Adopt contracts only**: shared identity, state family, extraction and curation. Exclude paid-provider integrations. |
| General asset orchestration | [OpenGameForge](https://github.com/joshbarros/open-game-forge) | Verify exact LICENSE before reuse | **Reference only**: resumable stage ledger and artifact ownership; reject RunPod lifecycle as default. |
| Reusable game-art agent skill | [game-art-pipeline](https://github.com/cuteyuchen/game-art-pipeline) | Verify exact LICENSE before reuse | **Study workflow**: project profile → generation → deterministic QA → engine integration journal. No unverified code copy. |
| Atlas packer | [FastPack](https://github.com/Hexeption/FastPack) | MIT (GitHub metadata) | **Benchmark / optional external tool**: MaxRects, trim, extrusion, nine-patch, alias detection. Do not replace our fixed manifest atlas without a deterministic compatibility test. |
| Atlas packer (simple CLI) | [texpack](https://github.com/urraka/texpack) | MIT (GitHub metadata) | **Fallback reference** for MaxRects CLI packing; evaluate output determinism and metadata before adoption. |
| Atlas packer (small static algorithm) | [potpack](https://github.com/mapbox/potpack) | Verify package LICENSE before reuse | **Algorithm reference** only; static near-square packing, not sufficient by itself for WC3-specific metadata. |
| Atlas packer (C++ allocator) | [smol-atlas](https://github.com/aras-p/smol-atlas) | MIT or Unlicense per upstream README | **Algorithm reference**; dynamic allocator is not our offline deterministic build requirement. |
| Warcraft III texture conversion | [blp-rs](https://github.com/WarRaft/blp-rs) | Repository contains LICENSE; verify exact SPDX before dependency | **High-priority Rust integration candidate** for BLP decode/encode and CLI conversion; validate BLP1 specifically against target WC3 version. |
| Warcraft III texture format API | [wow-blp crate](https://docs.rs/wow-blp/latest/wow_blp/) | Inspect crate metadata/license before dependency | **Independent cross-check** for BLP0/1/2 and compression/mipmap coverage. |
| ComfyUI reference conditioning | [ComfyUI IPAdapter Plus](https://github.com/cubiq/ComfyUI_IPAdapter_plus) | Check upstream LICENSE and model-weight terms separately | **Optional local node** for style/reference conditioning; no requirement for core. |
| Official ComfyUI core | [ComfyUI](https://github.com/comfy-org/ComfyUI) | See upstream LICENSE; model assets separate | **Optional local backend only**. Core runs offline; paid partner/API nodes must be disabled. |
| Segmentation / alpha matting | [rembg](https://github.com/danielgatis/rembg) | MIT code; each model has its own terms | **Optional fallback**, not default for ornate frames: automatic masks can erase thin filigree and produce edge halos. |
| ComfyUI game-dev workflows | [ComfyUI-GameDev](https://github.com/ecalot/ComfyUI-GameDev) | GPL-3.0 (upstream declares) | **Study algorithms only** unless project intentionally accepts GPL-3.0 compatibility obligations. |
| Spritekiln workflow catalogue | [spritekiln](https://github.com/brunocpf/spritekiln) | GPL-3.0 (upstream declares) | **Study only**; distinguish pixel-art operations from our painted HD style. |
| ComfyUI game assets maker | [GameAssetsMaker](https://github.com/quinteroac/ComfyUI-GameAssetsMaker) | License not verified | **Read-only reference** for Aseprite tags / atlas metadata; no code copy. |
| 2D asset pipeline | [2d-assets-pipeline](https://github.com/AdielMag/2d-assets-pipeline) | License not verified | **Read-only reference** for decomposition, segmentation and 9-slice editing. |
| Pixel-native evaluation | [loom](https://github.com/hongyuanc/loom) | MIT (upstream README states) | **Adopt evaluation ideas only**: package evals, alpha/palette/bounds checks, deterministic local renderer. Not a general painted HUD renderer. |

## Implementation matrix — what we actually build

| Capability | Current HUD Artisan state | Upgrade decision | Acceptance evidence |
|---|---|---|---|
| Manifest-first planning | Implemented; real normalized manifest has 92 PNG asset records, 5 fonts and 20 atlas entries | Keep manifest authoritative; never infer unknown dimensions/insets | Plan compiles 91 unique generation jobs + 20 atlas source cells |
| Renderer abstraction | Implemented CLI protocol + optional local ComfyUI adapter | Keep core backend-neutral; add capability negotiation before using reference/control features | Mock adapter contract tests + local smoke render |
| Reference conditioning | Research only / adapter workflow-specific | Optional capability flags: image reference, mask, edge/depth guide, inpaint; no hard dependency | A/B test same family with/without conditioning; record hashes and settings |
| Alpha and silhouette QA | Basic structural/pixel checks exist | Add alpha bounding boxes, aperture masks, edge-halo diagnostics and light/dark composites as report-only or calibrated gates | Fixture suite with transparent holes, thin edges, halo and empty alpha |
| Native-size readability | Vision/human review protocol | Require 100%, 50%, 25% contact sheets and semantic silhouette check at target display size | Art Director + Red Team raw reports + human approval |
| Nine-slice validation | Manifest inset checks exist | Add explicit stretch-band quietness metric only after defining per-asset masks; never globally blur/simplify | Test fixtures for corner detail and stretch seams |
| Atlas | Fixed coordinates from manifest; deterministic assembly | Keep fixed 2048² contract; add extrusion only if runtime sampling/filtering proves edge bleed | Pixel-exact cell crops, no overlap, atlas checksum, runtime sampling test |
| BLP export | Not yet proven in current HUD Artisan path | Add Rust BLP1 export/round-trip stage after target client version is confirmed | Decode→encode→decode comparison, alpha test vectors, in-game test |
| Fonts | Five external runtime files required | Track font family, version, license, source and checksum independently | Font inventory + license ledger + packaging test |
| Reproducibility | Prompt/job/adapter/output hashes and resume checkpoints implemented | Extend provenance with checkpoint/LoRA/ControlNet hashes, workflow hash, node package lock and git commit | Same inputs reproduce same postprocessed output; unknown fields remain null |
| Review integrity | Two roles, separate model/session IDs, raw response hashes | Retain evidence gate; do not demand dishonest 10/10 for every criterion. Use blocker threshold + documented score threshold after calibration | Calibrated rubric fixture tests; no claim of cryptographic reviewer independence |
| Local operation | Optional local ComfyUI adapter | No paid endpoints, hosted GPUs, subscriptions, secrets or telemetry in default path | Offline smoke test; network disabled except loopback backend |
| Engine integration | Engine profile and pack checks exist | Add BLP/FDF/resource loader round-trip and capture screenshots | Rust tests + target WC3 version runtime screenshots |

## Reference-conditioning strategy

Use a small, curated set of project-owned master assets, not dozens of unrelated reference images.

1. **Style reference**: approved material board and approved master frame. Apply low/medium style weight so the prompt controls semantic object and silhouette.
2. **Geometry reference**: line art, binary silhouette or authored layout mask. Prefer ControlNet-like structural conditioning where the selected local model supports it.
3. **Mask/inpaint reference**: only for controlled edits to an already approved asset. Keep unmasked pixels unchanged and compare hashes outside the edit mask.
4. **State-family reference**: generate normal state first; derive hover/pressed/disabled from the same base geometry using deterministic compositing/material adjustments where possible. Do not redraw each state independently.
5. **Reference exclusion**: unknown-license retail images, extracted game UI, screenshots with protected art, and fonts without a clear license must not enter conditioning. High-level textual study is permitted; pixel conditioning is not.

IP-Adapter is an optional local capability, not a promise of exact asset geometry. The official node documentation notes that reference images condition style/composition/subject and that masks can limit influence. Its weight files and CLIP vision encoders require separate provenance and license checks. ControlNet is likewise optional; use edge/depth/segmentation guides only where the model/workflow actually supports them.

## Warcraft III-specific technical references

- [OpenWarcraft3 FDF reference](https://corepunch.github.io/open-realm/games/warcraft-3/file-formats/fdf/): FDF is a declarative UI frame hierarchy describing dimensions, anchors, textures, fonts and child frames. Use this as a runtime-layout contract reference, not as proof that every field behaves identically across all retail client versions.
- [WarRaft/blp-rs](https://github.com/WarRaft/blp-rs): Rust BLP reader/writer candidate. Test BLP1 for the exact WC3 target; BLP2 support alone is not sufficient evidence.
- [wow-blp crate docs](https://docs.rs/wow-blp/latest/wow_blp/): independent format coverage reference for BLP0/1/2, alpha modes, mipmaps and compression.
- [ComfyUI official repository](https://github.com/comfy-org/ComfyUI): local graph execution and local API are built into the open-source core. Its README explicitly distinguishes paid Comfy Cloud and optional API nodes from offline local execution.

Do not import WoW-only XML Backdrop behavior as Warcraft III FDF behavior. The two games have related texture formats but different UI systems.

## Research papers

- [ControlNet (ICCV 2023)](https://arxiv.org/abs/2302.05543): structural conditioning from edges, depth, segmentation and other spatial controls. Use it to constrain silhouette/layout, then verify pixels independently.
- [IP-Adapter (arXiv:2308.06721)](https://arxiv.org/abs/2308.06721): image prompt conditioning alongside text. Useful for shared material/style references; model/base pairing must be pinned.
- [RefDrop (NeurIPS 2024)](https://arxiv.org/abs/2405.17661): reference feature guidance as a consistency technique; experimental candidate, not a production guarantee.
- [ControlNet++ (ECCV 2024)](https://arxiv.org/abs/2404.07987): consistency feedback for conditional control; motivates comparing output against structural input.

These papers do not demonstrate Warcraft III asset production. They support controlled experiments, not release claims.

## License policy — mandatory

1. **Code license ≠ model license ≠ dataset license ≠ font license ≠ generated-output rights.** Record each independently.
2. No code copy from unknown-license repositories. No copying GPL-3.0 code into this project unless the repository owner explicitly chooses compatible distribution obligations.
3. Before adapting MIT/Apache code, pin the upstream commit, save LICENSE and NOTICE, record copied files and retain copyright notices.
4. Before using any checkpoint, LoRA, ControlNet, VAE, CLIP vision encoder or embedding, record its exact model card URL, revision, file SHA-256, license and commercial-use restrictions.
5. A reference with unknown or incompatible rights may be a textual/structural inspiration only; it is not allowed as pixel conditioning.
6. Every generated output records the actual model license and reference set. Do not use a generic label such as "AI generated" as a license.
7. Do not bundle model weights or font files until redistribution rights are verified.

## Priority order

**P0 — already implemented / retain:** deterministic manifest plan, job hashes, local renderer adapter, transactional import, atlas coordinate assembly, provenance, resumable dispatch, schema-backed review evidence, CI matrix.

**P1 — implement next:** reference ledger + schema; alpha diagnostics; contact-sheet variants; workflow/model/node lock metadata; BLP1 conversion test harness; README correction and test coverage.

**P2 — only after first real render:** structural-guide conditioning, IP-Adapter A/B workflow, nine-slice visual metric, halo correction, state derivation experiments.

**P3 — after target client confirmation:** BLP1 packaging, FDF binding integration, in-game screenshots and runtime atlas sampling.

## Known limits and release blockers

- No actual local ComfyUI render has yet been verified in this branch.
- The 92 PNG and five font payloads remain local Termux pack data; manifest presence does not mean binary payload sync.
- A GitHub CI pass validates code/contracts only, not image quality or Warcraft III runtime compatibility.
- Reviewer IDs/session hashes improve auditability but cannot prove different model weights, isolated context or honest human attention.
- Do not mark the asset pack "done" until local render, visual inspection, complete payload import, BLP/FDF integration and in-game screenshots pass.
