# Donor matrix — design skills intake (2026-11-18)

Nine sources: three already on disk, six shallow-cloned. Every row names the file
and line read. No donor code was executed; the only tooling that ran is H2's own
(`tools/*.py` in this directory, `deep/skill_lock.py`, `deep/skill_refs.py`).

Verdicts: **adopted** (mechanism kept, text rewritten), **adapted** (mechanism
kept with a change forced by the target), **rejected** (read, not used).

## 1. `claude-design` — local `~/.hermes/skills/creative/claude-design` v1.1.0

| Mechanism | Evidence | Verdict | Where it landed |
|---|---|---|---|
| Seven surface archetypes + surface-first rule | `SKILL.md:168-191` | adopted | `design-core` SKILL Phase 2, `references/modes.md` |
| Ten slop tells, score-then-repair mapping | `SKILL.md:436-463` | adopted | `references/anti-slop.md` |
| Eight-step workflow, artifact shapes | `SKILL.md:193-243`, `:244-296` | adopted | `references/process.md` |
| Three-variant rule (conservative/strong-fit/divergent) | `SKILL.md:352-375` | adopted | `references/process.md` Step 5 |
| Content discipline (no filler, no fake metrics) | `SKILL.md:398-416` | adopted | `references/anti-slop.md`, `craft-floor.md` |
| Verification protocol (file exists → parse → render) | `SKILL.md:592-612` | adapted | `design-qa` gates; added measurement |
| HTML file defaults, React/CDN guidance | `SKILL.md:297-317` | adapted | generalised to "artifact shapes" (HUD screens are not HTML) |
| Hosted-tool plumbing (`done()`, preview panes, toolbars) | `SKILL.md:44-63`, `:642-649` | rejected | produces fake tool calls; never included |
| "Mobile hit targets ≥ 44px" | `SKILL.md:291` | adapted | replaced by platform numbers: 48 dp Android / 44 pt iOS (`craft-floor.md`) |

## 2. `design-md` — local `~/.hermes/skills/creative/design-md` v1.1.0

| Mechanism | Evidence | Verdict | Where it landed |
|---|---|---|---|
| DESIGN.md two-part anatomy and starter | `SKILL.md:39-95`, `templates/starter.md:1-99` | adopted | `design-md` SKILL, `references/schema.md` |
| Token types and component whitelist | `SKILL.md:97-109` | adopted | `references/schema.md` |
| Canonical section order | `SKILL.md:111-128` | adopted | `design-md` SKILL |
| Lint/diff/export CLI, nine rules, exit codes | `SKILL.md:144-192` | adopted | `references/lint.md` |
| YAML pitfalls (quoted hex, negative dims, silent typos) | `SKILL.md:194-213` | adopted | `references/lint.md` |
| "Ask the user for brand tone" opening | `SKILL.md:132-135` | adapted | replaced by context harvest from shipped art (`design-core` Phase 1) |
| Web/Tailwind as the primary consumer | `SKILL.md:141-142` | adapted | exports kept optional; HUD consumes tokens directly |

## 3. `popular-web-designs` — local `~/.hermes/skills/creative/popular-web-designs` v1.0.0

| Mechanism | Evidence | Verdict | Where it landed |
|---|---|---|---|
| 54 real systems with exact CSS values | `SKILL.md:116-201`, `templates/*.md` (54 files) | adopted | copied to `~/.harness2/knowledge/design-catalog/web-templates/`, indexed in `design-references/references/web-systems.md` |
| Font substitution table | `SKILL.md:88-114` | adopted | index note (a HUD uses the shipped face instead) |
| Brand-page framing ("make it look like X") | `SKILL.md:10-18` | adapted | reframed as pattern evidence, not imitation |
| Cloudflared tunnel / `browser_vision` serving flow | `SKILL.md:40-47`, `:85-86` | rejected | irrelevant to a HUD and requires external services |

## 4. `google-labs-code/design.md` @ `db977b3` (stitch-skills)

