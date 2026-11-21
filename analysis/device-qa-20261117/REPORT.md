# Device QA — build `0.1-p0` installed 2026-11-17 11:20 (artifact built 11:18)

**Owner:** device-qa (ledger 482, task_ref `wc3rust-deviceqa-20261117`)
**Scope:** read-only on source; writes only under `analysis/device-qa-20261117/`.
**Result:** **BLOCKED — the installed build native-aborts in its first draw, before any frame is
presented.** No menu / in-game / world frame exists to measure on this APK. The HUD *layout telemetry*
was captured at 2414x1080 and matches the headless contract vertically within 0.5 px; pixel rules
(toast/tooltip text, gold line, inventory plates, anisotropy) are **not measurable** on this build.

---

## 1. Build identity (observed)

| item | value |
|---|---|
| package / activity | `org.herofolends.p0` / `org.libsdl.app.SDLActivity` |
| version | `versionName=0.1-p0`, `versionCode=1` |
| installed APK | `/data/app/~~iFU4tA-EJjd0mQN--qv_xQ==/org.herofolends.p0-rtKNSOmquDZ4NxVt9H4AnA==/base.apk` |
| APK size / mtime | 7 374 394 B / 2026-11-17 11:20:23 |
| `lastUpdateTime` | 2026-11-17 11:20:25 |
| APK sha256 | `ea1f42bc5ba6d9d44e4e2fb9de56ee4c927bb42f305fac874b558e94ea2b3502` |
| repo artifact | `build/android-p0/herofolends-p0.apk`, 7 374 394 B, mtime 11:18:06, **same sha256** |
| display | `wm size` → Physical 1240x2772, Override 1080x2414; game surface 2414x1080 landscape |
| root | `su -c 'id'` → `uid=0(root) gid=0(root) groups=0(root) context=u:r:ksu:s0` |

The APK under test is byte-identical to the 11:18 build artifact named in the packet.

## 2. What happens on launch (observed)

Every start/resume reaches the HUD layout code and dies in the first draw:

```
11-17 11:39:09.607 I wc3rust: GAME_READY source=user-mpq map="Justice" ...
11-17 11:39:09.614 I wc3rust: GAME_HUD_RECTS scale=0.73 ...
11-17 11:39:09.727 I wc3rust: GAME_HUD_RECTS scale=1.10 ...
11-17 11:39:12.378 I wc3rust: GAME_HUD_RECTS scale=1.10 ...
11-17 11:39:13.120 I am_crash: [3239,0,org.herofolends.p0,816332358,Native crash,Aborted,unknown,0,0]
```

Tombstone abort message (identical in tombstones 24, 25, 26):

```
signal 6 (SIGABRT), code -1 (SI_QUEUE)
Abort message: 'wgpu error: Validation Error

Caused by:
  In a CommandEncoder, label = 'game-encoder'
    In a draw command, kind: Draw
      In bind group index 0, the buffer bound at binding index 0 is bound with size 112 where the shader expects 176.'
```

