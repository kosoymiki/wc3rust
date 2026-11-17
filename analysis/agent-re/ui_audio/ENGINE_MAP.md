# ENGINE_MAP — UI/FDF/FONT + AUDIO (TFT `Game.dll` + `Mss32.dll`)

Ledger 406 · layer: r2 RE · report only (no `src/` edits)
Date: 2026-11-15 · r2 6.2.6 + r2ghidra (`pdg`) · `su -c 'id'` = `uid=0(root)` (KernelSU)

Binaries (observed):

| target | path | sha256 (prefix) | class |
|---|---|---|---|
| TFT | `analysis/tft_files_Files100_Game.dll` | `d1a80c89597e08e0` | PE32 i386, base `0x6f000000`, built `Mon May 19 03:57:48 2003`, pdb `E:\Drive1\temp\buildwar3xloc\War3\bin\Game.pdb` |
| RoC | `analysis/game_files/Files100_Game.dll` | `9970c5af4ff4821f` | PE32 i386, base `0x6f000000` |
| Miles | `analysis/game_files/Files100_Mss32.dll` | `1f9a83f42de0e789` | PE32 i386, base `0x21100000` |

Method: `aa` (not `aaa`) + targeted `af`/`pdf`/`pdg` per function; string/xref discovery by
walking `rabin2 -zz` output and matching little-endian immediates inside `.text`
(`analysis/agent-re/ui_audio/re_helper.py`), then mapping hits to function starts from the
existing `analysis/agent-re/r2/*.afl.txt` dumps. All quotes below are trimmed decompiled or
disassembled lines with the instruction address.

---

## 0. TL;DR — the three decisions this map settles

1. **The command tile never carries the ability name.** `CCommandButton` owns exactly
   **one** text child — a `CTextFrame` created with the `MasterFont` font, whose texture
   companion is `CommandButtonNumberOverlay` and whose string source is `CommandButtonNumber`
   (`0x6f12c2b0`, lines `0x208/0x20c/0x20f`). Everything else in the tile is texture
   (icon, disabled art, cooldown, autocast, active highlight). Names, descriptions, costs and
   requirements are **tooltip** payload (`ToolTipName`/`ToolTipDesc`/`ToolTipCost`, 8-line
   tooltip). → Our "full names inside 64 px slots" is a data contract violation, not a
   fitting problem.
2. **Slot geometry is 4 × 3 with `size=0.038`, `gap=0.0075`, `pitch=0.0455`** (UI units where
   0.6 units = screen height). At 1920×1080 that is **68 px buttons on an 82 px pitch**; our
   64 px tile is already ~6 % *smaller* than retail. Raising the tile is cosmetic; removing the
   label text is the fix.
3. **The audible gate is per-sound `DistanceCutoff`, compared squared.** `Game.dll` refuses to
   start a 3-D sound when `cutoff² < dist²` (`0x6f102980`). Retail `UnitCombatSounds` ships
   `cutoff = 2100` (with `MinDistance 600`, `MaxDistance 10000`) — our fixed 4000 is ~2× too
   far for combat sounds and *wrong in kind* for ack sounds (`cutoff = 100000`, i.e. never
   distance-culled).

---

## 1. Command card / FDF layout

### 1.1 Observed

`CCommandBar` constructor **`0x6f12ae80`** (`..\UI/CCommandBar.cpp`, alloc line `0x14f`):

```asm
0x6f12aead  mov dword [edi + 0x59], 0xc          ; 12 buttons
0x6f12af46  fidiv dword [esi + 0x124]            ; PlaceButton(col, row, frame, 1) loop
0x6f12af69  fidiv dword [esi + 0x128]
```

```c
in_ECX[0x59] = 0xc;                       // 12 command buttons
in_ECX[0x58] = 0x6f69b7ec;                // vtable
func_0x6f013930((0.0075 + 0.038) * 4.0 - 0.0075);   // width  = 0.1745
func_0x6f013950((0.0075 + 0.038) * 3.0 - 0.0075);   // height = 0.129
func_0x6f013520(8, arg_8h /*parent*/, 8, *0x6f69ac24, *0x6f69ac28, 1);
arg_8h = 0; uVar5 = 0;
while (1) {                                // 4 x 3
  do {  ... new CCommandButton ...  func_0x6f5ebf90(arg_8h, uVar5, button, 1); uVar5++; }
  while (uVar5 < 3);
  arg_8h++; if (3 < arg_8h) break; uVar5 = 0;
}
```

* `0x6f013930` = **width** setter (`mov dword [ecx + 0x58], arg`, `0x6f013938`).
* `0x6f013950` = **height** setter (`mov dword [ecx + 0x5c], arg`, `0x6f013958`).
* Constants read from the image (`constants.json` → `command_card.evidence.globals`):
  `0x6f69ac20 = 0.0075` (gap), `0x6f699c40 = 0.038` (button), `0x6f692cbc = 4.0`,
  `0x6f69565c = 3.0`.
