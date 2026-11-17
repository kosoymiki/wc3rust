# Donor matrix — openclaw/Peekaboo → peekaboo-android

- Donor: `~/src/donors/peekaboo` (read-only), version **4.9.9** (`version.json`), git HEAD `b69713f02dec`, 2146 `*.swift` files, 69 MB.
- Contract sources read byte-level: `peekaboo-mcp.js` (MCP crash-restart wrapper, 191 lines), `docs/MCP.md` (462 lines), `docs/commands/mcp.md` (141 lines), `Core/PeekabooCore/Sources/PeekabooAgentRuntime/MCP/Server/MCPToolCatalog.swift` (132 lines, catalog at lines 92–131), `MCP/Tools/*.swift` (28 tool files), `ToolRegistry/ToolDefinitions.swift`.
- Port target: `tools/peekaboo-android/server.py` (Python 3 stdlib, newline-delimited JSON-RPC 2.0 over stdio, Android backend via `su -c`).
- Verdict legend: **ported** = same contract and semantics on Android; **adapted** = same purpose, Android mechanism differs (documented); **not portable** = no Android shell-level equivalent without instrumentation/OS facility that Android does not expose.

## 1. Donor MCP tool inventory (observed)

`MCPToolCatalog.unfilteredTools` (`MCPToolCatalog.swift:92-131`) exposes 27 tools: `image, capture, analyze, browser, permissions, sleep, see, inspect_ui, verify_state, click, type, set_value, select_text, action, scroll, press, drag, move, app, window, menu, clipboard, paste, agent, dock, dialog, space`. `ShellTool.swift` defines `shell` but it is **not** in the MCP catalog (docs/commands/mcp.md: "never exposes Shell").

## 2. Tool-by-tool matrix

