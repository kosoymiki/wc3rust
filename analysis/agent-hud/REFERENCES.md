# HUD reference ledger — 2026-11-12

Method: `stealth-browser-mcp` (headless Chromium 157, Termux aarch64, `--no-sandbox --disable-dev-shm-usage`), access date **2026-11-12**. Screenshots were saved with `take_screenshot` into the MCP data dir and copied to `analysis/agent-hud/references/`; auto-frames are 480x270 YouTube stills, so 1080p figures below are scaled estimates (labeled **inferred**). No CAPTCHA/auth/paywall was bypassed; interfaceingame.com returned a Cloudflare "verify" wall, so it was abandoned as a source.

Artifacts:

- `references/reference_sheet.png` — six labelled reference tiles.
- `references/comparison_sheet.png` — rejected device shot (annotated) vs three measured references.
- `references/measurements.json` — machine-readable measurements and source URLs.
- `references/frame_hok_0_crop.png`, `frame_hok_1_crop.png` — Honor of Kings in-game HUD.
- `references/frame_aow3_2_crop.png` — Art of War 3 RTS HUD.
- `references/rumble_battle3_crop.png` — Warcraft Rumble PvP HUD.
- `references/diablo_hud1_crop.png`, `diablo_hud3_crop.png` — Diablo Immortal dense HUD.
- `analysis/user-shot-4.png` — the rejected 1200x492 capture of our current HUD.

## Source table

| # | Game / source | URL | What was captured | Status |
|---|---|---|---|---|
| 1 | Warcraft Rumble (primary WC3 mobile reference) | https://apkpure.com/warcraft-rumble/com.blizzard.arc | Store screenshots (collection, upgrade, PvP battle) | observed |
| 2 | Warcraft Rumble gameplay | https://www.youtube.com/watch?v=LKs1XAEuQGI (frame `hq2.jpg`) | Battle HUD with bottom card bar | observed |
| 3 | Warcraft Rumble gameplay | https://www.youtube.com/watch?v=tVFmfKJ6SWU | Victory/collection UI, card treatment | observed |
| 4 | Honor of Kings | https://www.youtube.com/watch?v=EeYQtAfxW8k (frame `hq2.jpg`) | In-game MOBA HUD | observed |
| 5 | Honor of Kings | https://www.youtube.com/watch?v=UTdjjMInB5Y | Teamfight / minimap / skill wheel | observed |
| 6 | Honor of Kings | https://www.youtube.com/watch?v=jvaPQoEGYv4 | Lane HUD | observed |
| 7 | Art of War 3 | https://apkpure.com/art-of-war-3-rts-strategy-game/com.geargames.aow | Store screenshots | observed |
| 8 | Art of War 3 | https://www.youtube.com/watch?v=8lhEKQcETFk (frame `hq2.jpg`) | RTS battlefield, minimap, resources | observed |
| 9 | Art of War 3 | https://www.youtube.com/watch?v=busRFgpzJwA | Faction/UI art | observed |
| 10 | Diablo Immortal | https://www.youtube.com/watch?v=SgKkTdFcNak (frames `hq1/hq3.jpg`) | Dense in-game HUD, skill wheel | observed |
| 11 | Diablo Immortal | https://www.youtube.com/watch?v=NTM_mzkdy8s | Quest tracker, dark panels | observed |
| 12 | Mobile MOBA HUD case study | https://www.brianbaldueza.com/speculative-moba-design | "An HCI case study ... a mobile HUD designed to lower the learning curve without flattening the skill ceiling." | observed (text) |
| 13 | Mobile HUD redesign (industry write-up) | https://dovion.io/en/blog/mobile-hud-redesign | "where do a player's thumbs naturally land"; "the minimap relocated: it's moved up into a corner where it no longer hides the action"; "Portrait and landscape handled separately" | observed (text) |
| 14 | Android display cutouts | https://developer.android.com/develop/ui/views/layout/display-cutout | "A display cutout is an area ... that extends into the display surface"; "Lay out your app within windows insets" | observed (text) |
| 15 | Android accessibility | https://developer.android.com/guide/topics/ui/accessibility/apps | "each interactive UI element have a focusable area, or touch target size, of at least 48dpx48dp. Larger is even better." | observed (text) |
| 16 | Unity `Screen.safeArea` | https://docs.unity.com/en-us/engine/6000.6/script-reference/unityengine/screen/safearea | "Avoid placing user interface elements in areas outside the safe area rectangle"; safe area differs in portrait and landscape; Android 15+ always renders into the cutout | observed (text) |

## Per-reference observations

### Honor of Kings (MOBA, landscape)

Measured on a 480x270 auto-frame (`scale_to_1080p = 4.0`, **inferred** 1080p values):