* Bar size is exactly `(gap + size) * n − gap`, which is self-consistent **only** for
  `size = 0.038`, `gap = 0.0075`, `pitch = 0.0455`, 4 columns × 3 rows:
  `4·0.0455 − 0.0075 = 0.1745` ✓, `3·0.0455 − 0.0075 = 0.129` ✓.

`CCommandButton` constructor **`0x6f12c2b0`**:

```c
iVar1 = func_0x6f37bd76(0x124, "..\\UI/CCommandButton.cpp", 0x208, 0);  // new(0x124)
*(param_1 + 0x194) = func_0x6f5eacd0(param_1);        // base frame
func_0x6f013520(8, param_1, 8, 0, 0, 1);              // anchors (all-points style)
func_0x6f013950(*0x6f69ac2c);                          // height = 0.015625
func_0x6f013930(*0x6f69ac30);                          // width  = 0.015625
iVar1 = func_0x6f37bd76(0xe8, "..\\UI/CCommandButton.cpp", 0x20c, 0); // 0xe8 = texture frame
func_0x6f5eeab0(*(param_1 + 0x194), 2, 1);             // number-overlay texture
func_0x6f5eec40(GetSkinTexture("CommandButtonNumberOverlay"));
iVar1 = func_0x6f37bd76(0xc4, "..\\UI/CCommandButton.cpp", 0x20f, 0); // 0xc4 = text frame
func_0x6f5ef1b0(*(param_1 + 0x194), 2, 1);
func_0x6f5ef350(GetFont("MasterFont", <size>));        // 0x6f0ff570
func_0x6f013520(4, *(param_1 + 0x194), 4, 0, 0xba1d4952 /* -0.000061 */, 1);
*(param_1 + 0x198) = <text frame>;
```

Child frames owned by a command button and their creation sites:

| member | site | class (alloc size) | role |
|---|---|---|---|
| `+0x194` | `0x6f12c2b0` | `0x124` | button frame itself (vtable `0x6f69b780`-family) |
| `+0x198` | `0x6f12c2b0` | `0xc4` | **the only text frame**, font `MasterFont`, textured with `CommandButtonNumberOverlay` |
| `+0x19c`/`+0x1a0` | `0x6f12c3f0` | `0x1b0` + `0x128` | cooldown swatch (`CommandButtonCooldown`, line `0x218/0x21c`) |
| `+0x1a8`/`+0x1ac` | `0x6f12c4d0` | `0x1b0` + `0x128` | autocast border (`CommandButtonAutocast`, line `0x227/0x22b`) |
| `+0x1b0` | `0x6f12c230` | `0xe8` | active highlight (`CommandButtonActiveHighlight`, line `0x1fc`, created lazily) |

Art keys proven present as strings and consumed by these functions:
`CommandButtonNumber` (`0x6f76dd3c`), `CommandButtonNumberOverlay` (`0x6f76dd50`),
`CommandButtonCooldown` (`0x6f76dd6c`), `CommandButtonAutocast` (`0x6f76dd84`),
`CommandButtonActiveHighlight` (`0x6f76dd10`), `CommandButtonMDL` (`0x6f76ce04`),
`CommandButtonDisabledArtPath` (`0x6f76ce28`), `MasterFont` (`0x6f76dd30`),
`ReplaceableTextures\CommandButtons\BTNTemp.blp` (`0x6f69a394`),
disabled art built by `%sDIS%s` (`0x6f76ce20`, used at `0x6f114f00` / `0x6f114fc0`).

### 1.2 "Does WC3 print icon + hotkey + cost badge on the button?" — verdict

* **icon**: yes — one texture frame per button (`0x6f12c230` / ctor), disabled variant via
  `%sDIS%s` (path rewrite, e.g. `BTNFoo.blp` → `BTNFooDIS.blp`).
* **hotkey glyph**: yes — one `CTextFrame` with `MasterFont` (`CommandButtonNumber`), anchored
  to the tile's own corner `4` with a `-0.000061` x nudge (`0x6f12c3d3`). Observed as code.
* **cost badge**: **no cost text frame exists in `CCommandButton`.** Gold/lumber/supply/mana
  icons exist only as tooltip resources (`ToolTipGoldIcon` `0x6f76d0a8`, `ToolTipLumberIcon`
  `0x6f76d094`, `ToolTipSupplyIcon` `0x6f76d080`, `ToolTipManaIcon` `0x6f76d0b8`) and the cost
  string is `ToolTipCost` (`0x6f76fdcc`), composed by `CToolTipWar3` (see §3). Holding a
  modifier to swap the hotkey glyph for the cost is **not proven** by this map (no code path
  found that writes a numeric string into `+0x198`; mark `unknown`).

### 1.3 Geometry in pixels (derived — arithmetic on the unit space)

