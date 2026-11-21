# OPEN TAILS — wc3rust, snapshot 2026-11-18 (ledger 560)

Every row names one owner and the exact next step. Status vocabulary:
`operator` (only the operator can act), `blocked` (external gate, evidence recorded),
`in-progress` (a lane owns it now), `closed`.

Source anchors are file:line / report lines so the next agent does not re-discover.

| # | Tail | Status | Owner | Exact next step | Anchor / evidence |
|---|------|--------|-------|-----------------|-------------------|
| a | Install / run the APK on the phone and record a device verdict | `operator` | operator + tech-lead | Operator installs the current APK and opens it; tech-lead schedules the walk only when the phone is free. `H2_DEVICE_OK=1` is the only unlock. | `analysis/tech-lead/DEVICE-POLICY.md`; scripts now exit 2 without the env var |
| b | `FINDINGS_P0-P4.md` assembled from the six audit reports + interim TZ | `in-progress` | tech-lead (this line) | Merge the 6 lane reports (G 782, H 536, I 813, J 680, K 209, 547 876 lines) with `INTERIM-TZ-ASSIGNMENT.md` (654 lines, 21 findings I-001…I-021), dedupe by `file:line`, sort P0→P4, keep one finding row per defect. Artifact does not exist yet. | `analysis/full-audit-2026-11-18/lane-*.md`; `analysis/tech-lead/INTERIM-TZ-ASSIGNMENT.md` |
| c | P1 JASS: `war3map.j` `config` never run; registered trigger events never dispatched; `TriggerSleepAction` inside a called function does not suspend the caller | `in-progress` | lane 537 | Take the three P1s from lane H §0/§findings, implement `config` execution + event dispatch + nested `sleep` suspension in the live mission VM (`crates/wc3-game/src/game.rs:1759-1777` path), prove with a Prologue01 probe. | `lane-H-data-jass.md:17-19`; 69/69 files audited |
| d | `ui/menu.rs` `Difficulty: Hash` and `hudskin/mod.rs` duplicate `manifest`; tree temporarily red | `in-progress` | lanes 554 / 555 | Re-run `scripts/h2-cargo check` after the uncommitted edits settle. Current source already shows `#[derive(…, Hash)]` on `Difficulty` (`crates/wc3-game/src/ui/menu.rs:88-89`) and a single `pub mod manifest;` (`crates/wc3-game/src/hudskin/mod.rs:21`); compile confirmation is still owed by the owning lanes (cargo not run in ledger 560). | `git diff` on both files; `menu.rs:88`, `hudskin/mod.rs:21`, `pack_build.rs:22` |
| e | `state_hash` blind to 14 live simulation fields (WC3-SIM-001) | `in-progress` | lane 539 | Fold the 14 missing live fields (orders, timers, buffs, transient flags) into `Sim::state_hash` and prove divergence detection with a two-sim probe that mutates one field without a tick. | `lane-I-sim-input-media.md:186,245-248`; `crates/wc3-sim/src/lib.rs:7096` |
| f | Sound bug | `closed` | — | Not needed (operator decision). No action; do not reopen. | ledger 560 instruction |
| g | Mini-map §3.7 exists as a spec but is not wired into the runtime HUD | `in-progress` | lane 553 | Wire `hudskin/minimap_frame.rs` + fog/blips/camera rect into the live HUD path, then produce one headless frame artifact. The module returns the image slot and camera slot; the caller must draw them. | `crates/wc3-game/src/hudskin/minimap_frame.rs:1-17`; design rows `analysis/design/HUD-DIRECTION.md:137,181,289-290` |
| h | MP3 pitch (playback rate/tonality wrong) | `in-progress` | lane 546 | Reproduce with a synthetic 440 Hz MP3, compare decoded sample rate to the frame header, then fix the rate path in `wc3-audio` (`wc3-bink/src/mp3` tables supply the rate). | lane 546 dispatch; `crates/wc3-bink/src/mp3/tables.rs` |
| i | Release-gate identity: an APK not built from the audited SHA can pass QA | `in-progress` | lane 541 | Make `scripts/build-apk.sh` stamp `versionCode`/`versionName` from the tree SHA and make the acceptance gate hash the installed `base.apk`; missing identity must FAIL. | `lane-K-platform-build.md:14,20,43-58,81` (K-01, P1) |
| j | Device access by agents | `operator` | operator | Agents never call `am`/`input`/`pm`/`screencap`/`logcat -c`. Device work is operator-run or tech-lead-run only after `H2_DEVICE_OK=1`. | `analysis/tech-lead/DEVICE-POLICY.md`; enforced in both walk scripts |
| k | `~/tmp/cn/*.img` + `payload.bin` ≈ 15.3 GB of unpacked firmware images outside the repo | `operator` | operator | Confirm the images are backed up/reproducible, then delete; no agent may mass-delete 15 GB without the operator's explicit go. | `du`/`find`: `payload.bin` 7.2 GB, `my_stock.img` 2.0 GB, `odm.img` 1.7 GB, `my_heytap.img` 1.2 GB, `system_ext.img` 1.0 GB, `my_bigball.img` 0.85 GB, `my_product.img` 0.72 GB, `system.img` 0.64 GB |
| l | `analysis/trash-2026-11-18/` holds 689 old logs (35 MB on disk) | `operator` | operator | Review `MOVED-LOGS.csv`; delete the folder when the evidence is no longer needed (agent `rm` is DELETE_GUARD-blocked by design). | `analysis/tech-lead/STORAGE-DIET-2026-11-18.md` |

## Tails closed in ledger 560 (evidence)

| Item | Result |
|---|---|
| Device guard in `device_menu_walk.py` / `device_battle_walk.py` | Both exit `rc=2` with the gate message when `H2_DEVICE_OK` is unset, `--help` returns `rc=0` without the gate, `py_compile` rc=0. Gate is checked at import and again inside every `sh`/`sh_bytes`. |
| Skills block in `~/.config/opencode/AGENTS.md` | `skills_sync.py` → `In sync: True`; block lists 16 skills (was 13); `validate_skills.py` → 16 skills, 0 errors, 0 warnings. |
| `lock.json` in design-* skills | 4/4 PASS via `analysis/tech-lead/tools/skill_lock_check.py` (per-file digests + dir digest excluding `lock.json`). The generic `skill_lock.py` reports a false error because it resolves `dest: "."` twice and hashes `lock.json`; recorded, not a skill defect. |
| `FILE_COVERAGE.csv` union | 784 rows, 784 unique paths, 0 duplicates, one header, 0 empty statuses; all five lane files are subsets (0 missing); 11 rows added by lane 547. | 
| Storage diet | PNG 284: 1368.4 MB → 339.9 MB (−1028.5 MB); 689 logs (33.0 MB) moved to `analysis/trash-2026-11-18/`; `du analysis` 4.3 G → 3.3 G; 284/284 PNG re-open clean. See `analysis/tech-lead/STORAGE-DIET-2026-11-18.md`. | 

## Not tails (deliberately untouched)

- Source game data `analysis/tft_files_Files100_War3x.mpq` (287 MB) and `…War3xlocal.mpq` (51.5 MB): inputs, not artifacts.
- `analysis/agent-hud/redraw-v0.1/*.psd` (67.6 + 22.2 MB): layered sources; compressed only if the operator asks.
- `~/tmp/cn/*.img` (≈8 GB): unrelated system images outside the repo.