Backtrace: `panic` → `wgpu_core::CoreCommandEncoder::finish` → `wc3_game::render::Renderer::render_with`
(tombstone_26 frame #14) → `SDL_main` → JNI.

### Evidence that no frame was ever presented

| log | `GAME_FIRST_FRAME` | `GAME_STATE` | `GAME_RENDER_SCALE` | `GAME_HUD_RECTS` |
|---|---|---|---|---|
| `logs/launch-01.log` (attempt 1) | 0 | 0 | 0 | 4 |
| `logs/launch-02.log` (attempt 2) | 0 | 0 | 0 | 0 |
| `logs/attempt-03-game.log` (attempt 3) | 0 | 0 | 0 | 4 |
| current log buffer | 0 | 0 | — | — |

`GAME_FIRST_FRAME` is logged *after* `queue.present()` (`apps/android-native/src/lib.rs:1724`), and the
abort happens in the first `CommandEncoder::finish()` (`crates/wc3-game/src/render.rs`), so the present
path never completes: **there is no menu frame and no in-game frame on this build.**

## 3. Captures obtained (paths, sizes, provenance)

| file | size | 2414x1080? | content | provenance |
|---|---|---|---|---|
| `frames/menu-10.png` | 12 767 B | **yes** | **all-black**: mean 0.000, std 0.000, non-black px 0 | `screencap` at 11:43:17.5, `mCurrentFocus=org.herofolends.p0` at capture start, ChatGPT after (crash at 11:43:18.293) |
| `frames/menu-01.png` | 275 456 B | no (1080x2414) | ChatGPT UI | 11:40:00, app already dead |
| `frames/menu-02.png` | 572 786 B | no | ChatGPT/launcher | attempt 2, focus lost (crash) |
| `frames/menu-03.png` | 252 807 B | no | ChatGPT | attempt 2 |
| `frames/menu-04.png` | 245 647 B | no | ChatGPT | attempt 2 |
| `frames/game-01.png` | 234 544 B | no | ChatGPT | attempt 3, focus already lost |
| `frames/physical-now.png` | 298 921 B | no | Telegram | 11:46, operator app |
| `frames/vd-01.png`, `vd-now.png`, `vd-mirror-evidence.png` | ~157 KB | yes | mirror of display 0 (portrait ChatGPT letterboxed) | synthetic display 4 capture — the game surface never appeared there |

`menu-10.png` is the only native-resolution frame the app ever owned; it is pure black. Using it as a
"menu screenshot" would be fabrication, so it is reported as crash evidence only.

## 4. Attempt log (focus + crash)

| # | time | command | focus observed | outcome |
|---|---|---|---|---|
| 1 | 11:39:24 | `am force-stop` + HOME + `am start -W -n …` | `mCurrentFocus=…org.herofolends.p0/…SDLActivity` | `GAME_READY` 11:39:09; HUD rects 09.6–12.4; **SIGABRT 11:39:13.1** (tombstone_24) |
| 2 | 11:40:34 | `am start -n …` | game focused at start, ChatGPT 0.4 s later | layout at 11:40:45.4–45.9; **SIGABRT 11:43:18.3** on resume (tombstone_25) |
| 3 | 11:43:17 | resume script (`capture-02.sh`) | game at screencap start, ChatGPT at end | `menu-10.png` black 2414x1080; **SIGABRT 11:43:18.293** |
| 4 (synthetic) | 11:44:34 | `am start --display 4 -n …` on overlay display `2414x1080/560` | primary focus stayed ChatGPT/Telegram | layout 11:44:43.6–47.4; **SIGABRT 11:44:47.8** (tombstone_26); `settings delete global overlay_display_devices` → `null` |

The foreground was lost **because the app crashed** (`am_crash … Native crash,Aborted`), not because the
operator intervened; the system returned to the previous app after each `force-crash`. The packet's
limit of 3 physical launch attempts was respected; the 4th row is a non-intrusive synthetic-display
attempt (separate mechanism, no focus fight), fully reverted.

At the time of writing: `mCurrentFocus=Window{42e142 u0 org.telegram.messenger.web/org.telegram.messenger.DefaultIcon}`.

## 5. HUD geometry — device telemetry vs headless pack

Device values come from the app's own `GAME_HUD_RECTS` line (the same layout object the renderer
uses), captured twice at 2414x1080 on the physical display
(`logs/hud-rects-final.txt`, 11:40:45.902 and 11:43:17.327, identical):

```
scale=1.10 card=1878,782,373,279 tile=90.0
slots=[0@1923,827 1@2018,827 2@2112,827 3@2206,827 … 8@1923,1016 … 11@2206,1016]
army=1106,861 minimap=210,814,238,238 console=0,763,2414,317 strip=0,35 playfield=0,35,2414,728 army_slots=6
```

Headless truth: `analysis/hudskin/pack-layout-2414x1080.json` (viewport 2414x1080, scale 1.09987,
safe `[52.794, 87.990, 52.794, 52.794]`). Produced by `measure.py` (`logs/measure-output.txt`):

| element | headless (x,y,w,h) | device (x,y,w,h) | delta px |
|---|---|---|---|
| console | (0.0, 763.2, 2414.0, 316.8) | (0.0, 763.0, 2414.0, 317.0) | (+0.0, −0.2, +0.0, +0.2) |
| minimap | (52.8, 813.8, 237.6, 237.6) | (210.0, 814.0, 238.0, 238.0) | (+157.2, +0.2, +0.4, +0.4) |
| card grid | (1988.0, 782.2, 373.2, 278.8) | (1878.0, 782.0, 373.0, 279.0) | (−110.0, −0.2, −0.2, +0.2) |
| card cell side | 90.0 | 90.0 (`tile`) | +0.0 |
| card slot pitch | 94.40 | 95 | +0.60 |
| card grid right edge | 2361.2 | 2251.0 | −110.2 |
| top bar (strip) | (0.0, 0.0, 2414.0, 35.2) | (0.0, 0.0, 2414.0, 35.0) | (+0.0, +0.0, +0.0, −0.2) |
| playfield | (0.0, 35.2, 2414.0, 728.0) | (0.0, 35.0, 2414.0, 728.0) | (+0.0, −0.2, +0.0, −0.0) |
| toast plate | (899.0, 666.4, 615.9, 88.0) | n/a | needs pixels |
| tooltip plate | (899.0, 376.1, 615.9, 281.6) | n/a | needs pixels |
| gold 1-px line | not in pack JSON | n/a | needs pixels |

