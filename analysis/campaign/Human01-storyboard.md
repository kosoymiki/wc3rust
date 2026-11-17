# Human01 — cutscene storyboard

Map `/data/data/com.termux/files/home/src/herofolends-rs/crates/wc3-game/../../analysis/war3_full3/Maps/Campaign/Human01.w3m` — generated from the running runtime (`campaign_report --mission Human01 --frames 900 --write`).

`CAMPAIGN_CINE frames=900 cameras=12 fades=11 transmissions=11 subtitles=[8.35s Утер Светоносный: Добро пожаловать, принц Артес. Ваше присутствие - большая честь для всех нас. (5.63s) | 13.98s Артес: Не нужно формальностей, Утер. Я еще не король. Рад тебя видеть! (4.87s) | 19.10s Утер Светоносный: Я тоже рад, мой мальчик. Хорошо, что король Теренас прислал мне на помощь именно тебя. (6.13s) | 25.23s Артес: Мой отец не теряет надежды, что рядом с тобой я наберусь терпения и опыта. (5.47s) | 30.70s Утер Светоносный: Всякий отец имеет право мечтать. (3.16s) | 34.11s Артес: Мы остановились здесь. Разведчики сообщают, что где-то за следующим холмом укрыт лагерь орков. (6.57s) | 40.68s Утер Светоносный: Так я и думал. (2.17s) | 42.85s Артес: Это еще не все. Неподалеку находится деревня Странбард. Орки собираются напасть на нее. Насколько мне известно, деревня совершенно беззащитна. (9.21s) | 64.71s Утер Светоносный: Я должен немедленно отправиться к лагерю орков. Ты можешь взять на себя защиту Странбарда? (6.35s) | 71.06s Артес: Конечно, Утер. За меня не беспокойся. (3.44s) | 74.49s Утер Светоносный: Хорошо. Когда деревня будет в безопасности, приходи к лагерю орков. Я буду ждать тебя там. Будь осторожен. (7.23s) | 88.95s scene: |cffffcc00ОСНОВНОЕ ЗАДАНИЕ|r
Странбард
 - Доберитесь до Странбарда.
 - Артес не должен погибнуть. (20.00s) | 128.95s scene: |cff32ceebВЫ ПОЛУЧИЛИ ПОДКРЕПЛЕНИЕ|r
Пехотинец. Эти воины, составляющую основу регулярной армии Лордерона, неплохо сражаются в ближнем бою. (15.00s) | 158.95s scene: |cff32cd32ПОДСКАЗКА:|r Используйте заклинание "Благодать", чтобы лечить раненых. (12.00s)]`

Objectives: 3 rows (1 required) · camera keyframes: 12 · fades: 11 · subtitles: 14

Timeline note: the headless VM has no coroutine suspension, so the whole
intro runs inside one tick. Beats are stamped with the **cutscene clock**
(`t = max(event time, previous beat end)`) and each beat holds the cutscene
for the duration the script itself declares — a camera setup's blend, a
fade's duration, a line's transmission duration with a reading-time floor
when the voiceover length is not decoded. Order and durations are the
script's; only the clock origin is reconstructed.

## Objectives

| # | required | title | steps |
|---|---|---|---|
| 1 | yes | Странбард | 0/2 |
| 2 | no | Атаман разбойников | 0/2 |
| 3 | no | Спасите Тимми | 0/1 |

## Camera beats

| t (s) | shot | target x,y | dist | yaw | pitch | z | blend (s) |
|---|---|---|---|---|---|---|---|
| 0.00 | setup 467 | -2804, 1654 | 1815 | 68° | 348° | 0 | 0.00 |
| 0.25 | setup 478 | -2804, 1654 | 1815 | 77° | 336° | 0 | 0.00 |
| 4.10 | setup 476 | -3171, 2372 | 1364 | 90° | 300° | 0 | 0.25 |
| 4.35 | setup 475 | -2834, 2200 | 956 | 95° | 350° | 0 | 2.50 |
| 18.85 | setup 474 | -2600, 1833 | 841 | 20° | 328° | 0 | 0.25 |
| 33.86 | setup 462 | -2729, 1922 | 830 | 84° | 331° | 0 | 0.25 |
| 52.66 | setup 463 | 1490, 1994 | 1127 | 51° | 348° | 0 | 0.35 |
| 52.91 | setup 464 | 1832, 2357 | 1650 | 47° | 337° | 0 | 2.50 |
| 60.51 | setup 465 | 2493, 1549 | 974 | 179° | 336° | 0 | 0.25 |
| 60.76 | setup 466 | 2285, 1105 | 974 | 178° | 339° | 0 | 2.50 |
| 64.46 | setup 462 | -2729, 1922 | 830 | 84° | 331° | 0 | 0.42 |
| 108.95 | pan | 2112, 1904 | 830 | 84° | 331° | 0 | 0.40 |