`0.6` units = screen height ⇒ `1.0 unit = height/0.6` px.

| quantity | units | 1024×768 | 1920×1080 |
|---|---|---|---|
| button | 0.038 | 48.6 px | **68.4 px** |
| gap | 0.0075 | 9.6 px | 13.5 px |
| pitch | 0.0455 | 58.2 px | **81.9 px** |
| bar | 0.1745 × 0.129 | 223 × 165 | 314 × 232 |

### 1.4 MATCH / DIFF / MISSING vs our engine

| # | item | our side | engine | verdict |
|---|---|---|---|---|
| 1 | grid 4 × 3, 12 slots, row-major | `crates/wc3-game/src/ui/card.rs:3-4`, `101-110` | `0x6f12ae80` (`0xc`, 4×3 loop) | **MATCH** |
| 2 | tile size / gap | no constant; `64.0` in `mobile_hud.rs:4527` (test) | `0.038` / `0.0075` (`0x6f69ac20`, `0x6f699c40`) | **DIFF** (64 px vs 68.4 px @1080p, no gap constant) |
| 3 | per-tile text | `hud.rs:1029-1085` `card_label()` returns the **full label** | one `CTextFrame`, hotkey glyph only | **DIFF** (contract violation — root cause of the overflow) |
| 4 | overlays (cooldown/autocast/highlight) | not modelled as separate frames | 3 texture frames per button | **MISSING** |
| 5 | disabled art `%sDIS%s` | not modelled | `0x6f76ce20` → `0x6f114f00` | **MISSING** |
| 6 | cost badge on tile | — | does not exist in the engine | **refuted** (do not add one) |

---

## 2. Fonts

### 2.1 Observed

* Exactly **one** `.ttf` literal in the image: `FRIZQT__.TTF` at `0x6f753ee4`. Two users:
  `0x6f014570` and `0x6f03d900`.
* `0x6f014570` is the default-frame font getter (`CScreenFrame`):

```asm
0x6f0145f2  push 0x104                       ; 260-byte face buffer
0x6f0145f7  lea edx, [var_104h]
0x6f0145fd  mov ecx, str.FRIZQT__.TTF        ; "FRIZQT__.TTF"
0x6f014602  call 0x6f00ac3a                  ; copy/format face name
0x6f01460d  push 0
0x6f01460f  push 0x3c75c28f                  ; 0.015f  <-- rasterisation height
0x6f014614  lea eax, [var_104h]
0x6f01461a  push eax
0x6f01461b  call 0x6f00ca30                  ; font create(face, height, flags)
```

* Named fonts are FDF aliases resolved by `0x6f0ff570(name, size)`; the sizes are supplied by
  the FDF `Font "Name",size` / `FrameFont "Name",size` statements. Observed range for
  `MasterFont` across the shipped FDFs: `0.008 … 0.015`; `InfoPanelTextFont` `0.0085 … 0.013`
  (`analysis/war3_data/UI/FrameDef/UI/*.fdf`, e.g. `ResourceBar.fdf:8`,
  `SimpleInfoPanel.fdf:5,69`, `UpperButtonBar.fdf:32`).
* Sizes are **fractions of screen height** (`0.015` ⇒ 27 px @1080p, 19.2 px @768p) — the same
  unit space as the frame geometry, so text scales with the UI, not with a fixed px table.
* Text corners are per-instance 3-corner lists (`0x6f5ef590`, `0x6f5ef4c0`); the tooltip builds
  `TOPLEFT/TOP/TOPRIGHT` sets (see §3).
* `TextLength` (`0x6f764a94`) and `WrapWidth`/`WrapHeight` (`0x6f78915c/0x6f789168`) exist as
  table keys; **no direct code reference to `TextLength` was found** — the FDF handler table
  consumes it, so the exact truncation rule is **UNKNOWN** from this pass (a `getter`-based
  handler table would need to be walked to close it).

### 2.2 MATCH / DIFF / MISSING

| # | item | our side | engine | verdict |
|---|---|---|---|---|
| 1 | face list | (our renderer picks system/HUD fonts) | single `FRIZQT__.TTF` | **DIFF** |
| 2 | size unit = screen-height fraction | unknown/px-based | fraction (`0.015` default, FDF per context) | **DIFF** (should scale, not fix px) |
| 3 | default frame font height | — | `0.015` (`0x3c75c28f`) | **MISSING** constant |
| 4 | MasterFont usage for the tile glyph | uses card font | `0x6f0ff570("MasterFont", ...)` | **MISSING** |
| 5 | measurement + truncation | — | `TextLength` handler not decoded | **MISSING / unknown** |

---

## 3. Tooltips

### 3.1 Composition (observed, `CToolTipWar3` ctor `0x6f154a20`, `..\UI/CToolTipWar3.cpp`)