Reading of the horizontal deltas (observed numbers, inferred cause): the device applies
`SDL_GetWindowSafeArea` insets in a second layout pass. On the physical display the same process emits
two consecutive 2414x1080 layouts — safe=0 (`card=2006,782`, `minimap=35,814`) and safe applied
(`card=1878,782`, `minimap=210,814`) — so the shift is the device safe-area/gutter, not a pack-layout
bug. The headless JSON uses the pack's authored safe (52.8 px on all sides), hence minimap +157.2 px
and card grid −110.2 px. The synthetic display (no insets) reproduced exactly the safe=0 layout
(`logs/hud-rects-vd.txt`), which confirms the mechanism. `GAME_FIRST_FRAME` (which would print the
exact SDL safe rect) never ran on this build, so the inset values are inferred from the two passes.

Vertical geometry — console band, top bar, playfield, minimap height, card height — matches the
headless contract within 0.5 px.

## 6. Verdict per packet rule

| # | rule | verdict | basis |
|---|---|---|---|
| 1 | console top y (band ≈317 px on a 1080-tall frame) | **PASS (layout)** | console `y=763`, `h=317`; `763+317=1080`; JSON `763.2/316.8` (Δ 0.24 px). Note: 317 is the band *height*; the band *top* is 763. |
| 2 | console left/right = 0 and 2414 (full-bleed) | **PASS (layout)** | `console=0,763,2414,317` |
| 3 | minimap square side ≈238 | **PASS (layout)** | `minimap=210,814,238,238`; JSON 237.57 (Δ +0.43) |
| 4 | command-card cell ≈90 | **PASS (layout)** | `tile=90.0`; JSON slot 90.0 |
| 5 | card grid inside the frame | **PASS (layout)** | right edge 2251 < 2414 (157 px margin); 110 px left of the headless grid |
| 6 | toast/tooltip text inside their plates | **NOT MEASURABLE** | no pixels ever presented; plates are drawn per event |
| 7 | gold 1-px line present | **NOT MEASURABLE** | requires pixels |
| 8 | no empty inventory plates when only workers are selected | **NOT MEASURABLE** | requires an in-game frame + selection state |
| 9 | first frame presented (`GAME_FIRST_FRAME`) | **FAIL** | SIGABRT in the first draw; no first frame in any run |
| 10 | native-resolution menu + 2 in-game PNGs | **FAIL** | only a 2414x1080 all-black app frame exists |

Overall: **BLOCKED** for the visual acceptance; the numeric HUD contract that can be checked without
pixels is green.

## 7. Root cause and why the build is stale (evidence)

* The wgpu message says the bound uniform is **112 B** while the WGSL expects **176 B** — the old
  `CameraUniform` (`mat4x4 + light/sun/ambient`) against the grown shader `Camera`.
* The working tree already contains the fix and an explicit reference to this crash:
  `crates/wc3-game/src/render.rs:76-85` (176-byte assert), `:305-323` (both shader copies checked,
  readable error instead of SIGABRT), `:341` (`min_binding_size`), `:350` (buffer sized
  `CAMERA_UNIFORM_BYTES`), `:1703-1710` (test pins 176). The comment names
  `analysis/crash-20261117.txt` and build `0f037ea2` as the crashing artifact.
* `git log -1` = `b64b9b6 2026-11-17 11:25:05` — after the 11:18 APK build; `git status` shows
  `M crates/wc3-game/src/render.rs` (fix uncommitted, present in the tree but **not in the installed APK**).
* Conclusion (inferred, high confidence): the installed APK predates the 176-byte camera fix; the crash
  is not a HUD regression but a renderer uniform-layout mismatch.

## 8. Commands used (exact)

