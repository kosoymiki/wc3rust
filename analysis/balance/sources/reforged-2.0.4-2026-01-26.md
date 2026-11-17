# Source S2 — Warcraft III: Reforged Patch Notes, Version 2.0.4

- Publisher: Blizzard Entertainment (official forum post by Kaivax)
- URL: https://us.forums.blizzard.com/en/warcraft3/t/36567
- Published: 2026-01-26 (page created 2025-07-17, edited)
- Accessed: 2026-11-16
- Method: stealth-browser-mcp — spawn_browser(headless) → navigate(`/t/36567.json`) → get_page_content (saved JSON) → close_instance
- Completeness: full opening post retrieved; Balance section read end to end (lines 426–519 of the rendered post).

Excerpts used by `reforged-2.x.json` (verbatim):

```
Balance
General
...
Piercing damage against Heavy armor decreased from 100% to 90%

Human
...
Mountain King Bash will now be able to stun magic immune air units when attacking with an orb

Orc
...
Pulverize can now target Wards
Burning Oil can now target Wards

Night Elf
...
Wisp Lumber Gather rate decreased from 7 to 7.5 seconds
Archer health increased from 255 to 275
...
Hippogryph Rider health increased from 780 to 800
...
Moon Glaives upgrade time decreased from 60 to 50

Undead
...
Lich Frost Nova range increased from 700/700/700 to 750/750/750

Neutral
...
Dark Ranger Drain Life mana cost decreased from 50/50/50 to 25/25/25
Tinker Pocket Factory spawn interval decreased from 5/4.5/3.5/3 to 4.5/4/3.5/3 per Engineering level
Tinker Cluster Rockets max damage per interval increased from 105/195/300 to 200/250/300
Brewmaster Breath of Fire damage increased from 65/125/170 to 80/130/180
Brewmaster Breath of Fire max damage increased from 480/800/1200 to 600/900/1200
...
Brewmaster Breath of Fire damage over time from 7/14/21 to 10/15/25
```

Notes:
- The Piercing-vs-Heavy armor change (100% → 90%) is a combat-table constant, not a per-def field; it is documented in `../DIFF.md` as a follow-up for the damage-table owner.
- Neutral hero/creep/item rows are recorded in `../DIFF.md` only (no ROSTER def exists for them).