```c
in_ECX[0x21] = 8;                                   // 0x6f154b?? : up to 8 lines
in_ECX[0x49] = parent;
in_ECX[0x4a] = new CTextFrame(0xc4, line 0x3e);  SetFont(0x6f0ff570(...));  // NAME
in_ECX[0x4b] = new CTextFrame(0xc4, line 0x4a);  ...                        // DESC
for (i = 0; i < 8; i++) {                                                   // 8 lines
    in_ECX[i + 0x50] = new texture frame(0xe8, line 0x5a);                  // icon
    in_ECX[i + 0x4c] = new CTextFrame(0xc4, line 0x73);                     // text
    if (i == 0) { frame = in_ECX[0x4a]; anchor = 6; x = 0;               y = -0.004; }
    else        { frame = in_ECX[i+0x4b]; anchor = 5; x = 0.009375;       y = 0; }
    func_0x6f013520(anchor-args...);
}
```

Resource strings consumed by name:

| key | va | meaning |
|---|---|---|
| `ToolTipName` | `0x6f76fde4` | title line |
| `ToolTipDesc` | `0x6f76fdd8` | description block |
| `ToolTipCost` | `0x6f76fdcc` | cost line |
| `ToolTipBackground` / `ToolTipBorder` | `0x6f76db94` / `0x6f76dba8` | chrome |
| `ToolTipGoldIcon` / `ToolTipLumberIcon` / `ToolTipSupplyIcon` / `ToolTipManaIcon` | `0x6f76d0a8/94/80/b8` | cost glyphs |
| `REQUIRESTOOLTIP` | `0x6f76c280` | requirement header (`0x6f10efe0`, `0x6f112b00`, `0x6f112c30`) |
| `REQUIREDLEVELTOOLTIP`, `OUTOFSTOCKTOOLTIP`, `COOLDOWNSTOCKTOOLTIP` | `0x6f7a1fd4`, `0x6f79f688`, `0x6f79f69c` | ability-state variants |

