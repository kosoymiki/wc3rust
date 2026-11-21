# DESIGN WAVE — thick packets (2026-11-18, tech-lead)

Operator directive: the UI/UX/HUD must be re-reviewed from the ground up against real
design craft, with hard reliance on donors. Two lanes: intake (skills) and overhaul
(visual direction + assets). Neither may write tests or touch the phone.

## Donor base (read first)

Local, already on disk (Hermes skill root `~/.hermes/skills/creative/`):
- `claude-design/` — process and taste (brief → variants → verification, anti-slop rules)
- `design-md/` — formal DESIGN.md token-spec format
- `popular-web-designs/` — 54 real design systems as HTML/CSS templates (from
  `VoltAgent/awesome-design-md`)

Remote donors to fetch (shallow clone, byte inventory, no execution):
1. `google-labs-code/design.md` (stitch-skills) — DESIGN.md spec + examples
2. `anthropics/skills` → `frontend-design` — design process/taste skill
3. `Leonxlnx/taste-skill` — taste/quality bar
4. `pbakaus/impeccable` — impeccable UI rules
5. `VoltAgent/awesome-design-md` — design-system catalog
6. `Meliwat/awesome-ios-design-md` — mobile/iOS design specs

Rules: donor intake is NON-BYPASSABLE (file-to-file byte analysis → matrix → merge the
strongest → verify → persist). Never execute donor code. No CAPTCHA/paywall bypass; use
the stealth-browser MCP for web reading (primary), `git clone --depth 1` for repos.

## Lane 542 — DESIGN SKILLS INTAKE (ledger 542)

OWNER: `~/.config/opencode/skills/**`, `~/.harness2/memory.md` pointer,
`analysis/design-intake/**`.

Deliver one coherent H2 skill set, not six copies:
- `design-core` — the design *process*: brief, constraints, variants, critique loop,
  anti-slop checklist, verification of a rendered artifact. Merge `claude-design` +
  Anthropic `frontend-design` + `taste-skill` + `impeccable`.
- `design-md` — token spec format (DESIGN.md) with the fields a game HUD needs:
  palette roles, type scale, spacing rhythm, 9-slice insets, elevation, motion.
- `design-references` — the catalogs: 54 web systems + the iOS/mobile specs, indexed by
  *pattern* (framed panel, pill, tab row, list row, tooltip, modal), each entry carrying
  the file path and the exact values (colors, radii, insets).
- `design-qa` — how to prove a screen is right: pixel measurements, contrast ratios,
  target sizes (>= 48 dp), no clipped text, no off-screen rects, screenshot checklist.

Requirements: `SKILL.md` with front-matter (name/description/version/triggers), references
under `references/`, a `lock.json` (sha256 per file) and working relative links —
validate with the existing `deep-donor` helpers (`deep/skill_lock.py`,
`deep/skill_refs.py`) or the equivalent, and paste the check output. Update the
AVAILABLE_SKILLS block in `~/.config/opencode/AGENTS.md` (one line per skill).

REPORT: donor matrix (6 sources × what was adopted/adapted/rejected), the file tree, the
lock digests, the link check result.

## Lane 543 — VISUAL OVERHAUL: UI/UX/HUD (ledger 543)

OWNER: `crates/wc3-game/src/mobile_hud.rs`, `crates/wc3-game/src/hudskin/**`,
`analysis/agent-hud/**`, `analysis/design/**`.
READ_ONLY: `crates/wc3-game/src/game.rs` (tech-lead), `crates/wc3-game/src/assets.rs`
(lane 520), `crates/wc3-render/**` (lane 540), everything else.

Goal: a WC3-authentic mobile HUD and glue menu, judged on 2414×1080 / 805×360 dp, built
from real references and the donor catalogs.

Phases
1. **Reference hunt — 50 items.** GitHub only, real files: custom WC3 UI packs, RTS mobile
   HUDs, fantasy game UI kits, 9-slice panel sets, WoW/WC3 addon UI, Flutter/Unity RTS HUD
   repos, and the pattern entries of `design-references`. For each: repo, path, license,
   what pattern it proves (panel geometry, tab row, unit card, command grid, tooltip,
   resource pill, minimap frame), and the exact numbers you take from it. Record
   **provenance** (repo, path, commit) — not licence bookkeeping: donor art and code are in
   scope for the operator's private build, and no copyright/redistribution caveat belongs
   in any intake doc, README or delivered artifact. Store as
   `analysis/design/REFERENCES-50.md` with links, commit and access date.
2. **Direction.** Write `analysis/design/HUD-DIRECTION.md`: the visual language (palette
   roles from the shipped `war3skins`/`CommandButtons` art, type scale with Nimrod,
   spacing rhythm in dp, 9-slice inset table, elevation/states, motion), and a critique of
   the current screens with measurements (what is off and by how much).
3. **Assets.** Rebuild what the direction needs: 9-slice panels/buttons/tabs/tooltip/
   minimap frame at device scale, from the shipped BLPs where possible (screen/atlas
   extraction) and redrawn where they do not exist. Keep provenance per asset (source
   file, transform, sha256). No placeholder art presented as final.
4. **Implementation.** Wire the direction into `mobile_hud.rs` + `hudskin/**`: one layout
   source of truth (already consolidated), retail proportions, text fit with the Nimrod
   face, states (normal/hover/pressed/disabled/locked), and the glue menu (main,
   campaign, custom game, options, pause) matching the reference geometry.
5. **Proof.** Render mocks with `cargo run --example hud_mock` (and the menu mock path) at
   2414×1080 and 1080×2414; put before/after PNGs in `analysis/design/shots/` with the
   numbers (target size, contrast, insets, font px). The operator's bar: "no clipped
   label, no colour-square placeholder, no off-screen rect, retail proportions".

RULES: no `#[test]`, nothing under `tests/`; no `am`/`input`/`pm`/`screencap`/`logcat -c`;
`h2-cargo` only; every claim carries a file path or a measured number.

REPORT: 50 refs table, direction doc, asset list with provenance, mock PNGs, the exact
measurements before/after, and what the tech-lead must wire on the device.
