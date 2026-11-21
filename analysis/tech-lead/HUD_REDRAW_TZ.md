# ТЗ: редизайн HUD wc3rust под мобилу (Blizzard spirit)

Версия: 2026-11-13. Источник истины по размерам: `crates/wc3-game/src/mobile_hud.rs` +
`crates/wc3-game/src/hud.rs`. Референсы: `wc3-hud/`, `dragonui/`, `wc3rust-hud/` (эта папка).

## 0. Цель и рамки
Перерисовать весь HUD/UI так, чтобы на телефоне он читался как консоль Warcraft III: каменный
консольный бар, золотая окантовка, parchment-тултипы, крупные иконки, team-color акценты — но с
мобильной эргономикой (большие touch-зоны, safe area, без мелкого текста). Никакого «плоского
веб-UI» и никакого синего как основного цвета.

## 1. Дух и референсы
- База: WC3 RoC/TFT HUD (камень, металл, золото, пергамент, геральдика).
- Модерн: DragonUI (retail-inspired): чистые 9-slice панели, тонкие светлые кромки, мягкие
  градиенты, аккуратная типографика, единая сетка.
- Мобила: крупные силуэты, высокий контраст, читаемость на 5–7", thumb-friendly.
- Вдохновение: `wc3-hud/Console/Human`, `wc3-hud/Widgets/ButtonBorder`, `dragonui/Textures/UI`,
  `dragonui/Textures/UnitFrames`, `dragonui/Textures/Micromenu`, `dragonui/Textures/Castbar`.
- Запрещено: синий как доминирующий фон, мелкий текст <24 px (1080p ref), «стеклянные» панели,
  случайные шрифты, шумные текстуры под текстом.

## 2. Устройства, масштаб, safe area
- Референс-разрешение: 1920×1080 (landscape) и 1080×1920 (portrait); design height 768.
- Масштаб UI: `scale = min(vw,vh)/1080 * ui_scale`, ui_scale = 768/screen_h, clamp 0.40–1.15,
  итоговый clamp 0.62–2.0 (`mobile_hud.rs:16-22,767-770`).
- Safe area 1080p: landscape left/top/right/bottom = 48/80/48/48; portrait = 24/96/24/64
  (`mobile_hud.rs:90-96`). Консоль прижата к низу edge-to-edge (immersive, нижний inset = 0).
- Touch-зона: ≥48×48 dp; командная ячейка 64×64 при 1080p ≈ 9 мм — не уменьшать.
- Все размеры ниже — в px при 1080p reference; на устройстве умножаются на scale.

## 3. Карта экрана (landscape 1920×1080)
```
┌───────────────────────────────────────────────────────────────────────────┐
│ TOP BAR 32: map name (55% ширины, слева)            fps/timer (360, справа) │
│                                                                           │
│                         ИГРОВОЙ ВЬЮПОРТ (centered)                        │
│                                                                           │
├───────────────────────────────────────────────────────────────────────────┤
│ CONSOLE 288 (edge-to-edge, камень+металл, золотая кромка)                  │
│  pad 24 → ┌ minimap header 216×20 ┐  ┌ portrait 128×128 ┐  ┌ info 240 ┐  │
│           │ minimap 216×216       │  │ HP/mana ring     │  │ name 30  │  │
│           │ (заголовок + рамка)   │  │                  │  │ HP 24    │  │
│           └───────────────────────┘  └──────────────────┘  │ mana 16  │  │
│  resources shelf 720×32: [gold 32][lumber 32][food 32]      │ stats 24 │  │
│  upkeep 720×14 (под ресурсами)                              │ prod 6   │  │
│                                        ┌ command card 4×3 ┐ └──────────┘  │
│                                        │ 64×64, gap 4    │ 268×200       │
│                                        └─────────────────┘               │
└───────────────────────────────────────────────────────────────────────────┘
```
Точные прямоугольники (landscape, 1080p, из `mobile_hud.rs:788-900`):
- console: x=safe.left, y=H−288, w=W−left−right, h=288; inner inset 8×6; pad 24.
- minimap header: (console.x+24, console.y+20, 216×20); minimap: (console.x+24, console.y+48, 216×216).
- portrait: (console.x+264, console.y+40, 128×128).
- name: (portrait.x, console.y+2, 380×30); class: (portrait.x, console.y+32, 380×30).
- HP: (portrait.right+16, console.y+44, 240×24); mana: (…+72, 240×16); stats: (…+94, 240×24);
  production label: (…+120, 240×24); production bar: (…+148, 240×6).
- selection panel: (portrait.x, console.y, max(240, info_x+240−portrait.x), 180).
- command card: (console.right−24−268, console.y+64, 268×200); cell 64, gap 4; 12 слотов row-major.
- resources: (card.right−720, console.y+4, 720×32); upkeep: (…+38, 720×14); 3 пилюли: gap 6,
  ширина (720−12)/3 = 236.
