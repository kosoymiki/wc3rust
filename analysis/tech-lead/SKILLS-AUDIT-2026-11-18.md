# Skills audit — 2026-11-18 (ledger 560)

Root: `~/.config/opencode/skills` · 16 skill directories.

## validate_skills.py

```
$ python3 ~/.harness2/scripts/validate_skills.py ~/.config/opencode/skills
...
Skills: 16  Errors: 0  Warnings: 0
ALL CHECKS PASSED (warnings allowed)
rc=0
```

The only INFO lines are word-count notes and one trigger-phrase hint on
`handoff-snapshot`; no warnings, no errors.

## skills_sync.py (dry-run diff, then applied by hand)

Before: 13 names in the `AVAILABLE_SKILLS` block, missing `design-taste`, `gitx`,
`hud-pack-integration`, and `deep-donor` sat out of alphabetical order.

```
$ python3 ~/.harness2/scripts/skills_sync.py --skills-dir ~/.config/opencode/skills \
      --target ~/.config/opencode/AGENTS.md
Added: ['design-taste', 'gitx', 'hud-pack-integration']
Removed: (none)
In sync: False

# after the edit
Added: (none)
Removed: (none)
In sync: True
```

`AGENTS.md` block now lists 16 entries, alphabetically, between
`<!-- AVAILABLE_SKILLS_START -->` and `<!-- AVAILABLE_SKILLS_END -->`.

## Relative links (`skill_refs.check_skill_refs`)

```
refs: skills=16 errors=1
contracts-gate ERRORS ['tests/fixtures/invalid-broken-link/guide.md: broken link -> missing.md']
```

That link is the **intentional negative fixture** of `contracts-gate`
(`tests/fixtures/{valid,invalid-broken-link,invalid-missing-info,invalid-no-operationid}`);
the fixture must stay broken for the skill's own test. No other skill has a broken link.

## lock.json (`analysis/tech-lead/tools/skill_lock_check.py`)

```
PASS design-core
PASS design-md
PASS design-qa
PASS design-references
locks=4 failed=0
rc=0
```

Each lock is verified twice: every `files[].sha256` against the file, and the
directory digest against `sources[].sha256` with `dir_sha256_excludes=["lock.json"]`.

Note: the generic `~/.harness2/scripts/deep/skill_lock.py` reports a false error on
these locks (`skill dir missing …/design-core/design-core`) because it resolves
`dest: "."` as a repository root and hashes `lock.json` into the digest. The skills
are correct; the local checker implements the lock's documented semantics.