| # | Donor tool | Donor semantics (source) | Android equivalent | Verdict |
|---|------------|--------------------------|--------------------|---------|
| 1 | `image` | Screenshot only; display/frontmost/app/window/menubar targets; `path`, `format` (`png|jpg|data`), `scale`, `max_dimension`; inline base64 for `format: data` (`ImageTool.swift`) | `screenshot` → `su -c screencap -p <file>`; returns PNG base64 + width/height/size + saved path | **ported** (`screenshot`) |
| 2 | `see` | Screenshot + accessibility element map with opaque per-snapshot IDs, clickable/actionable flags, optional annotate (`SeeTool.swift`) | `ui_tree` → `su -c uiautomator dump` + XML parse; returns node tree, bounds, text, resource-id, clickable, center points; no screenshot overlay, no opaque snapshot IDs | **adapted** (`ui_tree`) — bounds replace element IDs; agents tap centers |
| 3 | `inspect_ui` | AX-only traversal, freshness flag, truncated/AX-incomplete diagnostics (`InspectUITool.swift`) | `ui_tree` (fresh dump every call; no cache — stronger freshness, no truncation metadata) | **adapted** (`ui_tree`) |
| 4 | `click` | Element ID / query / coords; double/right/middle/triple; background-safe routing, snapshot receipts (`ClickTool.swift`) | `tap {x,y}` → `su -c input tap x y` (+ `long_press {x,y,ms}` via `input swipe x y x y ms`) | **adapted** (`tap`, `long_press`) — coordinates only; no element-ID receipts, no right/middle/double (Android `input` lacks them) |
| 5 | `scroll` | Directional wheel with AX hit-test owner, background-safe (`ScrollTool.swift`) | `swipe {x1,y1,x2,y2,ms}` → `input swipe`; direction = caller-chosen vector | **adapted** (`swipe`) — pixel gestures, no AX scroll-owner guarantee |
| 6 | `drag` | Held-pointer linear path, 1–96 samples, background-safe (`DragTool.swift`) | `swipe` with long `ms` (single linear segment) | **adapted** (`swipe`) — no multi-sample path; `input draganddrop` not exposed on all builds |
| 7 | `type` | Typed text with snapshot target, clear/selection keys, unicode via AX (`TypeTool.swift`) | `type {text}` → `input text` with shell escaping, spaces as `%s`, newlines as KEYCODE_ENTER; non-ASCII requires clipboard route (`cmd clipboard settext` + `input keyevent 279`), else explicit `unsupported_text_encoding` error | **adapted** (`type`) |
| 8 | `press` | Key chords + modifier syntax (`cmd+c`), snapshot-targeted (`PressTool.swift`) | `key {code}` → `input keyevent <code|KEYCODE_NAME>`; single key only (Android has no `cmd` modifier; `input keycombination` not in this contract) | **adapted** (`key`) |
| 9 | `set_value` | Direct AX value mutation on settable fields (`SetValueTool.swift`) | none — Android shell has no in-process accessibility setter; UiAutomator `setText` needs instrumentation and is not exposed by `uiautomator` CLI | **not portable** (reason: no shell-level AX value API) |
| 10 | `select_text` | Literal selection/caret placement with UTF-16 ranges (`SelectTextTool.swift`) | none — no shell API for AX selection ranges; caret can only be moved with keyevents (unreliable, field-dependent) | **not portable** |
| 11 | `action` | Invoke named AX action (`AXPress`, custom actions) (`ActionTool.swift`) | none — Android `AccessibilityNodeInfo.performAction` is in-process only; `uiautomator` CLI dumps but does not act | **not portable** (coordinate tap covers the common `AXPress` case) |
| 12 | `scroll`/`move` pointer | `move` = global cursor move, foreground consent (`MoveTool.swift`) | none — Android touch is event-injected, there is no persistent arrow cursor | **not portable** |
| 13 | `window` | list (IDs/bounds/off-screen/capability), close, minimize, restore, maximize, move, resize, set-bounds, focus (`WindowTool.swift:13`) | `window` → `su -c dumpsys window displays` (`mCurrentFocus`/`mFocusedApp`) + `dumpsys window windows` (full blocks; bounds from `Frames: … frame=[…]`) → visible windows (package/activity/type/bounds); manipulation actions unavailable | **partial**: list/inspect **ported**; close/minimize/move/resize/focus **not portable** (no shell WM API; `am`/`wm` operate on tasks/display, not arbitrary windows) |
| 14 | `app` | list running apps, launch/open/quit/relaunch/focus/hide/unhide/switch (`AppTool.swift`) | `apps {filter,third_party}` → `pm list packages`; `start {component}` → `am start -W`; focus = `am start` + `dumpsys window` verification; quit = `am force-stop` (not exposed — destructive to user state) | **partial**: list **adapted** (installed, not running); launch **ported** (`start`); quit/focus/hide/switch **not exposed** (would kill/steal the operator's foreground app) |
| 15 | `menu` | macOS menu bar list/click (`MenuTool.swift`) | none — Android app-menu internals are not exposed to shell; visible menu items do appear in `ui_tree` if rendered | **not portable** (menus accessible only as UI nodes) |
| 16 | `dialog` | System dialog automation (open/save panels, TCC prompts) (`DialogTool.swift`) | none — Android system dialogs are regular windows; `ui_tree` can observe and `tap` can answer if visible, but there is no dialog-scoped API | **not portable as a tool** (covered by `ui_tree`+`tap`) |
| 17 | `permissions` | TCC/Screen Recording/Accessibility status (`PermissionsTool.swift`) | none — Android has no TCC; relevant state = `su` availability + `dumpsys accessibility` enabled services | **not portable** (replaced by `su` error reporting on every tool) |
| 18 | `dock` | macOS Dock list/click/launch (`DockTool.swift`) | none — no Dock on Android; launcher icons are ordinary UI nodes | **not portable** |
| 19 | `space` | Mission Control spaces switch/move window (`SpaceTool.swift`) | none — Android has no virtual desktops; recent-task switching via keyevent APP_SWITCH only | **not portable** |
| 20 | `clipboard` | Clipboard read/write with UTI (`ClipboardTool.swift`) | none in this contract — `cmd clipboard` on this device returns "No shell command implementation."; reading the user clipboard is also a privacy hazard | **not portable** (observed device limitation) |
| 21 | `paste` | Clipboard paste with AX target receipts (`PasteTool.swift`) | not exposed — same clipboard limitation; text entry is `type` | **not portable** |
| 22 | `browser` | Puppeteer/CDP browser control, foreground authority (`BrowserTool.swift`) | not applicable — web needs are covered by the existing `stealth-browser` MCP (nodriver/Chromium), a better engine than a device-side browser bridge | **not portable / superseded** |
| 23 | `agent` | Nested natural-language agent execution (`MCPAgentTool.swift`) | none — this is a model-runtime feature, not a device capability | **not portable** |
| 24 | `capture` | Live screen/window recording to frames + contact sheet + MP4 (`CaptureTool.swift`) | none — `screenrecord` exists but is a long-running recorder with no frame custody; not part of this contract | **not portable** |
| 25 | `analyze` | AI vision Q&A on an image file, 10 MiB cap, provider config (`AnalyzeTool.swift`) | `analyze {path, question?, ocr?}` → metadata (format/dims/bytes/sha256) + real local OCR via `tesseract` when installed; optional external hook via `PEEKABOO_ANDROID_ANALYZE_CMD`; no LLM, `answer` is `null` with an explicit reason | **adapted** (`analyze`) |
| 26 | `verify_state` | Bounded polling of AX/window predicates (`VerifyStateTool.swift`) | portable in principle via repeated `ui_tree`/`window`; not part of this contract | **not ported** (honest gap; agents poll `ui_tree`) |
| 27 | `sleep` | Millisecond sleep for clients without shell (`SleepTool.swift`) | `sleep {ms}` → `time.sleep`, capped (60 s) | **ported** (`sleep`) |
| 28 | `shell` | Shell command execution (defined but excluded from MCP catalog) | deliberately **not exposed** — would bypass the action guard and the MCP trust boundary | **not portable by design** |

## 3. Contract parity that is preserved

- **Transport**: stdio, newline-delimited JSON-RPC 2.0, stdout reserved for protocol messages (donor `docs/MCP.md:76-78`). Diagnostics/logs go to stderr and `~/.local/state/peekaboo-android/actions.log`.
- **Methods**: `initialize`, `notifications/initialized`, `tools/list`, `tools/call`, `ping` (+ empty `resources/list`/`prompts/list` for client compatibility).
- **Result shape**: `content[]` with a `text` block containing compact JSON; `screenshot` additionally returns an `image` block (base64 PNG), mirroring Peekaboo's inline screenshot.
- **Errors**: donor returns controller errors as tool results; we return `isError: true` plus `{"ok":false,"error":{"code","message","detail"}}`. A failed `su` is always `code: "permission"` with stderr, never an empty success.
- **Argument validation**: numeric arguments validated before dispatch (donor `docs/MCP.md:71-74`); rejections report `dispatched: false`.

## 4. Safety mapping

| Donor | Android |
|-------|---------|
| Background-only default catalog; `--allow-foreground` opt-in (`docs/commands/mcp.md:19-25`) | `H2_DEVICE_SLOT != 1` ⇒ every action tool (`tap`, `swipe`, `long_press`, `type`, `key`, `start`) refuses with `permission`; read-only tools (`screenshot`, `ui_tree`, `window`, `apps`, `analyze`, `sleep`) always allowed. Same guard as `scripts/game-selftest.sh:4` and `scripts/qa/device_evidence.sh:40-41`. |
| Snapshot receipts (fresh exact-window `see` before input) | Not achievable over `input`; instead every action is logged with args/result and coordinates are caller-explicit. No replay protection is claimed. |
| Target identity/process-generation receipts | Not available; documented limitation, not faked. |

## 5. Deliberate omissions (honest gaps)

- No element IDs, no AX actions, no value/selection mutation, no window manipulation, no clipboard, no menu/dock/space, no video capture, no nested agent. Reasons are in the table; none is stubbed.
- `ui_tree` is a full fresh dump each call (no cache) — that is the Android facility; freshness is stronger, but trees can be large, so output is bounded (`max_nodes`, default 400 nodes, interactive-first).