## Fades

| t (s) | duration | colour | alpha |
|---|---|---|---|
| 0.00 | 0.00 | #0000FF | 1.00 → 1.00 |
| 0.50 | 0.00 | #0000FF | 1.00 → 1.00 |
| 0.50 | 3.00 | #000000 | 1.00 → 0.00 |
| 3.50 | 0.30 | #0000FF | 0.00 → 1.00 |
| 3.80 | 0.30 | #000000 | 1.00 → 0.00 |
| 52.06 | 0.30 | #0000FF | 0.00 → 1.00 |
| 52.36 | 0.30 | #000000 | 1.00 → 0.00 |
| 59.91 | 0.30 | #0000FF | 0.00 → 1.00 |
| 60.21 | 0.30 | #000000 | 1.00 → 0.00 |
| 63.86 | 0.30 | #0000FF | 0.00 → 1.00 |
| 64.16 | 0.30 | #000000 | 1.00 → 0.00 |

## Subtitles

| t (s) | speaker | text | shown (s) | portrait | kind |
|---|---|---|---|---|---|
| 8.35 | Утер Светоносный | Добро пожаловать, принц Артес. Ваше присутствие - большая честь для всех нас. | 5.63 | — | transmission |
| 13.98 | Артес | Не нужно формальностей, Утер. Я еще не король. Рад тебя видеть! | 4.87 | — | transmission |
| 19.10 | Утер Светоносный | Я тоже рад, мой мальчик. Хорошо, что король Теренас прислал мне на помощь именно тебя. | 6.13 | — | transmission |
| 25.23 | Артес | Мой отец не теряет надежды, что рядом с тобой я наберусь терпения и опыта. | 5.47 | — | transmission |
| 30.70 | Утер Светоносный | Всякий отец имеет право мечтать. | 3.16 | — | transmission |
| 34.11 | Артес | Мы остановились здесь. Разведчики сообщают, что где-то за следующим холмом укрыт лагерь орков. | 6.57 | — | transmission |
| 40.68 | Утер Светоносный | Так я и думал. | 2.17 | — | transmission |
| 42.85 | Артес | Это еще не все. Неподалеку находится деревня Странбард. Орки собираются напасть на нее. Насколько мне известно, деревня совершенно беззащитна. | 9.21 | — | transmission |
| 64.71 | Утер Светоносный | Я должен немедленно отправиться к лагерю орков. Ты можешь взять на себя защиту Странбарда? | 6.35 | — | transmission |
| 71.06 | Артес | Конечно, Утер. За меня не беспокойся. | 3.44 | — | transmission |
| 74.49 | Утер Светоносный | Хорошо. Когда деревня будет в безопасности, приходи к лагерю орков. Я буду ждать тебя там. Будь осторожен. | 7.23 | — | transmission |
| 88.95 | — | \|cffffcc00ОСНОВНОЕ ЗАДАНИЕ\|r Странбард  - Доберитесь до Странбарда.  - Артес не должен погибнуть. | 20.00 | — | message |
| 128.95 | — | \|cff32ceebВЫ ПОЛУЧИЛИ ПОДКРЕПЛЕНИЕ\|r Пехотинец. Эти воины, составляющую основу регулярной армии Лордерона, неплохо сражаются в ближнем бою. | 15.00 | — | message |
| 158.95 | — | \|cff32cd32ПОДСКАЗКА:\|r Используйте заклинание "Благодать", чтобы лечить раненых. | 12.00 | — | message |

## Hooks the screens lane needs

* `Game::camera_director() -> Option<CameraDirector>` — the keyframe stream (`CameraKeyframe { t, pose, blend, setup }`).
* `Game::cutscene_camera() -> Option<CameraPose>` — pose at the current cutscene-clock time.
* `Game::screen_fade() -> Option<(f64 alpha, [f64;3] colour)>` — the `CinematicFadeBJ` tint.
* `Game::subtitle_now() -> Option<SubtitleLine>` — speaker, text, duration, portrait unit handle.
* `Game::quest_rows() -> Vec<QuestRow>` — the objectives panel.
* `Game::cutscene_active() -> bool` — the script holds the camera away from gameplay.
* `Game::campaign_frame(dt)` — one call per frame; syncs quests/camera/subtitles and starts a requested movie.
* Letterbox: draw two bars of `0.12 * viewport.y` while `cutscene_active()`, and suppress the command card + minimap with them.
