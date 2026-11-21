# LEDGER 555 — HUD asset pipeline, renderer budget, debug overlay

## Goal

Turn the HUD art into a controlled pack: a v2 manifest the loader understands,
a strict nine-slice/atlas validator that refuses a bad pack instead of clamping
it, a glyph-coverage proof for Nimrod, a device-and-host pack builder, and a
debug overlay/frame budget that is off in release. Two deliveries arrived while
the lane ran and were processed through the same pipeline: the `hud-pack-v2`
donor (schema accepted, art rejected) and the operator's orc art sheet
(segmented into 21 assets).

No tests were written (freeze), no phone was touched, cargo ran only through
`scripts/h2-cargo`.

## Delivered

| layer | file(s) | state |
|---|---|---|
| manifest v1/v2 parser + strict validator | `crates/wc3-game/src/hudskin/manifest.rs` | new, compiled |
| loader integration, atlas UVs, v2 pages, hashes | `crates/wc3-game/src/hudskin/pack.rs` | rewritten load path |
| atlas pack builder + SVG rasterizer | `crates/wc3-game/src/hudskin/pack_build.rs` | new |
| CLI: build / validate / inspect / fonts / rasterize / overlay | `tools/hud-pack/src/main.rs`, `tools/hud-pack/Cargo.toml` | new workspace member |
| orc sheet segmentation tool + spec | `tools/hud_segment_sheet.py`, `tools/hud_orc.json` | new |
| debug overlay + frame budget | `crates/wc3-game/src/hud/debug.rs` (`pub mod debug` in `hud.rs`) | new |
| main v2 pack | `assets/hud/pack/{manifest.json,atlas/ui_atlas.png,assets/**,fonts/**}` | built |
| orc v2 pack | `assets/hud/orc/pack/{...}` | built |
| donor matrix | `analysis/agent-hud/DONOR-555-MATRIX.md` | new |
| orc report | `analysis/agent-hud/ORC-SHEET-555.md` | new |
| Nimrod coverage | `analysis/agent-hud/NIMROD-COVERAGE.md` | new |

## 1. Manifest v2 and loader

`manifest.rs` owns one parsed model (`Manifest`, `AssetDecl`, `AtlasDecl`,
`FontDecl`) for both formats:

- **v1** (`schema_version: 1`, the shipped 92-asset final pack): top-level
  `design_height/reference_resolution/safe_area_1080p/command_card/control_groups`,
  `assets[].name`, `insets{l,t,r,b}`, `font_files`.
- **v2** (`format_version: 2`, the design delivery): `theme`, `assets[].id`,
  `kind`, `alpha`, `color_space`, `scale`, `nine_slice{left,top,right,bottom,center}`,
  `three_slice`, `states`, `atlases` pages with `padding`/`gutter`, `fonts`
  with roles/glyphs/bundled.
- A manifest with no declared version but the v2 asset shape (`id`/`path`/
  `width`/`height`) is parsed as v2 and reported as
  `WARN format-version-missing`; a **declared** unsupported version is a hard
  `HUD_PACK manifest_version=<found> supported=1,2` error.
- Donor aliases are accepted and canonicalised: `path` -> `file`,
  `width`/`height` -> `w`/`h`, `format`, `gutter`, and top-level
  `name/version/style/license/notes` metadata.

`HudPack::load` order is format -> strict validation -> hashes -> textures. A
bad pack cannot be half-loaded and cannot silently draw different art.

## 2. Strict validator (and the `menu_part_quads` clamp)

Implemented rules (critical unless noted):

