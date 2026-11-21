# Design skills intake — delivery report (ledger 543)

Date: 2026-11-18 · Owner: design-intake subagent · Packet:
`analysis/tech-lead/PACKETS-DESIGN-2026-11-18.md` (lane 542 section, operator
operator correction applied: provenance only — repo, path, commit, sha256).

## What was built

Four H2 skills under `~/.config/opencode/skills/`, merged from nine sources, each
with front matter, `references/`, working relative links and a `lock.json`:

| Skill | Role | Trigger phrases advertised |
|---|---|---|
| `design-core` | process + taste + anti-slop + craft floor | design this screen · make it look intentional · looks generic / templated / AI-made · design process · critique this UI · pick a visual direction · build design variants · ui looks like slop |
| `design-md` | DESIGN.md token spec + HUD extension | design tokens · write DESIGN.md · design system spec · token file for the UI · palette roles and type scale · 9-slice insets · define the HUD tokens |
| `design-references` | 84 + 54 + 200 catalogued systems, pattern-indexed | find a reference for this panel · what radius do real systems use · show me tab bar patterns · design like stripe / linear / vercel · mobile design specs · hud pattern reference · exact component values |
| `design-qa` | five gates, layout-dump auditor, contrast calculator | verify this screen · check contrast · is the text clipped · measure the layout · design qa · audit this UI · check touch targets · hud acceptance check |

## Tree

```
~/.config/opencode/skills/
├── design-core/            8 files  41 322 B
│   ├── SKILL.md
│   ├── lock.json
│   └── references/{process,anti-slop,craft-floor,writing,motion,modes,provenance}.md
├── design-md/              6 files  37 247 B
│   ├── SKILL.md
│   ├── lock.json
│   └── references/{schema,hud-extension,template-hud,lint,provenance}.md
├── design-references/      6 files  61 721 B
│   ├── SKILL.md
│   ├── lock.json
│   └── references/{patterns,hud-mapping,web-systems,mobile-systems,provenance}.md
└── design-qa/              8 files  40 017 B
    ├── SKILL.md
    ├── lock.json
    ├── references/{checklist,contrast,metrics,audit-rubric,provenance}.md
    └── scripts/{contrast.py,layout_audit.py}
```

## Lock digests

`lock.json` records sha256 per file (all files except `lock.json` itself, which
cannot hash itself) and the aggregate directory digest in the shape
`deep/skill_lock.py:check_lock_against_tree` verifies.

| Skill | files | dir sha256 |
|---|---|---|
| design-core | 8 | `5bb69d30695871c7ba3669425813f3a6589d92dded0c56601a8610699ce59ac0` |
| design-md | 6 | `689dd4b5d8df016c53641d0bff22ba8fc51361ade8fa47af39ca5b6e16b9aab3` |
| design-references | 6 | `b9453e187b0ff3d82a1af5f20a317092b7821c12eb66795e8f599612471281cc` |
| design-qa | 8 | `21fdc3215350b2bf0781907ef7c932e93aab448780ba6b653b2bb059ef990b65` |

Per-file samples (first three per skill):

```
design-core       28707b75fdfb SKILL.md
                  de9bb9111d9d references/anti-slop.md
                  11803219065a references/craft-floor.md
design-md         6ca6187c3d3a SKILL.md
                  47ab7dbc8f0c references/hud-extension.md
                  ae6b9bff1ed2 references/lint.md
design-references 6ebb2226ab34 SKILL.md
                  3046c0fc1dab references/hud-mapping.md
                  40d0239abdd7 references/mobile-systems.md
design-qa         e408bcb417c5 SKILL.md
                  2a6ed090ce7b references/audit-rubric.md
                  3ef25e7a5a8f references/checklist.md
```

## Validation output (`deep/skill_lock.py` + `deep/skill_refs.py`)

```
design-core: skill_refs errors=[] | skill_lock errors=[] | staged_digest=5bb69d30695871c7
design-md: skill_refs errors=[] | skill_lock errors=[] | staged_digest=689dd4b5d8df016c
design-references: skill_refs errors=[] | skill_lock errors=[] | staged_digest=b9453e187b0ff3d8
design-qa: skill_refs errors=[] | skill_lock errors=[] | staged_digest=21fdc3215350b2bf
```

Generator run (`tools/gen_locks.py`, also verifies per-file digests and links):

```
| skill | files | bytes | dir sha256 | files+links |
|---|---|---|---|---|
| design-core | 8 | 41322 | `5bb69d30695871c7…` | OK |
| design-md | 6 | 37247 | `689dd4b5d8df016c…` | OK |
| design-references | 6 | 61721 | `b9453e187b0ff3d8…` | OK |
| design-qa | 8 | 40017 | `21fdc3215350b2bf…` | OK |

ALL CHECKS PASSED
```

`validate_skills.py ~/.config/opencode/skills`:

```
Skills: 16  Errors: 0  Warnings: 0
ALL CHECKS PASSED (warnings allowed)
```