Composition order (observed by frame creation order and by the strings' roles):
**name → description → cost/requirement lines (≤ 8)**. Padding `0.005` (`0x6f69abb4`),
tooltip backdrop insets `0.01` and `0.0019` (`0x3c23d70a`, `0x3af9096c` in the ctor).

### 3.2 Requirement-failure path

`Requirescount` (`0x6f797d88`) and `Requires%s` (`0x6f797d98`):

```c
uint fcn.6f3a7020(int32_t count) {                    // 0x6f3a7020
    fcn.6f3a19c0();
    iVar1 = fcn.6f3b5310(&buf, 0x6f797d88 /* "Requirescount" */, 0);
    if (count < iVar1) { fcn.6f3a7090(0x104); return fcn.6f3b5210(&buf, &out); }
    return 0;
}
void fcn.6f3a7090(int32_t n) {                        // 0x6f3a7090
    if (0 < n) SStrPrintf(&dst, 10, 0x6f797da4 /* "" */);   // Storm Ordinal_578
}
```

Also referenced from `0x6f3ad070` (264 B) and `0x6f3af300` (799 B) — the ability/upgrade tooltip
builders. So the engine answers "requirements not met" by counting `Requires*` cells in the
ability row and emitting one `Requires%s` line per unmet requirement; there is no separate
"sorry, you cannot" string.

### 3.3 Colours / escape codes

* `|c%2.2x%2.2x%2.2x%2.2x%s|r` — `0x6f76922c` (ARGB). Variants at `0x6f769254`,
  `0x6f769f7c` (`%s|c…%s: |r%s`), `0x6f76a024`.
* Literals `|cffff0000` (red) `0x6f76d7f0`, `|cff00ff00` (green) `0x6f76d7fc`.
* Gold tooltip title `|cfffed312` — `0x6f76e7ec` / `0x6f76e814` (`%s (|cfffed312F%d|r)`).
* FDF text templates carry the actual colours, e.g.
  `SimpleInfoPanel.fdf:5-8` `FontColor 0.99 0.827 0.0705` = `FED312`, shadow
  `0.0 0.0 0.0 0.9`, `FontShadowOffset 0.002 -0.002`.

### 3.4 MATCH / DIFF / MISSING

| # | item | our side | engine | verdict |
|---|---|---|---|---|
| 1 | name/desc/cost composition | tooltip exists | name → desc → ≤8 cost/requirement lines | **MATCH** (order) |
| 2 | line cap | unbounded | 8 (`in_ECX[0x21] = 8`) | **MISSING** cap |
| 3 | requirement failure | — | `Requirescount` + `Requires%s` per unmet cell | **MISSING** |
| 4 | escape codes | parser handles `|cAARRGGBB`/`|r` (`hud.rs:904`) | same | **MATCH** |
| 5 | wrap width | fixed | FDF/frame-driven; exact rule unknown | **MISSING / unknown** |

---

## 4. Audio distance model

### 4.1 SoundData schema and parser defaults (observed)

Parser **`0x6f3a2530`** (`UI\SoundInfo\*` tables, 727 B), quoted verbatim:

```c
uVar2 = func_0x6f3a2810("Volume", 0x7f);              // 127
fVar8 = func_0x6f3a2870("Pitch", 0x3f800000);         // 1.0
fVar8 = func_0x6f3a2870("PitchVariance", 0);          // 0.0
uVar2 = func_0x6f3a2810("Priority", 8);               // 8
uVar2 = func_0x6f3a2810("Channel", 0);                // 0
fVar8 = func_0x6f3a2870("MinDistance", 0);            // 0.0
fVar8 = func_0x6f3a2870("MaxDistance", 0x461c4000);   // 10000.0
fVar8 = func_0x6f3a2870("DistanceCutoff", 0x453b8000);// 3000.0
fVar8 = func_0x6f3a2870("InsideAngle", 0);
fVar8 = func_0x6f3a2870("OutsideAngle", 0);
uVar2 = func_0x6f3a2810("OutsideVolume", 0x7f);       // 127
fVar8 = func_0x6f3a2870("OrientationX", 0);           // …Y, …Z likewise
if (puVar6[0x34] == 0.0f) puVar6[0x34] = 1.0f;        // pitch sentinel fix-up
```

Struct slots are `puVar6[0x33..0x40]` ⇒ byte offsets `0xcc (Volume) … 0x100 (OrientationZ)`.
Field order matches the shipped SLK columns exactly
(`analysis/war3_data/UI/SoundInfo/UnitAckSounds.slk` header: `SoundName, FileNames,
DirectoryBase, Volume, Pitch, PitchVariance, Priority, Channel, Flags, MinDistance,
MaxDistance, DistanceCutoff, InsideAngle, OutsideAngle, OutsideVolume, OrientationX/Y/Z,
EAXFlags`).

### 4.2 Play-instance defaults (observed, `0x6f10a5a0`, 125 B)

```
+0x2c = 0x7f (127)     volume          +0x44 = 1000.0     MinDistance
+0x30 = 1.0            pitch           +0x48 = 10000.0    MaxDistance
+0x38 = 100            priority        +0x4c = 1000.0     DistanceCutoff
+0x5c = 0x7f (127)     OutsideVolume   +0x50 = 1000000.0  DistanceCutoff² (and "max range")
+0x70 = -1             channel id
```

`SetSoundDistanceCutoff` (`(Hsound;R)V`, handler `0x6f2c8b30` → `fcn.6f1048e0`) keeps the
squared copy in sync — observed at `0x6f10493c`:

```asm
0x6f104931  fld  dword [eax]          ; new cutoff
0x6f104939  fst  dword [edx + 0x4c]
0x6f104942  fld  st(0)
0x6f104944  fmul st(1)                ; cutoff * cutoff
0x6f104946  fstp dword [eax + 0x50]
```

### 4.3 The hard audibility gate — `CSoundManager::StartSound` (`0x6f102980`)

```c
if ((flags3d & 1) && (*(iVar6 + 0x50) != 0.0f) && (pos != NULL && (*pos & 2))) {
    fVar4 = (lx - px)² + (ly - py)² + (lz - pz)²;      // squared distance, no sqrt
    if (*(iVar6 + 0x50) < fVar4) {                     // cutoff² < dist²
        log("sound cutoff at %g, current distance at %g, aborting", fVar4, *(iVar6 + 0x50));
        return 4;                                      // sound never starts
    }
}
```

Disassembly (`0x6f102ac9` listener loads … `0x6f102b37` abort):

```asm
0x6f102ac9  mov  ecx, dword [0x6f7d6f64]      ; listener X
0x6f102af5  fld  st(0) / fmul st(1) …         ; dx²+dy²+dz²
0x6f102b0b  fcom dword [ebx + 0x50]           ; dist²  ?  cutoff²
0x6f102b10  test ah, 0x41
0x6f102b13  jne  0x6f102b45                   ; dist² <= cutoff²  -> keep playing
0x6f102b15  fld  dword [ebx + 0x50]
0x6f102b29  push str.sound_cutoff_at__g__current_distance_at__g__aborting
0x6f102b31  call dword [ecx + 0xc]            ; log, then return 4
```

(Note: the log string's two `%g` slots print `dist²` first and `cutoff²` second, i.e. the
message text reads backwards relative to the numbers — a retail log quirk, not a decode error.)
Consequences:

* **`DistanceCutoff` is the audibility radius**, not `MaxDistance`, and the comparison is done
  in squared space (no `sqrt`, no interpolation at the gate).
* The gate only applies to 3-D sounds (`flags & 1`) with a non-zero cutoff and a valid listener
  position (`*pos & 2`) — 2-D UI sounds are never distance-culled.
* Additional state gates observed in the same function: `0x2000` flag requires game state `5`
  (`*0x6f7d73b4`), `0x40000`/`0x20000` flags gate music/ambience, and
  `*0x6f76a570 & 8 == 0` aborts with `"master sounds disabled, aborting"`.

### 4.4 Near/far interpolation and the occlusion curve

`fcn.6f10c880` (the per-voice mixer parameter builder) shows the only distance→gain arithmetic
in `Game.dll`:

```c
fVar6 = *(param_2 + 0x44);                                   // MinDistance == "cutoff radius" here
fVar8 = SQRT(dx*dx + dy*dy + dz*dz);
if (fVar6 == 0.0f || fVar8 < fVar6)            pan = 0.0f;                       // inside: unity
else if ((fVar8 - fVar6) < fVar6)              pan = C - occlusion();            // ramp zone
else                                           pan = ((fVar8 - fVar6) / fVar6) * maxAtten;
```

guarded by `(play.flags & 0x4000)` **and** `sound provider index < 3` (`*0x6f76a574`) **and**
`(*param_1 & 2)` — i.e. it is the EAX/occlusion path, not the plain near/far path.

**Confirmed negative:** `Game.dll` does **not** import `mss32`/`AIL_*` (`rabin2 -i` → 0 matches
for `mss|ail` in both `Game.dll` and `Storm.dll`), and no mixture curve constant survives in
`Game.dll`. Near/far radii travel to the Storm/Miles mixer in a 0x104-byte parameter block
(`0x6f10a3d0`, flags `0x20` volume / `0x60` pitch / `0x80` pan / `0x100` extras, fields
`param[0xf]=volume, [0x10]=pitch, [0x11]=pan, [0x12]=priority, [0x13]=channel`), and the
rolloff curve itself is the Miles 3-D provider's (`_AIL_3D_sample_distances@12`,
`_AIL_3D_rolloff_factor@4`, `_AIL_3D_distance_factor@4` in `Mss32.dll` exports). That last step
is **inferred** (dynamic `GetProcAddress`; not provable from static imports).