| Mechanism | Evidence | Verdict | Where it landed |
|---|---|---|---|
| Normative token schema, value types | `docs/spec.md:79-150` | adopted | `design-md/references/schema.md` |
| Grouped tokens, flattening, collisions | `docs/spec.md:156-158` | adopted | same |
| External `@` references and error levels | `docs/spec.md:160-196` | adopted | same |
| Ten-section order and aliases | `docs/spec.md:196-213` | adopted | `design-md` SKILL |
| Per-section prose/token pairing | `docs/spec.md:246-500` | adopted | `schema.md` section table |
| Motion token shape + example curves (100/250/150 ms) | `docs/spec.md:457-500` | adopted | `design-core/references/motion.md`, `design-md` schema |
| Component property whitelist | `docs/spec.md:553-566` | adopted | `schema.md` |
| Consumer behaviour for unknown content | `docs/spec.md:607-624` | adopted | `schema.md`; basis for the `hud:` extension |
| CLI (`@google/design.md`) as the gate | donated via local skill `SKILL.md:144-192` | adopted | `design-md/references/lint.md` |

## 5. `anthropics/skills` @ `e4bb5d9` — `skills/frontend-design`

| Mechanism | Evidence | Verdict | Where it landed |
|---|---|---|---|
| Ground the design in subject matter | `SKILL.md:11-13` | adopted | `design-core` Phase 1 |
| Five current AI clusters | `SKILL.md:38-45` | adopted | `references/anti-slop.md` |
| Plan → adversarial review → build (two passes) | `SKILL.md:47-53` | adopted | `references/process.md` Step 3 (the "would a generic prompt produce this?" test) |
| One bold place + quality floor | `SKILL.md:59` | adopted | `craft-floor.md`, invariants |
| Interface copy doctrine | `SKILL.md:61-71` | adopted | `references/writing.md` |
| "Client studio" framing | `SKILL.md:9` | adapted | operator-as-owner framing |
| Typeface-by-typographic-canon advice | `SKILL.md:21-23` | adapted | kept as principle, not as font mandates |

## 6. `Leonxlnx/taste-skill` @ `f6b7657`

| Mechanism | Evidence | Verdict | Where it landed |
|---|---|---|---|
| Spacing/rhythm ladder (4…128, nav 72/56, section 120/96) | `references/system.md:10-42` | adopted | `craft-floor.md` numbers |
| Hover/loop numbers (2 px lift, 1.02–1.03 zoom, 2.5–4 s) | `SKILL.md:41-45` | adopted | `motion.md` |
| "Never" list (fade-ins, stock imagery, invented reviews) | `SKILL.md:365-377` | adopted | `anti-slop.md` |
| Equal edges, one section per screen, type floor | `references/layout-v4.md:22-137`, `references/system.md:43-65` | adapted | HUD regions instead of page sections; numbers kept |
| Finish gate ("no FAIL left unfixed") | `SKILL.md:267-290` | adapted | `design-qa` gates G1–G5 |
| Node/Chrome checker scripts (`gate.mjs`, `check.mjs`) | `SKILL.md:14-19`, `:267-270` | rejected | donor code must not run; H2 ships stdlib auditors |
| Absolute "no violet" | `SKILL.md:373-374` | adapted | narrowed to "violet is a default, not a choice" — a pinned brief wins |

## 7. `pbakaus/impeccable` @ `d41d986`

