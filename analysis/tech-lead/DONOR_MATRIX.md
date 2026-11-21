# DONOR MATRIX — byte-level intake (2026-11-14)

Source of counts: `DONOR_INVENTORY.json` (filesystem walk: 21 donors, 23,656 files,
734,143,631 bytes). This table is the intake work list: for every donor the
unique capability, the receiving module, and the verdict. "PORT" means the
strongest implementation is merged into our crate with tests; "ADAPT" means the
mechanism is reimplemented for our data model; "REJECT" is recorded with reason.

| Donor | Unique capability (evidence) | Receiving module | Verdict |
|---|---|---|---|
| **StormLib** (408 f, 4.65 MB) | Reference MPQ reader: 234 C files, all compression codecs, encrypted sectors, listfile-less archives (`src/storm/`) | `wc3-mpq` (robustness: encryption, codecs, `(listfile)` fallback) | PORT |
| **CascLib** (154 f, 120 MB) | CASC/CDN storage, MPQ legacy layer, storage handle model | `wc3-mpq` (only if Reforged CASC support is needed; our install is MPQ) | ADAPT/REJECT for CASC |
| **War3Net** (1,911 f, 179 MB) | Canonical binary serializers for **every** map format + **99 real maps** (54 `.w3m`, 45 `.w3x`) + JASS parser/compiler + object-data merge | `wc3-formats` (field-order authority), `wc3-re` (JASS), test corpus for melee maps | PORT (formats) + USE AS MAP CORPUS |
| **HiveWE** (523 f, 18.5 MB) | Editor: `PathingMap::Flags` semantics (0x02/0x40/0x80 — already ported), terrain/cliff/water rendering, doodad placement, 61 `.mdl` samples | `wc3-formats` (pathing), `wc3-game` terrain/pathing | PORT semantics; terrain rendering parity |
| **WarsmashModEngine** + **-Android** (3,724 f, 27 MB) | Java WC3 engine: simulation (units/abilities/orders/AI), render (GLSL 30 files), audio, UI; Android port layer | `wc3-sim` (combat/orders/AI), `wc3-render` (particles/shaders), `wc3-game` (UI flow) | PORT gameplay + render mechanisms |
| **W3AndroidEngine** (3,184 f, 22 MB) | Android Java engine: 3,038 Java files, JASS runtime, GLES renderer, input | `apps/android-native` (lifecycle/GLES), `wc3-re` (JASS native coverage) | ADAPT |
| **OpenRA** (4,192 f, 100 MB) | Mature RTS: order queue, pathfinding (A* + hierarchical), AI (squad/priority), fog, replay, mod data | `wc3-sim` (AI squads, order queue), `wc3-path` (hierarchical pathfinding) | ADAPT strongest mechanisms |
| **mdx-m3-viewer** (584 f, 7.8 MB) | MDX/M3 parser + WebGL renderer: geosets, materials, particles, ribbons, event objects | `wc3-formats::mdx`, `wc3-render` (particles/ribbons) | PORT parser edge cases + FX |
| **war3-model** (113 f, 1 MB) | MDX/MDL model pipeline (TypeScript), animations, attachments | `wc3-anim` (attachment/event objects) | ADAPT |
| **XenosRecomp** (47 f, 176 KB) | Shader model 3 → SPIR-V recompiler (Xbox 360) | not needed for our asset path | REJECT (no SHM3 shaders) |
| **kira** (190 f, 9.4 MB) | Rust audio engine: mixer, spatialisation, effects | `wc3-audio` (spatialisation quality, sound groups) | ADAPT |
| **parry** (530 f, 4.5 MB) | Rust collision/geometry math | `wc3-sim` (unit collision queries), `wc3-path` | ADAPT |
| **wgpu** (2,739 f, 53.6 MB) | Renderer reference + 466 WGSL examples | `wc3-render` (pipeline patterns, MSAA, mip generation) | REFERENCE |
| **sdl3-sys-rs** (1,522 f) | SDL3 bindings + 489 headers | Android app layer (already SDL3) | REFERENCE |
| **SDL** (2,415 f, 71.8 MB) | SDL sources, Android Java glue | `apps/android` (activity/input hints) | REFERENCE |
| **android-activity** (158 f, 2.8 MB) | Rust Android native-activity glue | `apps/android-native` lifecycle | ADAPT |
| **libadrenotools** (109 f) | Adreno driver replacement hooks | device GPU only; our APK uses system driver | REJECT (no custom driver) |
| **libxemit** (51 f, 14 MB) | Qualcomm shader compiler bridge | not needed | REJECT |
| **WarsmashWindowsWrapper** (40 f) | Windows launcher wrapper | not needed (Android/Linux) | REJECT |

## Intake groups (agents)

1. **Formats/MPQ**: StormLib + CascLib + War3Net — robustness + field-order authority + 99-map corpus.
2. **Gameplay**: WarsmashModEngine(-Android) + W3AndroidEngine + OpenRA + War3Net (sim) — combat, orders, AI, abilities.
3. **Render/models**: mdx-m3-viewer + war3-model + wgpu + XenosRecomp — MDX render parity, particles/ribbons, shader patterns.
4. **Terrain/editor**: HiveWE + kira — terrain/cliff/water rendering, pathing, audio spatialisation.
5. **Android/input**: android-activity + sdl3-sys-rs + SDL + parry + libadrenotools/libxemit — lifecycle, input, collision math, solved GPU constraints.
6. **Original game extraction**: War3Net 99 maps + the install's 44 campaign maps — use the real Blizzard melee maps `(2)Justice`, `(4)Deadlock`, etc. as the 1v1 corpus; extract assets (models/icons/sounds/UI) for the runtime.

Each intake must produce: file-to-file matrix (donor path → our path → verdict with `file:line`),
the port/adapt of the strongest mechanisms, tests per language/module, verification, and
persistence (capture/experience/monitor/brain). No partial intake, no un-tested merge.