## Functional evidence for `design-qa` tooling

```
$ layout_audit.py fixtures/hud-layout-good.json
audited 23 elements at 2414×1080 px @ 3.0x: 307 pass checks, 0 failures,
0 overlapping pair(s), 36 gap pair(s)                                    rc=0

$ layout_audit.py fixtures/hud-layout-bad.json
FAIL panel_offscreen: off-screen — rect [2300.0,100.0,300.0,200.0] outside 2414.0×1080.0
FAIL panel_offscreen: safe-area — rect exceeds inset L36 R36 T24 B24 px
FAIL panel_outside_safe: safe-area — rect exceeds inset L36 R36 T24 B24 px
FAIL tiny_button: target-size — 120×120 px < 48 dp (144 px)
FAIL clipped_label: text-fit — 'Not enough lumber' needs ~421 px at 45 px font, box is 120 px
FAIL overlap_b: off-screen — rect [700.0,900.0,400.0,240.0] outside 2414.0×1080.0
FAIL overlap_b: safe-area — rect exceeds inset L36 R36 T24 B24 px
FAIL overlap_a ↔ overlap_b: overlap — 28000 px² shared
audited 6 elements ... 8 failures, 1 overlapping pair(s)                  rc=1

$ contrast.py palette fixtures/hud-palette.txt
PASS ink_on_panel: 14.26:1 (normal, AAA)
PASS muted_on_panel: 9.19:1
PASS accent_on_panel: 8.93:1
PASS border_vs_panel: 3.06:1 (ui, AA)
PASS border_strong: 5.63:1 (ui, AAA)
PASS ink_on_raised: 12.98:1
PASS disabled_on_panel: 4.38:1 (ui, AA — disabled text is exempt from 4.5)
PASS danger_on_panel: 5.37:1
PASS success_on_panel: 6.99:1
PASS ink_on_sunken: 15.31:1
checked 10 pair(s)                                                       rc=0
```

The contrast run caught a real defect in the HUD template: `danger` #cf4a3e on
`panel` #1b1813 was **3.96:1** (below AA for text). Replaced with #e2685c (5.37:1)
in `design-md/references/template-hud.md`.

## AGENTS.md registration

Four lines inserted into the AVAILABLE_SKILLS block (backup:
`AGENTS.md.bak-20261118-designintake`). `skills_sync.py` after the edit:

```
Current: [... deep-donor, design-core, design-md, design-qa, design-references, engineering-reflection ...]
Added: ['design-taste', 'gitx', 'hud-pack-integration']
```

My four are registered; the three remaining additions are other owners' (see
open items).

## Donor matrix

Full adopted/adapted/rejected table with `repo@commit path:line` for all nine
sources: `DONOR-MATRIX.md` (same directory).

## Artifacts in this directory

```
analysis/design-intake/
├── DONOR-MATRIX.md                    nine-source matrix, file:line verdicts
├── REPORT.md                          this report
├── generated/
│   ├── donor-values.json              394 948 B extraction (84 + 54 + 200 records)
│   └── validation.log                validator run output
├── fixtures/
│   ├── hud-layout-good.json           passing 23-element HUD layout
│   ├── hud-layout-bad.json            6 defect classes to prove detection
│   └── hud-palette.txt                10 contrast pairs
└── tools/
    ├── extract_donors.py              read-only donor extraction
    ├── gen_indexes.py                 regenerates web/mobile indexes deterministically
    └── gen_locks.py                   writes lock.json + runs both validators
```

Reproduce:

```bash
cd ~/src/wc3rust/analysis/design-intake
python3 tools/extract_donors.py --json generated/donor-values.json --md-dir generated
python3 tools/gen_indexes.py --json generated/donor-values.json \
  --out-dir ~/.config/opencode/skills/design-references/references
python3 tools/gen_locks.py
```

## Catalogue copy

`~/.harness2/knowledge/design-catalog/` now holds the durable catalogue the
indexes point at: `voltagent-design-md/` (84, 3.2 MB), `meliwat-ios/` (200 apps,
1 011 md, 20 MB), `web-templates/` (54, 948 KB). Temporary clones live in
`$PREFIX/tmp/donors-design/` and can be pruned.

## Open items (owners named, no silent gaps)

1. **Concurrent `design-taste` skill** — created 15:53 by another agent while this
   lane ran; overlaps `design-core` in intent and uses the older donor-metadata
   table that this lane replaced with commit + sha256 rows. Not touched (other
   owner). Tech-lead decision needed: keep as an alias/thin pointer to
   `design-core`, or retire it.
2. **`gitx` and `hud-pack-integration` missing from AVAILABLE_SKILLS** —
   pre-existing drift, other owners; `skills_sync.py` reports them as `Added`.
3. **Lane 543 gaps** — retail 9-slice insets, unit-card composition and minimap
   terrain are not answerable from these donors; recorded in
   `design-references/references/hud-mapping.md` with the exact missing numbers.