- Minimap: 70x70 frame px → **280x280 @1080p**, top-left, 25.9% of frame height, with a thin bright border and dark backdrop.
- Top HUD: score/KDA centered in a narrow strip; team portraits immediately right of the minimap; settings and gold at the far right. Top strip is ~14% of height.
- Bottom HUD: ~27% of height. Joystick radius ~28 frame px → **~112 px @1080p**; skill buttons 22-30 frame px diameter → **88-120 px @1080p**; consumable slots in the bottom-centre.
- Edges are used; the centre of the screen is never covered. Buttons are circular with a dark ring, saturated icon, and a bright rim on the active/level-up state.
- Adopt: corner minimap, edge-hugging controls, circular primary actions, large attack button, high-saturation icons on dark circular chrome. Reject for RTS: 5+ skill buttons and a fixed 2-thumb MOBA layout; our game needs a 4-slot primary ring plus a 4-slot context tray.

### Warcraft Rumble (primary WC3 mobile reference, portrait)

- Battle HUD occupies the bottom ~18%: a wide gold/level strip with chunky card portraits (34x44 frame px → **~184 px wide @1080p**), saturated painted icons, heavy rounded-square frames, and a strong dark outline.
- Collect/upgrade screens use large rounded panels, gold trim, cream/white display type with a 2-3 px dark outline; secondary values are gold on dark.
- The battlefield itself is unobstructed; controls and cards live at the bottom edge, exactly the hierarchy our 3D view needs.
- Adopt: chunky card frames, gold/cream hierarchy, thick dark outlines, portrait icon at card size, bottom-edge command cluster. The 2026-11 Rumble visual language is the closest first-party translation of WC3 into touch UI.

### Art of War 3 (mobile RTS, landscape)

- Minimap 46x46 frame px → **184x184 @1080p**, top-left, adjacent resource/command strip; timer top-right.
- Resource bar is a thin, high-contrast strip (~12 frame px → **48 px @1080p**); battlefield is not covered.
- RTS grammar matches ours: minimap + resources top-left, orders from the bottom-right in other frames; unit cards use 3D renders, not flat symbols.
- Adopt: top-left information stack, small persistent resource strip, minimap card with a readable frame; avoid covering the battlefield with a large opaque console.

### Diablo Immortal (dense MMORPG HUD, landscape)

- Portrait/name top-left, quest and menu top-right, joystick bottom-left, skill wheel bottom-right, ~22% bottom HUD.
- Skill buttons 18-24 frame px → **59-79 px @1080p**; dark translucent chrome, saturated ability icons, thin gold/red accents.
- Density is higher than ours, but the edge discipline and dark translucent panels are directly reusable.
- Adopt: dark glass + warm accent, icon-first buttons, edge-only HUD, small persistent top-left identity card. Reject: 8+ tiny ability buttons; our touch targets must stay ≥104 px.

### Current rejected shot (analysis/user-shot-4.png, 1200x492)

Observed failures (pixel coordinates in the 1200x492 capture):

- System time/status icons at y≈8-30 overlap the resource pills starting at y≈14. Native scale is 2414/1200 ≈ 2.01; the required top inset is at least **72 px @1080p** (80 px chosen).
- ARMY/BASE/ZOOM chips at top-right sit under system status icons; their labels are ~10 px.
- Context buttons start at x≈990 and run to x≈1190 (right edge); the last label is clipped. Primary buttons are ~57 px in this capture (~115 px native) but labels are ~10 px and the buttons are scattered rather than a clean ring.
- Joystick at bottom-left has low contrast on grass; the knob is a pale blob without a rim or direction ticks.
- Selection card is legible but uses a small portrait and no armor/damage/ability slots.

## Safe-area policy evidence

- Android: cutouts extend into the display and apps must lay out inside window insets; Android 15+ forces edge-to-edge rendering into the cutout area on some stacks (Unity doc). Therefore the HUD must never assume `(0,0)` is usable.
- Android touch targets: at least 48x48 dp, "larger is even better".
- Industry mobile HUD write-up: thumbs land at the bottom corners; the minimap should move to a corner where it does not hide the action; portrait and landscape need separate layouts.
- Design consequence: layout takes an explicit `SafeArea` in pixels; defaults for the mock are 80/48/48/48 at 1080p landscape and 96/64/24/24 portrait; all top text is placed below `safe.top`, all controls above `safe.bottom`, and no interactive rect may cross the safe rectangle or another rect.

## Limitations / confidence

- YouTube auto-frames are 480x270 (or 400x330 for portrait crops); pixel measurements are ±8%, so all 1080p values are **inferred**, not device-native. Layout proportions and relative hierarchy are **observed**.
- Store screenshots are marketing compositions; they were used for chrome/typography/icon treatment, not for exact HUD geometry.
- The source devices' densities and safe insets are unknown; the design therefore uses relative proportions plus explicit Android inset input, not copied absolute dp values.
- interfaceingame.com was blocked by a Cloudflare verification wall; no attempt was made to bypass it.