| rule | check |
|---|---|
| `size-zero` / `size-limit` | `w>0`, `h>0`, `<= 8192` |
| `duplicate-id` | unique asset ids |
| `insets-finite/negative` | finite, `>= 0` |
| `insets-cross-centre` | `l+r < w` and `t+b < h`; names the element |
| `nine-slice-center` | `stretch` / `tile` / `vertical` only |
| `three-slice-axis/caps` | vertical only; `top>0`, `bottom>0`, `top+bottom<h` |
| `mips-atlas-bleed` | a mipmapped asset cannot share an atlas page |
| `mips-nine-slice` | straight-alpha nine-slices are not mipmapped |
| `atlas-missing/entry-missing` | an asset on a page must have a page and an entry |
| `atlas-entry-size` | entry `w×h` equals the declared source size |
| `atlas-uv-outside` | `x+w <= page.w`, `y+h <= page.h` |
| `atlas-bleed-padding` | entry at least `padding` from the page border |
| `atlas-entry-overlap` | entries at least `gutter` apart on one axis |
| `atlas-duplicate-id/entry` | unique page ids and entry ids per page |
| `colorspace` | `sRGB`/`linear`/`rgba8` |
| `font-*` | unique ids, bundled file present, sha256 shape, roles/glyphs warnings |

v1 keeps its documented 11 unsliced rows as **warnings** (single-image types,
never sliced); v2 gets no tolerance. The validator's first error becomes
`PackError::Invalid { element, rule, detail }` and the load is refused.