- top bar: (safe.left, safe.top, safe_frame.w, 32); map name 55% ширины; fps (right−360, 360×32).

Portrait (1080×1920): та же консоль внизу (288), minimap/portrait/info слева, card справа
(resources width = card width), safe area 24/96/24/64. Проверить, что 216+128+240+268+24*2 < 1080.

## 4. Компоненты HUD (что рисуем и зачем)
1. **Console tiles (4 шт.)** — левый кап, левая середина, правая середина, правый кап; 9-slice,
   inset 32/48/32/48. Камень + металл + золотая кромка, team-color тонкая полоса. Почему: единая
   консоль без «плавающих виджетов».
2. **Minimap frame 216×216 + header 216×20** — рамка, угловые орнаменты, заголовок «Minimap»,
   состояния ping/alert. Почему: навигация и тактика.
3. **Portrait frame 128×128** — геральдическая рамка, HP/mana ring, фон под портрет. Почему:
   идентификация выделенного юнита/героя.
4. **Selection panel 240×180** — name plate (30), class (30), HP bar (24), mana bar (16), stats (24),
   production label (24) + bar (6). Почему: статы и производство в одном месте.
5. **Command card 4×3 (64×64, gap 4)** — ButtonBorder 9-slice (inset 12), состояния: normal,
   pressed (scale 0.96), disabled, cooldown (radial), autocast (glow), empty, hotkey-текст.
   Почему: все действия под большой палец.
6. **Resource pills 3× (236×32)** — gold/lumber/food иконки 32×32, фон-пилюля 9-slice, warning
   (мало еды/золота) — янтарный/красный. Почему: экономика всегда на виду.
7. **Upkeep strip 720×14** — 0/40/70/90 и 0/50/80/100 (RoC/TFT), подсветка порога.
8. **Top bar 32** — map name plate, fps/timer, разделители.
9. **Tooltip** — parchment/stone 9-slice (inset 24), заголовок, тело, hotkey-строка, стрелка.
10. **Cursor / selection / orders** — selection circle (unit/hero/enemy), target cursor,
    move/attack/patrol markers, rally point.
11. **Build menu** — те же 12 слотов, но с build-прогрессом и cancel.
12. **Control groups 1–0** — ряд из 10 кнопок (44×44), active/inactive, цифра/hotkey.
13. **Menus** — main menu, map select, loading, score screen, skirmish setup: каменные панели,
    золотые кнопки, parchment-подложки; без синего.
14. **Fonts** — primary (Friz Quadrata-подобный, Cyrillic), action bar (expressway/PTSansNarrow),
    цифры табличные. Минимум 24 px; line-height 1.25.

## 5. Полный список ассетов к перерисовке
Формат source: PNG-24 RGBA (straight alpha, sRGB, без color profile). Runtime: PNG/BLP1 (по пайплайну).
Мипы: иконки — да; 9-slice/панели — нет (во избежание bleed). Padding 2 px, extrude 2 px.

| # | Ассет | Размер (1080p) | 9-slice inset | Состояния |
|---|---|---|---|---|
| 1 | console_left_cap / console_left_mid / console_right_mid / console_right_cap | 256×288 каждый, tileable | 32/48/32/48 | normal |
| 2 | minimap_frame | 216×216 | 24 | normal, alert |
| 3 | minimap_header | 216×20 | 12/0 | normal |
| 4 | portrait_frame | 128×128 | 16 | normal, hero, enemy |
| 5 | selection_panel | 240×180 | 24 | normal |
| 6 | button_border | 64×64 | 12 | normal/pressed/disabled/cooldown/autocast/empty |
| 7 | button_glow / cooldown_radial | 64×64 | — | overlay |
| 8 | resource_pill | 236×32 | 16/8 | normal, warn |
| 9 | icon_gold / icon_lumber / icon_food | 32×32 | — | normal, warn |
| 10 | upkeep_strip | 720×14 | 8/0 | 0/40/70/90, 0/50/80/100 |
| 11 | topbar_plate | 720×32 | 16/0 | normal |
| 12 | tooltip_panel | 384×256 | 24 | normal |
| 13 | tooltip_arrow | 24×12 | — | up/down |
| 14 | selection_circle_unit/hero/enemy | 96×96 | — | animated (pulse) |
| 15 | cursor_target / cursor_move / cursor_attack | 48×48 | — | normal |
| 16 | rally_point / order_marker | 32×32 | — | normal |
| 17 | build_progress | 64×8 | 4 | normal |
| 18 | control_group_button | 44×44 | 8 | active/inactive |
| 19 | menu_panel / menu_button | 512×384 / 256×64 | 24 / 16 | normal/hover/pressed/disabled |
| 20 | loading_frame / score_panel | 1024×512 | 32 | normal |
| 21 | commandbuttons atlas | 2048×2048 | — | 12 slots × N pages |
| 22 | fonts: primary/actionbar | TTF | — | Cyrillic + Latin |