### 4.5 Constants table (retail data, observed from the shipped SLKs)

| table | rows | MinDistance | MaxDistance | **DistanceCutoff** | PitchVariance | flags |
|---|---:|---:|---:|---:|---|---|
| `UnitAckSounds` | 739 | 3000 | 10000 | **100000** | 0.01 | WANT3D, NODUPEUSERNAMES, CHANNELFULLPREEMPT, RANDOMPITCH |
| `UnitCombatSounds` | 60 | 600 | 10000 | **2100** | 0.05 / 0.1 | WANT3D, RANDOMPITCH, DYNAMICOCCLUSION |
| `AmbienceSounds` | 60 | 100–600 | 5000–10000 | **900–2500** | — | WANT3D, LOOPING, NODUPLICATES, CHANNELFULLPREEMPT, SCALEPRIORITY |
| `AbilitySounds` | 33 | 600–900 | 100000 | **2000** | — | WANT3D, IGNOREUSERNAME, CHANNELFULLPREEMPT |
| `UISounds` | 111 | 0–600 | 0–10000 | **2000–10000** | — | NODUPLICATES, CHANNELFULLPREEMPT, WANT3D |

### 4.6 MATCH / DIFF / MISSING vs our engine

| # | item | our side | engine | verdict |
|---|---|---|---|---|
| 1 | hard audibility gate | `DEFAULT_AUDIBLE_RADIUS = 4000.0` (`crates/wc3-audio/src/game_audio.rs:71`) | `cutoff² < dist² → abort` per sound (`0x6f102980`) | **DIFF (priority)** — wrong quantity *and* wrong scope |
| 2 | near radius | `min_distance` fallback `MAX_DISTANCE` (`soundinfo.rs:60-92`) | `MinDistance`, default `0` from SoundData, `1000` instance default | **DIFF** — our fallback ignores the instance default 1000 |
| 3 | far radius | `max_distance` (`soundinfo.rs:82-91`) | `MaxDistance` (default 10000) | **MATCH** |
| 4 | curve inside near→far | `10^(-3t)` (kira −60 dB) `engine.rs:955-958` | not in `Game.dll` (Miles provider) | **unknown** — no evidence either way |
| 5 | gate only for 3-D sounds | applied to all positioned sounds | `flags & 1` (3D) + valid listener | **DIFF** |
| 6 | occlusion mix (`DYNAMICOCCLUSION`) | `1 - 0.45*occlusion` (`engine.rs:932`) | `0x4000` flag + provider<3 + sqrt(d²) ramp (`0x6f10c880`) | **DIFF** |
| 7 | priority/channel routing | not modelled | `Priority` (8/100/1000), `Channel` 0–10, `CHANNELFULLPREEMPT` | **MISSING** |
| 8 | voice cap | — | not found in `Game.dll`; dynamic table (`0x6f7d6ad4`) | **MISSING / unknown** |

---

## 5. Sound selection (SoundSet → ack/death/ready) and pitch variance