The current `mobile_hud.rs::menu_part_quads` clamps insets to 45 % of the target
(`.min(rect.w * 0.45)`), which hides a bad manifest and squashes the ornament.
Patch (not applied — `mobile_hud.rs` is ledger 553's): `analysis/agent-hud/patches/ledger-555-menu-part-strict.patch`.
It removes the clamp, refuses a crossing inset set with
`HUD_PACK invalid element=menu_part rule=insets-cross-centre`, and falls back to
one stretched quad when the *target* is smaller than the authored corners (a
target-size decision, not an inset rewrite).

## 3. Nimrod coverage

Command:
`./target/debug/wc3-hud-pack fonts --font analysis/war3_full3/Fonts/NIM_____.ttf --out analysis/agent-hud/NIMROD-COVERAGE.md`
rc=0.

| block | required | covered | status |
|---|---:|---:|---|
| Basic Latin | 95 | 95 | PASS |
| Latin-1 Supplement | 96 | 96 | PASS |
| Latin Extended-A | 128 | 128 | PASS |
| Combining Diacritics | 112 | 0 | FAIL (reported) |
| Cyrillic (Russian, 66) | 66 | 66 | PASS |
| Cyrillic (full U+0400–04FF) | 256 | 94 | PARTIAL |
| Cyrillic Supplement | 48 | 0 | FAIL (reported) |
| UI specials | 43 | 42 | PARTIAL — only `₽` missing |
| Digits | 10 | 10 | PASS, tabular |

- `HUD_FONT_DIGITS tabular=true advances=["0:359.42" … "9:359.42"]` — resource
  and HP/mana numbers cannot shift the layout.
- Metrics: units/em 2048, ascent 2333, descent -516, line gap 0, 30 px line box
  41.73 px, space advance 179.71/1000. Line break contract: word wrap; space,
  hyphen and underscore are covered.
- Combining diacritics are absent from Nimrod; precomposed Latin-1/Extended-A
  are fully covered. The runtime chain (`Nimrod -> pack serif -> pack sans ->
  system DejaVu`) covers any codepoint Nimrod lacks; the coverage table is the
  evidence, not a claim that Nimrod itself has them.
- The gate fails on Basic Latin/Latin-1/Latin-A/Russian Cyrillic/digits or
  non-tabular digits; diacritics are reported, not fatal (WC3 strings do not
  use combining marks, and the fallback faces carry them).

Full output: `analysis/agent-hud/FONT-NIMROD-555.txt`,
`analysis/agent-hud/NIMROD-COVERAGE.md`.

## 4. Packs

### Main v2 pack (`assets/hud`)

`./target/debug/wc3-hud-pack build --src assets/hud --out assets/hud/pack`
(rc=0):

```
HUD_PACK_BUILD assets=21 atlases=1 entries=21 page=1024x1024 padding=2 bleed=1 bytes_in=198550 bytes_out=1823222 errors=0
HUD_PACK_BUILD_PROVENANCE assets=21 page=1024x1024 padding=2 bleed=1
```

`validate assets/hud/pack` (rc=0):

```
HUD_PACK_VALIDATE format=2 theme=classic_fantasy assets=21 atlases=1 atlas_assets=21 fonts=4 states=4 sliced=12 standalone=0 errors=0 warnings=0
HUD_PACK_TEXTURES root=assets/hud/pack loaded=5 errors=0
```

The manifest is v2, every asset has `atlas: "ui"` + `atlas_rect`, an atlas page
entry and the original provenance; the atlas carries a 1 px edge extrusion and
the manifest records `padding`/`gutter`. The three authored faces are bundled
(`fonts/`), Nimrod stays external (`bundled:false`).

### Orc v2 pack (`assets/hud/orc`)

Built with the same tool and the final pack's fonts:

```
HUD_PACK_BUILD assets=21 atlases=1 entries=21 page=2048x2048 padding=2 bleed=1 bytes_in=2009938 bytes_out=5539201 errors=0
HUD_PACK_VALIDATE format=2 theme=orc assets=21 atlases=1 atlas_assets=21 fonts=4 states=4 sliced=3 standalone=0 errors=0 warnings=0
HUD_PACK_TEXTURES root=assets/hud/orc/pack loaded=5 errors=0
```

`sliced=3` are the vertical three-slices (`frame.portrait_orc`, `banner.ornate`,
`panel.wide`); every other orc element is deliberately fixed-size.

Logs: `analysis/agent-hud/PACK-555-BUILD.txt`,
`analysis/agent-hud/ORC-555-BUILD.txt`; per-asset provenance lines are in each
pack's `BUILD.log`.

## 5. Renderer budget and debug overlay

Renderer consequences of the pack (hud.md §10):

- A v2 pack is **one atlas texture** per page. `UiBatch::textured_quad` only
  starts a new draw when the texture changes, and preserves painter order, so a
  pack skin's continuous element run collapses into one draw instead of one per
  PNG. The commandbuttons sheet stays a second texture only when the pack has
  one.
- PNG decode and file I/O happen only in `HudPack::load` (startup) and in the
  pack builder; the draw path samples already-registered textures. No PNG is
  decoded and no file is opened during `Game::scene`.
- Layout is solved in `Game::rebuild_layout` (timer A); draw commands are built
  in `Game::scene` (timer B). `HudFrameTrace` reports both, plus draws,
  vertices and textures, so a regression is attributable instead of guessed.
- The atlas builder keeps a 1 px edge extrusion and manifest `padding`/`gutter`;
  this is the anti-bleed contract the renderer's linear sampler relies on.

`hud/debug.rs`:

- `WC3RUST_HUD_DEBUG` selector: `0`/unset off, `1`/`all`, or a comma list of
  `bounds,hit,clip,safe,z,ids,breakpoint,theme,state,diagnostics,budget`.
  Default off; `HudDebug::end` returns before touching the batch when off.
- `HudFrameTrace::from_batch` counts draws (white/font/minimap/textured),
  vertices per channel, distinct textures, and takes `layout_micros` /
  `prepare_micros` from two timers (`begin_layout`/`end_layout`,
  `begin`/`end`). `HUD_FRAME draws=… vertices=… textures=… layout_us=…
  prepare_us=… budget=PASS|OVER:…` is logged on a bounded cadence (60 frames).
- Default ceilings: 96 draws, 120 000 vertices, 24 textures, 2 ms layout,
  4 ms preparation. They are ceilings; the measurement is what matters.
- `collect_layout(&MobileHudLayout)` is the single geometry source: console,
  safe frame, selection/portrait/card/groups/army/tooltip bounds, every card
  slot, group slot, resource pill, tab, inventory touch rect, drawer and
  tooltip anchor. The same rects feed hit-testing, so the overlay cannot drift.
- Layers draw outlines/fills, ID labels, z-coloured outlines, safe/clip frames,
  and a diagnostics panel with breakpoint, viewport px/dp, density, theme,
  state, budget and layer list.

Headless proof (same `draw_debug` + `soft_raster` as the device path):

```
HUD_DEBUG_OVERLAY viewport=device-2414x1080 size=2414x1080 rects=7 layers=bounds,hit,clip,safe,z,ids,breakpoint,theme,state,diagnostics,budget
```

Artifact: `analysis/agent-hud/hud-debug-overlay.png` (all layers on; the
diagnostics panel and hit/clip/safe rects are visible). Log:
`analysis/agent-hud/HUD-DEBUG-555.txt`.

Production integration patch (not applied — `game.rs` is the orchestrator's):
`analysis/agent-hud/patches/ledger-555-hud-debug-game.patch`. It adds the
`hud_debug` field (`from_env()`), times `rebuild_layout`, times the
draw-command build in `scene`, and calls `end` once per frame. Release code is
unchanged when the env var is unset.

## 6. Donor `hud-pack-v2` (schema accepted, art rejected)

Full file-to-file matrix: `analysis/agent-hud/DONOR-555-MATRIX.md`.

- **Accepted/adapted**: manifest schema (`id/path/width/height/format/states/
  nine_slice`) -> parser aliases; atlas index `padding=4`/`gutter=2` -> separate
  `padding` and `gutter` fields; integration contract (deterministic raster,
  alpha/sRGB, panels-only nine-slice, state by semantics, one draw/hit
  geometry) -> `pack_build` + `hud::debug`; ID taxonomy -> naming reference.
- **Rejected as art**: all 91 SVGs. Evidence: 37/40 panel/menu/frame/widget
  files use the same `<rect rx=...>` silhouette and 40/40 start with the
  identical metal/gold/wood gradient defs; 51/51 icons are a circle ring plus
  one primitive. Nothing was copied into `assets/hud/**`.
- **Rasterizer**: `rsvg-convert` 2.63.4. One SVG (topbar 960×94) rendered twice
  -> identical `13107ff5…6fa53`. Whole donor: 91/91 determinism. Log:
  `analysis/agent-hud/DONOR-555-RASTER.txt`.
- **Capacity test**: the rasterized donor was packed by the same builder
  (`assets=91 atlases=1`, page 4096²), then the device loader validated and
  decoded it: `assets=91 atlases=1 atlas_assets=91 errors=0 warnings=0`, 1
  texture.
- **Validator test**: the donor manifest (no version, v2 shape) gives
  `assets=91 ... sliced=23 ... errors=0 warnings=1` (warning is the missing
  declared version). A copy with `hud.topbar` insets `600/18/600/18` on 960×94
  is refused: `ERROR element=hud.topbar rule=insets-cross-centre ...`. Logs:
  `analysis/agent-hud/DONOR-555-VALIDATE.txt`,
  `analysis/agent-hud/DONOR-555-VALIDATE-BAD.txt`.

## 7. Orc sheet

Full report: `analysis/agent-hud/ORC-SHEET-555.md`. Source
`9a97e00c-…png` (1536×1024), sha256 `eb1278ce…530f5`.

- 12 measured elements -> 21 assets: 10 fixed panels/bars/tabs/dialog, 3
  vertical three-slices, 5 round icons + 5 procedural disabled variants.
- Matte: degree-2 per-channel background fit (robust-trimmed, RMSE 0.03151),
  colour/saturation/darkness score, 3×3 close, enclosed-hole fill, 3 px edge
  decontamination; neutral near-background edge pixels are shaved rather than
  shipped as a halo.
- Measurements: every asset `gray_edge=0.000 %`, `soft_interior=0`, alpha holes
  0; outputs open as RGBA.
- Determinism: `runs=2 files=22 identical=22` (two complete segmentation passes,
  byte-identical SHA-256).
- Insets: `frame.portrait_orc` top 125/bottom 72; `banner.ornate` top 95/bottom
  70; `panel.wide` top 75/bottom 48 — all vertical-only, width fixed, caps hold
  the gems/cloth/trees. Everything else is explicitly fixed-size with the
  reason in the manifest and report (baked slots/rows/buttons, centred gems in
  the stretch band, internal dividers).
- Manifest v2 `theme: orc` with per-asset provenance (sheet, sheet sha, crop
  rect, transform, QC, inset reason, output sha256). Pack built and validated
  above.
- Note: the brief said 4 and 3 tab cells; the art has **6** and **5**. The files
  are named `tabs.strip6` / `tabs.strip5`; the discrepancy is recorded instead
  of mislabelling the cells.

## Evidence (observed)

| check | command | result |
|---|---|---|
| library build | `scripts/h2-cargo check -p wc3-game --lib` | rc=0 (38 warnings, 0 errors); `analysis/agent-hud/CHECK-555.txt` |
| main pack | `wc3-hud-pack build --src assets/hud --out assets/hud/pack` | rc=0; 21 assets, 1 atlas, 0 errors |
| main validation | `wc3-hud-pack validate assets/hud/pack` | rc=0; 21 assets, 0 errors, 5 textures |
| orc pack | `wc3-hud-pack build --src assets/hud/orc --out assets/hud/orc/pack --fonts …` | rc=0; 21 assets, 1 atlas, 0 errors |
| orc validation | `wc3-hud-pack validate assets/hud/orc/pack` | rc=0; 21 assets, 0 errors, 5 textures |
| Nimrod | `wc3-hud-pack fonts --font …/NIM_____.ttf` | rc=0; Russian 66/66, Latin 95/95, digits tabular |
| donor validation | `wc3-hud-pack validate …/hudpack-v2` | rc=0; 91 assets, 0 errors, 1 warning |
| donor negative | same with broken topbar insets | rc=1; `element=hud.topbar rule=insets-cross-centre` |
| donor raster | `wc3-hud-pack rasterize --manifest … --out …` | 91/91 deterministic, 0 mismatched |
| overlay proof | `wc3-hud-pack overlay` | rc=0; PNG with all layers |

No test suite was added or run (freeze); no device/APK step was performed.

## Patches (owned elsewhere; in `analysis/agent-hud/patches/`)

- `ledger-555-hud-debug-game.patch` — `game.rs`: field, init, layout timer,
  draw-command timer, one `end` call per frame.
- `ledger-555-menu-part-strict.patch` — `mobile_hud.rs`: remove the 45 % inset
  clamp; refuse crossing insets; stretch only when the target is too small.
- `ledger-555-assets-pack-strict-fallback.patch` — `assets.rs`: when a pack
  directory exists but `HudPack::load` fails, log the `HUD_PACK invalid …`
  error and skip the legacy-atlas substitution instead of quietly rendering
  other art.

## Risks / next

- The game still selects the **v1 final pack** for the HUD skin; the new v2
  packs load and validate, but wiring `assets/hud` into the menu/HUD draw path
  is the orchestrator's `game.rs` work (the patch shows the debug call site).
- `Skin::from_pack` is the 24-role v1 contract; a component v2 pack
  legitimately has no such roles. The loader reports `role_missing` and
  `HUD_PACK fail=role-mapping`; it does not pretend the pack is a full skin.
- Nimrod has no combining diacritics and only the Russian subset of Cyrillic;
  the fallback chain covers other codepoints. Do not render combining marks
  through Nimrod alone.
- The orc sheet's interiors are opaque (hole-filled). A transparent cut-out
  (portrait/command slots) needs a derived mask, intentionally not invented
  here.
- The builder rejects atlas pages with `mips:true`; if a future backend needs
  mipmapped pages, the padding rule must grow to at least the mip bleed and the
  validator must be extended together.

## H2

- Corpus preflight: `ev-6fb71c210387cb0d` (task_type=debug, weak).
- Capture: `1098` (`ledger-555-hud-pipeline`).
- Experiences: `4799` (SVG raster determinism), `4800` (manifest v1/v2 detection
  + donor aliases), `4801` (contact-sheet matte), `4802` (centred gem vs
  nine-slice -> vertical three-slice).
- Monitor: `555` closed `completed` with this report and the artifact paths.
- Evidence files (absolute): `/data/data/com.termux/files/home/src/wc3rust/analysis/agent-hud/`.
- Patches: `patches/ledger-555-hud-debug-game.patch`,
  `patches/ledger-555-menu-part-strict.patch`,
  `patches/ledger-555-assets-pack-strict-fallback.patch`.