## 6. Стиль, материалы, палитра, типографика
- Материалы: тёмный камень (гранит/базальт), кованый металл (тёмная сталь), золото/латунь,
  пергамент, тиснёная кожа. Свет: верхний-левый, мягкие блики, глубокая тень.
- Палитра (ориентиры): камень #2A2622/#3B342D, металл #5A5148/#8A7A66, золото #C9A227/#F0D060,
  пергамент #E8D9B5/#C9B48A, team colors red #C0392B, blue #2E6FB0 (только акцент), teal #1F8A8A,
  purple #6C3FA0, warning #E0A030, danger #C03030. Синий не должен быть фоном HUD.
- Типографика: заголовки — антиква с засечками (Friz-подобная), числа — табличные, body — читаемый
  sans. Минимум 24 px, контраст ≥4.5:1, обводка 1–2 px тёмным для читаемости на камне.
- Иконки: painterly, сильный силуэт, тёмная обводка, тёплый свет; читаемы в 32×32 и 64×64.
- Кромки: 2 px светлая + 1 px тёмная (bevel), без «неоновых» свечений.

## 7. Технические требования
- Атлас: ≤2048×2048, power-of-two, padding 4 px, extrude 2 px, без rotation; HUD-атлас ≤64 MB VRAM.
- UV: целые пиксели, без bleeding; 9-slice режется по inset из manifest.
- Масштабирование: 9-slice тянется только центром; углы фиксированы; текст не растягивать.
- Safe area: все интерактивные элементы внутри; консоль edge-to-edge по низу.
- Touch: ≥48 dp; gap между кнопками ≥4 px; без двойных тапов.
- Читаемость: min 24 px; числа не «прыгают» (tabular-nums); warning-состояния дублируются цветом+иконкой.
- Производительность: один draw call на атлас/бленд-стрим; без полноэкранных оверлеев.
- Форматы: source PSD/Krita (слои, маски), export PNG RGBA; имена snake_case; manifest.json рядом.

## 8. Анимации и состояния
- Кнопка: press scale 0.96 + затемнение 8%; disabled — desaturate 60% + alpha 0.5; cooldown —
  радиальная маска по часовой; autocast — тёплое свечение рамки.
- Low resource: пульс янтарный 1.2 s; critical — красный.
- Selection circle: пульс 0.8 s, team color; hero — золотой орнамент.
- Build progress: заполнение слева-направо, без «скачков».
- Tooltip: fade in 120 ms, без резких теней.

## 9. Меню/загрузка/скирмиш
Main menu: каменная панель, золотые кнопки, фон — карта/арт; Map select: список карт, превью,
сложность; Loading: рамка + прогресс + подсказки; Score: таблица, MVP, ресурсы; Skirmish:
раса/противник/сложность. Везде тот же камень/золото/пергамент, без синего.

## 10. Поставка (структура)
```
hud-redraw/
  console/ minimap/ portrait/ selection/ commandcard/ resources/ topbar/ tooltip/
  cursor/ build/ controlgroups/ menus/ icons/ fonts/
  manifest.json   # name, size, insets, states, atlas group, format
  README.md       # провенанс и как экспортировать
```
manifest.json: для каждого файла — `name`, `w`, `h`, `insets{l,t,r,b}`, `states[]`, `atlas`,
`format`, `mips` (bool). Имена: `<component>_<state>_<size>.png`.

## 11. Acceptance criteria (проверяемые)
1. Все 22 позиции из §5 присутствуют; manifest валиден; имена по схеме.
2. 9-slice: при растяжении 0.62×–2.0× нет артефактов, углы не мылятся.
3. Текст ≥24 px на 1080p, контраст ≥4.5:1 (замерить на скриншоте).
4. Touch-зоны ≥48 dp; командная ячейка 64×64; control groups 44×44.
5. Safe area: ничего не обрезается в landscape/portrait на 1080p и на устройстве.
6. Нет синего фона HUD; team colors и warning-состояния читаемы.
7. Атлас ≤2048², ≤64 MB VRAM; один draw call на бленд-стрим.
8. Device-скриншот: консоль, миникарта, портрет, карта команд, ресурсы, тултип — совпадают с картой §3.
9. Blizzard-дух: камень/металл/золото/пергамент; визуально сравнимо с `wc3-hud/Console/Human`.

## 12. Приложение: текущие константы кода
- `mobile_hud.rs:16` REFERENCE_HEIGHT=1080; `:20` DRAGONUI_DESIGN_HEIGHT=768; `:21-22` UI_SCALE 0.40–1.15; `:26` CONSOLE_HEIGHT=288; `:90-96` safe areas; `:61-64` hotkey font scale; `:767-770` scale; `:788-900` layout rects.
- `hud.rs:282` MIN_TEXT_PX=24; `:439` line_height=1.25; `:556` 18 px secondary.
- `assets.rs:39/46` TownHall model; `:410` UNIT_IDS; `:728` roster ids.
- `sim_bridge.rs:114/129` map ground plane (x,y) — не путать с z (высота).
