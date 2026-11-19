# HUD Artisan — donor and research review

Research snapshot: 2026-11-19. This is a curated engineering survey, not a claim that every GitHub repository or forum has been exhaustively indexed. Repositories are references/donor candidates; no third-party implementation is copied by this document. The actionable donor/implementation matrix is in DONOR_MATRIX.md. The machine-readable source/license ledger contract is reference_ledger.schema.json with a conservative example in reference_ledger.example.json.

## Architecture decisions

1. Keep HUD Artisan provider-neutral. The manifest, job compiler, provenance, PNG validator, atlas builder, importer and review gate must not depend on any one image UI, hosted endpoint, account, API key or subscription.
2. Treat image generation as an unreliable upstream proposal. Exact geometry, alpha, palette, framing, atlas packing, 9-slice checks, normalization, hashing and packaging are deterministic local post-processing.
3. Never ask an image model to draw the final command-button atlas. Generate/crop each cell independently, then assemble it using manifest coordinates.
4. Use references to constrain a family, not to clone a protected game's pixels. Reference files need source, license and purpose metadata.
5. Measure actual results before adopting a post-process. A plausible algorithm can destroy silhouettes or details at native UI size.
6. A model's self-score is not objective QA. Vision reviews need exact image hashes, independent raw evidence and human release approval.

## Donor repositories

### Strong implementation donors

