# Mechanic runner integration — ui.fdf.frames / ui.fonts / ui.text / ui.layout

Dispatcher-owned files (not in this layer's write scope). Two edits are needed so the four
mechanic tests count as coverage per `analysis/tech-lead/TEST_POLICY.md`.

## 1. `analysis/tech-lead/mechanics_gap_map.json` — add four rows

```json
{"id":"ui.fdf.frames","mechanic":"Every UI screen parses shipped FDF definitions and renders its frames with required controls","status":"ported","source":"analysis/war3_data/UI/FrameDef/** + UI/war3skins.txt","implementation":"crates/wc3-game/src/fdf_ui.rs `UiScreen`/`coverage`/`render`","test":"crates/wc3-game --test ui_fdf ui_fdf_frames_six_screens_from_shipped_definitions","note":"6 screens, 220 frames, 12 required controls"},
{"id":"ui.fonts","mechanic":"Shipped fonts load; Latin+Cyrillic coverage; no missing glyphs in UI strings","status":"ported","source":"analysis/war3_full3/Fonts/FRIZQT__.TTF + StringList","implementation":"crates/wc3-game/src/fdf_ui.rs `glyph_coverage`/`ui_strings`","test":"crates/wc3-game --test ui_fdf ui_fonts_shipped_frizqt_covers_latin_and_cyrillic","note":"1782 strings, 51122 chars, latin 95/95, cyrillic 94/256, missing 0"},
{"id":"ui.text","mechanic":"All UI labels resolve from shipped StringList/WTS; unresolved keys reported","status":"ported","source":"UI/FrameDef/*Strings.fdf + UI/war3skins.txt","implementation":"crates/wc3-game/src/fdf_ui.rs `string_coverage`","test":"crates/wc3-game --test ui_fdf ui_text_labels_resolve_from_shipped_stringlist","note":"34 labels, 33 resolved keys, 1 runtime literal, 0 unresolved"},
{"id":"ui.layout","mechanic":"FDF anchors/9-slice applied at 720p/1080p/1440p without overflow","status":"ported","source":"FDF anchors + Backdrop art","implementation":"crates/wc3-game/src/fdf_ui.rs `overflow`/`nine_slice`","test":"crates/wc3-game --test ui_fdf ui_layout_anchors_three_resolutions_without_overflow","note":"overflow 0 at 3 resolutions; nine-slice area exact"}
```

## 2. `analysis/tech-lead/run_mechanic_tests.sh` — add after the `ui.command-card` line

```sh
run ui.fdf.frames                wc3-game --test ui_fdf ui_fdf_frames_six_screens_from_shipped_definitions || status=1
run ui.fonts                     wc3-game --test ui_fdf ui_fonts_shipped_frizqt_covers_latin_and_cyrillic  || status=1
run ui.text                      wc3-game --test ui_fdf ui_text_labels_resolve_from_shipped_stringlist      || status=1
run ui.layout                    wc3-game --test ui_fdf ui_layout_anchors_three_resolutions_without_overflow || status=1
```

Verified command forms (rc=0 observed 2026-11-14):

```
cargo test -p wc3-game --test ui_fdf ui_fdf_frames_six_screens_from_shipped_definitions
cargo test -p wc3-game --test ui_fdf ui_fonts_shipped_frizqt_covers_latin_and_cyrillic
cargo test -p wc3-game --test ui_fdf ui_text_labels_resolve_from_shipped_stringlist
cargo test -p wc3-game --test ui_fdf ui_layout_anchors_three_resolutions_without_overflow
```
