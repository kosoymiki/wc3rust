# Source S1 — Warcraft III: Reforged Patch Notes, Version 2.0.2

- Publisher: Blizzard Entertainment (official forum post by Kaivax)
- URL: https://us.forums.blizzard.com/en/warcraft3/t/35355
- Published: 2025-04-15 (Build 22761)
- Accessed: 2026-11-16
- Method: stealth-browser-mcp — spawn_browser(headless) → navigate(`/t/35355.json`) → execute_script (JSON post body → rendered text) → close_instance
- Completeness: full opening post retrieved (len 7184 chars); balance section read end to end.

Excerpts used by `reforged-2.x.json` (verbatim):

```
Human
...
Flame Strike mana cost reduced from 135/135/135 to 125/125/125
...
Holy Light cooldown increased from 5/5/5 to 5.5/5.5/5.5 seconds
...
Militia Armor reduced from 4 to 3
...
Defend cost reduced from 150/100 to 125/75
Defend research time reduced from 45 to 40 seconds
Slow duration reduced from 60 to 45 seconds
Invisibility Range increase from 300 to 400
Heal range increased from 250 to 350
Flying Machine movement speed reduced from 400 to 375

Orc
...
Great Hall build time reduced from 140 to 135 seconds
Envenomed Spears damage per tick reduced from 4 to 3
Wind Rider movement speed reduced from 320 to 310
...
Tauren now have Resistant Skin
Tauren cost increased from 280/80 to 300/100

Night Elf
...
Keeper of the Grove strength gain increased from 1.8 to 2
...
Upgrade Moon Glaives cost increased from 100/150 to 125/175
Upgrade Moon Glaives research time increased from 35 to 60 seconds
Huntress movement speed reduced from 350 to 340
Ultra Vision research time reduced from 45 to 30 seconds
Chimaera movement speed increased from 250 to 270
Vorpal Blades upgrade now grants an additional 10 damage

Undead
...
Frost Nova range reduced from 800 to 700
Carrion Swarm damage increased from 75/125/200 to 75/135/200
...
Burrow research time reduced from 45 to 30 seconds
Gargoyle movement speed reduced from 375 to 350
...
Frost Wyrm ground and air attack range increased from 300 to 375
```

Notes:
- 2.0.1 (https://us.forums.blizzard.com/en/warcraft3/t/33148, 2024-11-13) was retrieved the same way and contains no Balance section (engine/UI/lighting only).
- Values not applicable to the four-race ROSTER (neutral heroes, creeps, items, Ward-target changes) are listed in `../DIFF.md` and were not turned into overlay rows.