| Mechanism | Evidence | Verdict | Where it landed |
|---|---|---|---|
| Craft floor verify list | `reference/craft-floor.md:5-17` | adopted | `design-core/references/craft-floor.md` |
| Refuse list + the one outright ban (kicker) | `reference/craft-floor.md:19-44` | adopted | `anti-slop.md` |
| Four modes (Persuade/Operate/Read/Experience) | `SKILL.md:35-44` | adopted | `modes.md` |
| Five-dimension audit, 0–4, rating bands | `reference/audit.native.md:9-96` | adopted | `design-qa/references/audit-rubric.md` |
| P0–P3 severity and finding fields | `reference/audit.native.md:90-107` | adopted | same |
| 48 dp / 8 dp targets, Material roles, insets | `reference/android.md:13-40` | adopted | `checklist.md`, `craft-floor.md` |
| 44 pt, Dynamic Type, Reduce Motion | `reference/ios.md` | adopted | same |
| Nielsen heuristic scoring | `reference/critique-rubric.md:112-317` | adapted | compressed into the G5 states gate |
| Launcher (`scripts/impeccable context`), detector hooks, live mode | `SKILL.md:22-26`, `:87-90` | rejected | requires executing donor binaries; not allowed here |
| Device capture (`adb … screencap`, font-scale overrides) | `reference/android.md:42-46` | rejected | lane rule: no device commands; renders come from the project mock path |

## 8. `VoltAgent/awesome-design-md` @ `fcc7028`

| Mechanism | Evidence | Verdict | Where it landed |
|---|---|---|---|
| 84 spec-format DESIGN.md systems | `design-md/<slug>/DESIGN.md` (84) | adopted | catalog copy + `web-systems.md` index |
| Component distributions (cards, badges, pills, nav, modal) | parsed `components:` blocks | adopted | `patterns.md` evidence tables |
| Named component values (notion card, stripe pills, vercel nav) | `design-md/notion/DESIGN.md`, `stripe/DESIGN.md`, `vercel/DESIGN.md` | adopted | `patterns.md` examples |
| Brand surfaces as ready-made pages | — | rejected | only values and principles are extracted |

## 9. `Meliwat/awesome-ios-design-md` @ `5d3aeca`

| Mechanism | Evidence | Verdict | Where it landed |
|---|---|---|---|
| 200 apps × `DESIGN.md` + SwiftUI/Android/Expo guides (1 011 files) | `design-md/<category>/<app>/` | adopted | catalog copy + `mobile-systems.md` index |
| Row heights and target counts (44/49/56/72 pt) | counted with `rg` across 200 `DESIGN.md` | adopted | `patterns.md` §4, `checklist.md` |
| Cross-platform resolution of one design (iOS vs Material 3) | `dating/hinge/DESIGN-swiftui.md`, `DESIGN-android.md` | adopted | `design-references` selection rules |
| 44 pt heart-tap below Material minimum + 8 dp hit-slop note | `dating/hinge/DESIGN-android.md:740` | adopted | `checklist.md` D1/D2 |

## Cross-cutting findings (observed)

| Finding | Evidence | Consequence |
|---|---|---|
| 3 of 84 web DESIGN.md files are invalid YAML (unquoted colon in `description`) | `elevenlabs`, `raycast`, `supabase` | recorded in `design-references` SKILL; those are read as prose |
| 10 of 84 have no front matter | kraken, lamborghini, lovable, mastercard, runwayml, sanity, spotify, starbucks, tesla, theverge | index shows em dashes for their values; no invented tokens |
| `impeccable` is a multi-runtime plugin (4 780 files, Rust + binaries) | repo inventory | only `SKILL.md` + `reference/*.md` read; nothing executed |
| `taste-skill` ships a Node gate and 963 SVG assets | repo inventory | rejected; H2 ships its own stdlib checks |
| A concurrent agent created `design-taste` at 15:53 while this lane ran | `~/.config/opencode/skills/design-taste/` mtime | not touched (other owner); overlap reported to the tech-lead |

## Resulting H2 skills

| Skill | Files | Bytes | Role |
|---|---|---|---|
| `design-core` | 8 | 41 322 | process, taste, anti-slop, craft floor, copy, motion, modes |
| `design-md` | 6 | 37 247 | token spec + HUD extension (9-slice, dp, states) |
| `design-references` | 6 | 61 721 | 84 + 54 + 200 catalogued systems, pattern-indexed |
| `design-qa` | 8 | 40 017 | 5 gates, 2 stdlib auditors, rubric |
