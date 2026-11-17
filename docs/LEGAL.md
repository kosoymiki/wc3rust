# LEGAL — clean-room, IP, licenses, release gate

> Это инженерная памятка, **не юридическая консультация**. Перед публичным релизом нужен
> review квалифицированного юриста по выбранной юрисдикции.

## 1. Базовая модель

- **Код:** новый, clean-room Rust; лицензия проекта — MIT/Apache-2.0 (выбрать до релиза).
- **Данные:** пользователь подключает свою легальную копию Warcraft III (RoC/TFT/Reforged).
  В репозитории/APK **нет** игровых ассетов, карт, звуков, моделей, текстов, видео.
- **Бренд:** Warcraft/Blizzard — торговые марки Blizzard Entertainment. Проект не аффилирован,
  не endorsed; явный disclaimer в README/About.
- **Donor-код:** MIT/BSD — можно использовать с сохранением notices; GPL/unknown — не копировать.
  Мы предпочитаем поведенческий/форматный reference и собственную реализацию.

## 2. Что запрещено

1. Коммитить/распространять extracted MPQ/CASC, MDX/BLP, карты, кампании, музыку, видео,
   тексты Blizzard.
2. Обходить DRM/пароли/шифрование (включая FreeArc AES-256) или выдавать себя за официальный
   клиент.
3. Эмулировать Battle.net/официальные сервисы, использовать официальные аккаунты/токены.
4. Включать Blizzard-код, ассеты, SDK, art tools, editor code без лицензии.
5. Распространять «готовый APK с игрой» — только движок; данные пользователь добавляет сам.
6. Использовать чужие ассеты из Herofolends/репаков без прав.

## 3. Что разрешено/планируется

- Clean-room реализация форматов (MPQ/CASC/MDX/BLP/SLK/W3M/W3X) по публичным спецификациям
  и наблюдаемому поведению; reverse engineering для interoperability.
- Чтение user-owned данных на устройстве пользователя; без загрузки на серверы.
- Собственные тестовые ассеты (созданные нами) и публичные sample-карты с явной лицензией.
- Donor-код MIT/BSD с attribution; independent tests.
- Документация/исследование форматов.

## 4. FreeArc-архивы Herofolends

`data1/2/3.bin` — FreeArc **AES-256/CTR**, пароль не в APK; `setup.exe` — Inno Setup без пароля,
но архивный ключ встроен/выдаётся installer'ом. Мы **не ломаем** шифрование и не брутфорсим.
Варианты:
1. Пользователь запускает оригинальный installer на Windows/VM и предоставляет распакованную
   папку.
2. Пользователь предоставляет пароль (если он ему известен) для `freearc-c/unarc`.
3. Использовать чистые ISO-образы `Warcraft III.mdf`/`THEFROZENTHRONE.mdf` → MPQ (уже проверено)
   как источник classic-данных.
Если ни один вариант не разрешён — эти архивы остаются `unavailable`; проект работает с MPQ/CASC.

## 5. Turnip / SDL3 / driver distribution

- SDL3 — **zlib**; можно распространять с notices.
- Mesa core — **MIT**; Turnip — часть Mesa Freedreno; отдельные файлы могут иметь другие лицензии.
  Сборки K11MCH1/AdrenoToolsDrivers свободно распространяются, но мы **не бандлим** неизвестные
  бинарники; driver manager импортирует/скачивает драйвер, проверяет SHA-256 и показывает
  provenance/лицензию.
- libadrenotools — **BSD-2-Clause**; требует `useLegacyPackaging=true`, driver в internal storage,
  не на sdcard.
- Пользователь предупреждён, что кастомный драйвер экспериментален; system Vulkan fallback.
- Никаких Blizzard-ассетов/кода; никакого обхода DRM/паролей.

## 6. Лицензии доноров

| донор | лицензия | что делаем |
|---|---|---|
| Warsmash/W3AndroidEngine | MIT | behavioral reference; сохранить notice при переносе кода |
| War3Net | MIT | format/test reference; notice |
| mdx-m3-viewer / war3-model | MIT | parser/test reference; notice |
| HiveWE | MIT | editor/data model reference; notice |
| StormLib / CascLib | MIT | format reference/test oracle; notice |
| OpenRA | MIT | architecture reference; notice |
| XenosRecomp | MIT | R&D reference; notice |
| libadrenotools | BSD-2-Clause | driver integration; notice |
| android-activity | MIT | Android glue reference; notice |
| **SDL3** | **Zlib** | platform layer; notice |
| **wgpu/naga** | Apache-2.0/MIT | renderer; notices |
| **libadrenotools** | BSD-2-Clause | driver loader; notice |
| **Mesa/Turnip** | MIT (core; files may differ) | optional driver; notices/provenance |
| parry / kira | Apache-2.0 | collision/audio; notices |
| libxemit | unknown | research only; no linking until clarified |
| Urho3D (Herofolends) | MIT | UI/architecture reference only |
| BLPConverter | MIT | format reference only |

## 7. Blizzard AUP/EULA

- Blizzard legal hub: https://www.blizzard.com/en-us/legal (Warcraft III Art Tools EULA, Custom
  Game Acceptable Use Policy, fan art terms).
- Reforged custom-game policy обсуждалась как передающая Blizzard права на custom games
  (reported/contested; см. `RESEARCH.md` L2/L3). Для нашего проекта это означает: **не**
  претендовать на карты/контент пользователей, не monetize custom content, legal review.
- EULA может запрещать reverse engineering/derivative works в некоторых юрисдикциях; clean-room
  и interoperability требуют юридической оценки. Не публиковать до review.

## 8. Release gate (must)

- [ ] Нет Blizzard-ассетов/кода в git/APK/installer.
- [ ] README/About: disclaimer, no affiliation, user-owned data.
- [ ] Лицензии доноров/зависимостей перечислены; notices сохранены.
- [ ] Нет Battle.net/официальных сервисов/токенов.
- [ ] Нет DRM/password bypass; FreeArc — только user-assisted.
- [ ] Юридический review выбранной юрисдикции (лицензия, age gate, export, privacy).
- [ ] Приватность: нет телеметрии без consent; user data не уходит.
- [ ] Тесты/benchmark/Soak; crash reporting sanitized.

## 9. Monetization

- MVP: бесплатно/донаты; никаких продаж Blizzard-контента.
- Если когда-либо платно: только собственные косметические/сервисные функции, legal review,
  налоги/возвраты; не продавать доступ к «Warcraft III».
- Не использовать торговые марки в названии продукта без разрешения; рабочее имя
  `Herofolends-RS` — юридически проверить перед релизом.