```bash
# build identity
su -c 'pm path org.herofolends.p0'
su -c 'sha256sum /data/app/~~iFU4tA-EJjd0mQN--qv_xQ==/org.herofolends.p0-rtKNSOmquDZ4NxVt9H4AnA==/base.apk'
sha256sum build/android-p0/herofolends-p0.apk

# launch (attempt 1)
su -c 'am force-stop org.herofolends.p0'
su -c 'input keyevent KEYCODE_HOME'
su -c 'logcat -c'
su -c 'am start -W -n org.herofolends.p0/org.libsdl.app.SDLActivity'
su -c 'dumpsys window | grep -E "mCurrentFocus"'
su -c 'logcat -d -s wc3rust:I'

# attempts 2-3
bash analysis/device-qa-20261117/capture-01.sh    # 3-frame burst, focus-checked
bash analysis/device-qa-20261117/capture-02.sh    # OCR-driven menu->PLAY, 3 in-game frames

# crash evidence
su -c 'logcat -d -b events | grep -aE "am_crash|wm_finish_activity.*wc3rust"'
su -c 'ls -lt /data/tombstones/'
su -c "grep -aE '^(Timestamp|Process name|signal|Abort message)' /data/tombstones/tombstone_26"

# synthetic display (reverted)
su -c 'settings put global overlay_display_devices "2414x1080/560"'
su -c 'dumpsys SurfaceFlinger --display-id'
su -c 'am start --display 4 -n org.herofolends.p0/org.libsdl.app.SDLActivity'
su -c 'screencap -p -d 11529215047183126904 /data/local/tmp/vd.png'
su -c 'settings delete global overlay_display_devices'   # verified: settings get → null

# measurement
python3 analysis/device-qa-20261117/measure.py \
  analysis/device-qa-20261117/frames/menu-10.png \
  analysis/device-qa-20261117/logs/hud-rects-final.txt \
  analysis/hudskin/pack-layout-2414x1080.json
```

## 9. Next steps (for the build owner — this agent does not edit source)

1. **Rebuild from the current tree** (`crates/wc3-game/src/render.rs` 176-B fix included) and install it
   (`pm install -r`). Verify on device: `GAME_CAMERA_UNIFORM bytes=176 buffer_bytes=176 …` and
   `GAME_FIRST_FRAME …` appear in `logcat -s wc3rust:I` before `GAME_STATE`.
2. Re-run `analysis/device-qa-20261117/capture-02.sh` (unchanged) — it OCRs the menu frame, taps the
   PLAY path with coordinates derived from the capture, and grabs 3 in-game frames 5 s apart.
3. Re-run `measure.py` on the real menu/in-game PNGs to fill rules 6–8 (toast/tooltip/gold line/
   inventory) and to confirm the safe-inset-dependent horizontal geometry against `GAME_FIRST_FRAME`.
4. Do not ship the `ea1f42bc…` APK: it cannot present a single frame on the target device.

## 10. Artifact index (all under `analysis/device-qa-20261117/`)

| path | size | what |
|---|---|---|
| `REPORT.md` | this file | report |
| `frames/menu-10.png` | 12 767 B | 2414x1080 all-black frame captured while the app owned the display |
| `frames/menu-01..04.png`, `game-01.png`, `physical-now.png` | 234–573 KB | focus-loss evidence (ChatGPT/Telegram at 1080x2414) |
| `frames/vd-01.png`, `vd-now.png`, `vd-mirror-evidence.png` | ~157 KB | synthetic display 4 = mirror of display 0 (no game surface) |
| `logs/launch-01.log`, `logs/launch-02.log`, `logs/attempt-03-game.log` | 1.0–2.7 MB | raw logcat of the three physical attempts |
| `logs/hud-rects-final.txt` | 718 B | the two 2414x1080 `GAME_HUD_RECTS` lines used for measurement |
| `logs/hud-rects-vd.txt` | 1 069 B | the safe=0 layout from the synthetic display |
| `logs/tombstone-26-abort.txt` | 336 B | abort signature, timestamp, exact wgpu message |
| `logs/crash-events.txt` | 634 B | `am_crash` / `force-crash` event lines |
| `logs/focus-attempt-03.txt` | 773 B | focus before/after every screenshot |
| `logs/measure-output.txt` | 1 615 B | machine output of `measure.py` |
| `capture-01.sh`, `capture-02.sh`, `find_word.py`, `measure.py` | — | reproducible harness (launch/focus/OCR/tap/measure) |

**Honesty notes:** no success claim is made for the visual acceptance. The only PNG at native
resolution from this build is black; the in-game frames requested by the packet do not exist because
the build aborts in the first draw. All layout numbers above are the app's own telemetry, not a
measurement of rendered pixels.