### 5.1 The four indices that decide which line plays

`0x6f3a2530` opens the table selected by its first argument through a static pointer array at
`0x6f6b4e30` (observed, 14 entries):

```
[0] UI\SoundInfo\UnitAckSounds      [7] "What"        [11] "Ready"
[1] UI\SoundInfo\UnitCombatSounds    [8] "Pissed"      [12] "Warcry"
[2] UI\SoundInfo\UISounds           [9] "YesAttack"   [13] "MusicFiles"
[3] UI\SoundInfo\AmbienceSounds    [10] "Yes"
[4] UI\SoundInfo\AnimSounds
[5] UI\SoundInfo\AbilitySounds
[6] UI\SoundInfo\DialogSounds
```

So the engine's selection model is:

1. a unit carries a sound-set name (`unitSound` in `Units\UnitUI.slk`);
2. the label is the **row key** in `UI\SoundInfo\UnitAckSounds.slk` (739 rows, first column
   `SoundName`, e.g. `ArthasWhat`, `ArthasPissed`, `ArthasYes`, `ArthasWarcry`);
3. the suffix selects the occasion — `What` (click/select ack), `Yes` (order ack),
   `YesAttack` (attack-order ack), `Pissed` (repeat-click annoyance), `Ready` (finished
   training/created), `Warcry`;
4. death lines are **not** in `UnitAckSounds`: `%sDeath` (`0x6f797c90`) is referenced from
   `0x6f3a5f43` and the `AnimSounds` table (`UI\SoundInfo\AnimSounds`, 409 rows) plus the
   per-tileset `Death<Race><Size>Building` keys (`0x6f797c98..0x6f797d88`) supply the building
   deaths. `UNITSELECTSOUNDTIMER` (RTTI string `0x6f76cfc4`) holds the repeat-click counter that
   drives `Pissed`.
5. `RandomSoundLabel` / `LoopingSoundFadeIn` / `LoopingSoundFadeOut` / `BuildingSoundLabel` /
   `MovementSoundLabel` (`0x6f76a778..0x6f76a7c8`) are the `UnitData`/`UnitUI` hooks that
   choose among the labels.

Every label resolves to a `SoundData` row with `FileNames` (comma-separated variants — the file
picker is `0x6f10a3d0` and, for `Flags & 0x400`, adds `+ *(iVar6+0x70)` as a random index offset
inside `0x6f102980`).

### 5.2 Pitch variance

* `PitchVariance` is parsed into `0xd4` (default `0.0`) by `0x6f3a2530`.
* The gate is the **`RANDOMPITCH`** flag (`0x6f76b19c`), present on `UnitAckSounds`
  (`0.01`) and `UnitCombatSounds` (`0.05`/`0.1`) and **absent** from `UISounds` /
  `AmbienceSounds` (their `PitchVariance` columns hold `1` and are inert).
* Application point: the start path (`0x6f102980`) passes a pitch value to the mixer
  (`func_0x6f5c8560(paramBlock, …, volume, pan, pitch)`), and `0x6f10c880` forwards it as
  `param[0x10]` when the player pitch differs from `0.0`.
* **Exact formula: UNKNOWN.** No `±variance` arithmetic was located; the shipped magnitudes
  (1 %, 5 %, 10 %) are consistent with "variance is a fraction of the base pitch", which is what
  we should implement, but that is `inferred`, not observed.

### 5.3 MATCH / DIFF / MISSING

| # | item | our side | engine | verdict |
|---|---|---|---|---|
| 1 | `unitSound` → ack row mapping | `wc3-audio` ambient/soundinfo parsers | row key in `UnitAckSounds`, suffix selects occasion | **MATCH** (structure) |
| 2 | occasion set | What/Yes/YesAttack/Pissed/Ready/Warcry | identical six (`0x6f6b4e30[7..12]`) | **MATCH** |
| 3 | death path | `%sDeath` rows in AnimSounds | `%sDeath` (`0x6f797c90`) + tileset building keys | **MATCH** |
| 4 | repeat-click → `Pissed` timer | — | `UNITSELECTSOUNDTIMER` list | **MISSING** |
| 5 | pitch variance | — | `PitchVariance` gated by `RANDOMPITCH` | **MISSING** |
| 6 | filename randomisation | round-robin/random | `FileNames` list + `0x400` index offset | **DIFF** |

---

## 6. Evidence index (address → claim)

