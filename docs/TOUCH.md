# TOUCH — touch-first UX для RTS на Android

Цель: игрок проходит матч **пальцем**, без мыши/клавиатуры. Мышь/клавиатура/S Pen — усиление,
не обязательное условие. **Источник событий — SDL3** (`SDL_EVENT_FINGER_*`, `SDL_EVENT_PEN_*`,
mouse/keyboard/gamepad), затем наш translator. Основа: W3AndroidEngine `input/touch/*` + Generals
Android touch doc (см. `RESEARCH.md` T1/T4), переосмысленные под Rust/SDL3.

## 1. Принципы

1. **Deferred commitment:** палец, коснувшийся экрана, ещё не клик. Tap/drag/long-press
   распознаются после порога; случайный клик не должен отдавать приказ.
2. **Hover-before-click:** для UI, где важен hover (тултипы, подсветка), сначала synthetic
   mouse move в точку, затем click; но игровые приказы не должны требовать hover.
3. **DPI-scaled thresholds:** пороги в dp/мм, не в пикселях; учитывать плотность и размер экрана.
4. **Mode locking:** two-finger pan XOR pinch на жест; после победы одного режима второй не
   переключается до поднятия пальцев.
5. **One source of mouse events:** либо системная touch→mouse эмуляция, либо наш translator,
   не оба; иначе двойные клики.
6. **Cancelled touches never click:** CANCEL/уход пальца за экран/палм — без клика.
7. **Long-press — явный, с haptic:** 500–600 мс, stationary; показывает, что распознан.
8. **Undo/confirm для разрушительных приказов:** продажа/отмена строительства — confirm или
   короткое окно отмены.
9. **Крупные цели:** минимум 44×44 dp для кнопок; command card — 48–56 dp.
10. **Никаких hover-only функций:** каждый тултип/статус доступен tap/long-press.

## 2. Экранные зоны (landscape)

```
┌──────────────────────────────────────────────────────────────┐
│ resources  |  idle worker  |  menu / timer / fps (optional)  │
├───────────────┬──────────────────────────────────────────────┤
│ minimap       │                                              │
│ (tap jump,    │                 WORLD VIEW                   │
│  drag camera) │      tap select / drag box / long-press      │
│               │                                              │
├───────────────┴──────────────────────────────────────────────┤
│ unit portrait | command card (3×3 / context) | group tabs   │
└──────────────────────────────────────────────────────────────┘
```

- 6" phone: minimap 22–25% width, command card bottom 30–34% height; world remains ≥50%.
- 10" tablet: larger world, side panels; command card can be 2 rows.
- Fold/portrait: launcher/menus portrait; in-game landscape locked.
- Safe areas: use `WindowInsets`/`contentSafeAreaInset`; no controls under notch/gesture bar.

## 3. Gesture map

| жест | когда | действие |
|---|---|---|
| 1-finger tap | world | select unit/building; tap ground = move/attack context |
| 1-finger tap | UI | activate button / select card |
| double tap | world | select all units of same type on screen |
| 1-finger drag (> threshold) | world | selection box; if command targeting, aim |
| 1-finger long-press (500–600 ms, stationary) | world | deselect / context (right-click equivalent) |
| 1-finger long-press | UI button | tooltip / autocast toggle |
| 2-finger drag | world | camera pan |
| 2-finger pinch | world | camera zoom (stepped, ±1 per 3% distance) |
| 2-finger tap | world | cancel current command/targeting |
| drag from edge | world | camera pan (optional; disable if conflicts) |
| S Pen hover | world | cursor/preview (no click) |
| S Pen button + tap | world | right-click equivalent |
| S Pen button + drag | world | box select / camera? (configurable) |

**Targeting mode:** after Build/Ability, one-finger drag aims; cursor raised above finger for
buildings (`buildAimOffsetPx`), release casts; ghost placed only on release; tap ghost builds;
tap elsewhere moves ghost. Two-finger tap cancels.

## 4. Selection and command card

- Command card: 3×3 grid, context pages (unit/ability/build/train), cost/cooldown, disabled state.
- Queue: Shift overlay (on-screen button held by one finger) + command = queued; show queue dots.
- Autocast: long-press ability button toggles; visual ring.
- Control groups: on-screen group tabs (1–0) + tap assign/long-press recall; no keyboard required.
- Subgroup cycling: tap portrait / swipe portrait.
- Idle worker button: cycles idle workers; long-press selects all.
- Minimap: tap jump, drag camera, ping; long-press ping menu.

## 5. Camera

- Two-finger drag/pinch primary; minimap drag; optional edge pan disabled by default.
- Camera follow selected unit toggle; double-tap portrait centers.
- Zoom levels 3–4; clamp; remember per player.
- Shake/haptic only on impact, configurable.

## 6. Feedback and latency

- Touch-to-command p95 <80 ms; visual acknowledgment <50 ms (selection ring/ghost).
- Haptics: light on selection, medium on invalid, heavy on base attack (optional).
- Audio: UI click, invalid, command accepted; no sound spam.
- Frame pacing: render 60/120; input sampled every frame, command applied next tick.

## 7. Settings

- Left-handed layout mirror; UI scale 80–140%; command card size; gesture sensitivity; long-press
  duration (400–800 ms); tap slop; camera pan speed; zoom sensitivity; haptics; colorblind modes;
  contrast; text size; S Pen behavior; invert two-finger pan; edge pan off/on.

## 8. Accessibility

- Minimum contrast 4.5:1; scalable text; no color-only signals (team color + shape/outline).
- One-handed mode (phone): command card reachable; optional bottom sheet.
- Motor accessibility: larger targets, sticky selection, confirm destructive actions, no timed
  double-tap requirement.
- Screen reader for launcher/menus; in-game critical alerts via audio/haptic.

## 9. Testing

- Unit tests: gesture state machine (tap/slop/long-press/pan/pinch/cancel), thresholds in dp.
- Integration: synthetic PointerEvents → commands; replay of touch traces.
- Device lab: 6" phone, 10" tablet, fold, S Pen device; Android 12–16; 60/90/120 Hz.
- Playtest: 5+ users, first match without instructions; task success, misclicks/min, soft-locks.
- Metrics: tap latency, accidental orders, camera fights, command card errors.

## 10. Anti-patterns

- Требовать right-click/hover для базовых действий.
- Маленькие кнопки/текст; перекрытие safe area.
- Скрытые жесты без обучения; long-press без haptic/подсказки.
- Двойная эмуляция mouse (SDL + наш translator).
- Приказ на touch-down (должно быть после распознавания).
- Камера, дерущаяся с selection box; отсутствие mode locking.
- Модальные окна, блокирующие игру без паузы.