- [JoeyHammoth/stable-diff-game-asset-generator](https://github.com/JoeyHammoth/stable-diff-game-asset-generator) — MIT. Closest architectural match: declarative asset specs, engine-neutral output packs, versioned deterministic post-processing, cache/resume, validation, audit and review. Adopt the separation of generation from deterministic post-processing, resumable job ledger and per-stage contracts. Its renderer transport is ComfyUI-specific; do not inherit that dependency. Inspect its schema and tests before selectively adapting any code.
- [doriansao/AssetForge](https://github.com/doriansao/AssetForge) — MIT code license. Particularly valuable for measured post-processing: pixel-grid detection, palette locking, alpha matting, crop/anchor normalization, tile seam measurements and explicit findings explaining rejected approaches. Do not assume model weights share the repository's MIT license; the project documents distinct licenses per checkpoint/LoRA.
- [aldegad/sprite-gen](https://github.com/aldegad/sprite-gen) — Apache-2.0. Strong reference for row-by-row state generation, identity preservation, alpha cleanup, frame extraction, runtime frame-layout metadata and human curation. Its showcased video/image providers may require paid API access; those provider calls are not a fit for the no-new-service constraint. Borrow the file contracts and QA concepts, not its provider dependency.

### Useful algorithm/workflow references; license caution

- [brunocpf/spritekiln](https://github.com/brunocpf/spritekiln) — GPL-3.0. Large 2D-art workflow catalogue and useful reference for HD/pixel art separation, rotations, sprite layers, palette operations, normal maps and model-free post-processing. Study concepts and compare outputs; do not copy code into a differently licensed project without an explicit GPL compatibility decision.
- [ecalot/ComfyUI-GameDev](https://github.com/ecalot/ComfyUI-GameDev) — GPL-3.0. Useful algorithms for grid slicing, nearest-neighbour resizing, dithering and depth-to-layer mapping. Treat as a workflow reference unless the project deliberately accepts GPL-3.0 obligations.
- [quinteroac/ComfyUI-GameAssetsMaker](https://github.com/quinteroac/ComfyUI-GameAssetsMaker) — useful Aseprite atlas/tag schemas, batch slicing, animation metadata and VLM-assisted frame-rectangle audits. A license file was not found during this pass; do not copy code until licensing is verified.
- [AdielMag/2d-assets-pipeline](https://github.com/AdielMag/2d-assets-pipeline) — useful reference for screenshot decomposition, HQ-SAM extraction, alpha matting, 9-slice editing and engine atlas export. A license file was not found during this pass; reference architecture only, no code reuse.
- [joshbarros/open-game-forge](https://github.com/joshbarros/open-game-forge) — useful for stage ledger, resumable generation, output hashing, transparent-background fallback and explicit infrastructure lifecycle. Its published example uses RunPod and is not our execution model.
- [caioross/AssetsMaker](https://github.com/caioross/AssetsMaker) — local-first agent/renderer split, project-level Style DNA, persistent memory and multi-project isolation. The documented brain/muscle separation is useful; verify its license before code reuse.

- [hongyuanc/loom](https://github.com/hongyuanc/loom) — MIT (as stated by its README). Valuable for strategy selection, deterministic local rendering, package-level eval reports, alpha/palette/bounding-box consistency checks, provenance and provider-pixel accounting. Its promoted pixel-native path uses curated indexed templates for a supported slime archetype; it is not a general painterly HUD renderer. Borrow evaluation/package architecture, not the pixel-art renderer as our visual solution.

### Additional focused donors

- [Hexeption/FastPack](https://github.com/Hexeption/FastPack) — MIT (GitHub repository metadata). Rust atlas packer with MaxRects, trim modes, extrusion, nine-patch metadata, alias detection and alpha-bleed reduction. Strong benchmark candidate; do not replace the manifest's fixed atlas coordinates without pixel-exact compatibility tests.
- [urraka/texpack](https://github.com/urraka/texpack) — MIT (GitHub repository metadata). Small MaxRects CLI packer; useful fallback and algorithm comparison, but confirm output metadata and determinism before integration.
- [mapbox/potpack](https://github.com/mapbox/potpack) — compact static near-square rectangle packer. Useful algorithm reference; verify exact package license and remember it does not provide the full WC3 metadata contract.
- [aras-p/smol-atlas](https://github.com/aras-p/smol-atlas) — MIT or Unlicense per upstream README. Small dynamic C++ atlas allocator; algorithm reference only, not a direct fit for deterministic offline packaging.
- [WarRaft/blp-rs](https://github.com/WarRaft/blp-rs) — Rust BLP library/CLI for Blizzard texture formats. High-priority WC3 integration candidate; verify the exact repository LICENSE and test BLP1 against the target Warcraft III version before depending on it.
- [wow-blp crate documentation](https://docs.rs/wow-blp/latest/wow_blp/) — parser/encoder documentation for BLP0/1/2 and compression/mipmap variants. Independent technical cross-check; inspect crate metadata and exact license before adding it.
- [ComfyUI IPAdapter Plus](https://github.com/cubiq/ComfyUI_IPAdapter_plus) — reference conditioning node implementation. Optional local capability for style/composition/subject references; check node license and each model/CLIP-vision weight license independently.
- [ComfyUI core](https://github.com/comfy-org/ComfyUI) — local node-graph execution, local HTTP API, offline operation. Core and paid cloud are separate; optional partner/API nodes can be disabled. Model files and custom nodes remain separate dependencies.
- [cuteyuchen/game-art-pipeline](https://github.com/cuteyuchen/game-art-pipeline) — reusable agent skill describing profile → generation → deterministic QA → engine integration. License must be checked before code reuse; workflow reference only for now.

## Community workflow evidence (anecdotal, not peer-reviewed)

- [r/aigamedev: consistent 2D game assets by drawing onto a game screenshot, then isolating](https://www.reddit.com/r/aigamedev/comments/1xavthu/the_ai_trick_for_consistent_2d_game_assets/) — suggests conditioning on the actual project composition can improve placement/style coherence, followed by isolation and cleanup. For wc3rust, use only our own HUD screenshot or a neutral layout board as a composition guide; isolate each asset and validate it independently. Do not reuse retail game screenshots or bake the entire HUD into one generated texture.
- [r/comfyui: difficulties with genuine pixel-art consistency and fixed palettes](https://www.reddit.com/r/comfyui/comments/1wny6zq/true_pixel_art_game_asset_workflows_palettes/) — useful caution that "pixel-art-looking" output is not necessarily palette-limited, grid-consistent or engine-ready. Our accepted direction is HD Classic + Modern, not pixel art; therefore pixel snapping/dithering must not be applied globally.
- [r/comfyui: batch replacement using Florence/SAM segmentation](https://www.reddit.com/r/comfyui/comments/1xw3zo6/how_to_batchreplace_a_2d_game_character_across/) — the discussion describes text localization plus SAM-style masks for isolating a subject while preserving untouched props. Relevant only if we later need screenshot decomposition or asset extraction; it is not the default route for original HUD assets.
- [r/comfyui: local reference-conditioned animation workflow](https://www.reddit.com/r/comfyui/comments/1sve5id/one_image_in_2d_animated_and_customizable/) — useful architecture pattern: separate base identity, motion, alpha extraction and customization layers. It targets animation and large local GPU hardware, so do not copy its resource assumptions into the Termux/HD-HUD path.

## Warcraft III runtime and texture references

- [OpenWarcraft3 FDF reference](https://corepunch.github.io/open-realm/games/warcraft-3/file-formats/fdf/): FDF describes frame hierarchy, dimensions, anchors, textures, fonts and children. Treat as a format reference and validate behavior against the actual target client.
- [WarRaft/blp-rs](https://github.com/WarRaft/blp-rs) and [wow-blp docs](https://docs.rs/wow-blp/latest/wow_blp/): useful BLP format implementation references. BLP1 compatibility and alpha/mipmap round-trip must be tested on the selected WC3 version; do not infer WC3 behavior from WoW-only BLP2 documentation.
- [ComfyUI official README](https://github.com/comfy-org/ComfyUI): local core supports offline execution and local API workflows; paid Comfy Cloud and optional partner API nodes are distinct. Our default pipeline remains local and does not require cloud access.

## Research papers and technical basis

- [ControlNet — Adding Conditional Control to Text-to-Image Diffusion Models, arXiv:2302.05543 / ICCV 2023](https://arxiv.org/abs/2302.05543). Practical implication: text prompts alone are weak at exact composition. Keep silhouette masks, edge maps, depth maps, segmentation and layout constraints as optional explicit conditioning inputs when the selected renderer supports them. ControlNet is not a replacement for pixel-level validation.
- [IP-Adapter — Text Compatible Image Prompt Adapter, arXiv:2308.06721](https://arxiv.org/abs/2308.06721). Practical implication: combine a written job prompt with curated visual references using image conditioning, rather than expecting long text prompts to reproduce a material language. Pin the exact adapter/base-model pairing and record its weight/license.
- [RefDrop — Controllable Consistency via Reference Feature Guidance, NeurIPS 2024, arXiv:2405.17661](https://arxiv.org/abs/2405.17661). Practical implication: reference-feature guidance is a promising control axis for keeping subject/style consistent across variants. Treat it as an optional renderer capability and validate empirically on the actual icon/frame families.
- [ControlNet++ — Improving Conditional Controls with Efficient Consistency Feedback, ECCV 2024, arXiv:2404.07987](https://arxiv.org/abs/2404.07987). Practical implication: compare generated output against the input structural condition, not merely against the text prompt. A future HUD Artisan QA module can measure silhouette/mask/edge agreement where a structural guide exists.

These papers are general controllable image-generation research, not papers proving production-ready Warcraft III HUD generation. They justify conditioning and consistency experiments, not quality claims.

## Engineering implications for HUD Artisan

### A. Asset contract

Extend the manifest only with explicit, validated fields: semantic identity, family, state, layer/material intent, reference image paths and licenses, alpha contract, 9-slice insets, pivot/anchor, safe margins, intended background contrast and optional structural guide paths. Never derive critical engine semantics solely from filenames.

### B. Generation and reproducibility

Each job should record the exact renderer adapter version, model/checkpoint revision and hash, LoRA/control weights and hashes, sampler/scheduler, steps, CFG, dimensions, seed, prompt hashes, input reference hashes and output hash. If a renderer cannot expose a setting, record it as unknown rather than inventing a value. Keep a content-addressed cache keyed by the complete job specification, not only by the seed.

### C. Post-processing before review

Normalize alpha and color mode; inspect edge halos against both light and dark backgrounds; compute alpha bounds and transparent aperture masks; compare silhouette masks to structural guides; verify native-size readability; check 9-slice stretch bands; detect atlas overlaps/bleed; create nearest-neighbour and 100% previews. Do not blindly pixel-snap painted Warcraft-style HD assets: grid snapping is for assets whose art direction explicitly requires pixel art.

### D. QA

Separate deterministic tests from subjective review. Deterministic tests should reject malformed PNGs, wrong dimensions, empty alpha, invalid color mode, unsafe paths, unexpected files, incomplete job coverage, stale provenance, broken atlas geometry, missing fonts and illegal 9-slice insets. Vision review should judge semantics, material quality, lighting, hierarchy, family cohesion and originality. Human sign-off remains mandatory.

### E. Recovery and operations

Use an append-only run ledger with per-job states (planned, running, succeeded, failed, rejected, approved), atomic state writes, bounded retries, timeout process-tree termination, resumable outputs, stale-provenance invalidation and a clean distinction between temporary files and accepted pack artifacts.

## License and provenance policy

Repository code license does not grant rights to model weights, LoRAs, reference art, fonts or generated outputs. Track each independently. MIT/Apache-2.0 code is the preferred donor pool for selective adaptation; GPL-3.0 sources are algorithm references unless copyleft compatibility is intentionally accepted; projects with no verified license are read-only references. Every copied/adapted file must carry the required upstream notices and an attribution record.

## Current implementation status

Implemented in this branch: deterministic manifest planning, per-job prompt/seed hashes, provider-neutral CLI dispatch, strict PNG validation, atlas assembly, transactional ingestion, provenance binding, dual-role review evidence schema v2 with prompt/session/raw-response hashes, path/symlink protections, optional local HTTP adapter hardening, dependency-aware per-job resume checkpoints, failed-run stale-provenance invalidation, original visual reference board and OpenCode 2 role guidance.

Not yet proven: actual local image generation, artistic quality of the 92-asset pack, runtime screenshots in Warcraft III, syncing the 92 PNGs and five font files into the remote branch, and true independence of model review sessions. These remain release blockers, not assumptions.