| address | claim |
|---|---|
| `0x6f12ae80` | CommandBar ctor: 12 buttons, 4×3 loop, bar = `(0.0075+0.038)*n − 0.0075` |
| `0x6f5ebf90` | PlaceButton(col,row,frame,1) using per-cell rects at `+0x124..0x138` |
| `0x6f013930` / `0x6f013950` | width (`+0x58`) / height (`+0x5c`) setters |
| `0x6f12c2b0` | CommandButton ctor: default 1/64 size, NumberOverlay texture, MasterFont text frame |
| `0x6f12c230/3f0/4d0` | highlight / cooldown / autocast child frames |
| `0x6f114f00`, `0x6f114fc0` | `%sDIS%s` disabled-art path builder |
| `0x6f014570` | default font `FRIZQT__.TTF` at height `0.015` |
| `0x6f753ee4` | the only `.ttf` literal in the image |
| `0x6f0ff570` | font lookup by name (`MasterFont`) |
| `0x6f154a20` | CToolTipWar3 ctor: 8 lines, name/desc frames, padding 0.005 |
| `0x6f155b90` | CUnitTip (unit hover) reuses ToolTipBackground/Border |
| `0x6f3a7020` / `0x6f3a7090` | `Requirescount` / `Requires%s` requirement path |
| `0x6f76922c` | `|c%2.2x%2.2x%2.2x%2.2x%s|r` colour escape |
| `0x6f3a2530` | SoundData parser + all defaults (127, 1.0, 0, 8, 0, 0, 10000, 3000, 127) |
| `0x6f10a5a0` | play-instance defaults 1000/10000/1000, cutoff² = 1e6 |
| `0x6f1048e0` | SetSoundDistanceCutoff keeps `cutoff²` at `+0x50` |
| `0x6f102980` | StartSound: squared-distance cutoff gate + state gates |
| `0x6f10c880` | mixer param builder: volume/pitch/pan, occlusion ramp `((d−c)/c)·max` |
| `0x6f10a3d0` | 0x104-byte mixer parameter block assembly |
| `0x6f6b4e30` | SoundDB table pointer array + `What/Pissed/YesAttack/Yes/Ready/Warcry` |
| `0x6f29de90..0x6f29dfd0` | JASS sound native registration (`0x6f62f140`) |
| `0x6f797c90` | `%sDeath` label builder |
| `0x6f76b19c` | `RANDOMPITCH` flag name |

---

## 7. Actionable consequences (report only — no code changed here)

1. **Command card text contract (root cause of the overflow).** `card_label()` in
   `crates/wc3-game/src/hud.rs:1029-1085` deliberately falls back to the *full* label and the HUD
   draws it inside the tile. The engine's tile has room for exactly one glyph string
   (`CommandButtonNumber`, `MasterFont`) — the hotkey. Recommendation: draw `hotkey` only in the
   tile; move `label`, `cost` and requirements to the tooltip builder. This is the contract
   fix; nothing about it depends on tile size.
2. **Tile geometry.** Replace the ad-hoc `64.0` with the engine's unit-space constants
   (`size 0.038`, `gap 0.0075`, `pitch 0.0455`, bar `0.1745 × 0.129`) scaled by
   `screenHeight / 0.6`. At 1080p that yields 68.4 px tiles on an 81.9 px pitch.
3. **Audio gate.** Delete the fixed `DEFAULT_AUDIBLE_RADIUS = 4000.0` as the audibility rule and
   gate on the row's `DistanceCutoff` when `WANT3D` is set (`cutoff == 0` ⇒ no gate), comparing
   `dist² > cutoff²`. Keep `MinDistance`/`MaxDistance` for the gain ramp. Expect combat sounds to
   become quieter sooner (2100 vs 4000) and ack lines to never be distance-culled (100000).
4. **Priority/channel.** `Priority` (8 default, 100 instance, 1000 in shipped tables) and
   `Channel` (0–10) with `CHANNELFULLPREEMPT` are the only eviction policy the engine has; our
   engine has none. Needed before the mixer can behave like retail under load.
5. **Pitch variance.** Apply `PitchVariance` only when `RANDOMPITCH` is set, as
   `pitch · (1 ± variance · u)`, `u ∈ [−1, 1]` — flagged as `inferred`; worth a follow-up pass to
   pin the exact expression.

## 8. Limitations of this pass (honest)

* `TextLength` truncation, tooltip wrap width, and the `MasterFont` size argument are
  table/handler-driven and were not decoded — marked `unknown`.
* The Miles rolloff curve is not in `Game.dll`; `Mss32.dll` loads dynamically, so the mapping
  `Game param block → AIL_3D_sample_distances` is `inferred` from the API surface, not proven.
* No fixed voice cap was found in `Game.dll`; the sound table is a dynamic array. The cap, if
  any, lives in the Miles mixer.
* RoC (`9970c5af…`) was surveyed for the audio parser/strings only; the command-card and tooltip
  addresses above are TFT (`d1a80c89…`). No behavioural divergence was observed in the shared
  string set, but a per-binary diff of the layout constant block was not run.

Raw disassembly for every quoted function: `analysis/agent-re/ui_audio/raw/<bin>.<addr>.dis.txt`;
helper: `analysis/agent-re/ui_audio/re_helper.py`; machine-readable numbers:
`analysis/agent-re/ui_audio/constants.json`.
