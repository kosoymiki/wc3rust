# peekaboo-android — Peekaboo's computer-use contract on Android/Termux

Port of the [openclaw/Peekaboo](https://github.com/openclaw/Peekaboo) MCP tool contract to Android, replacing the macOS-only backend (AXorcist / Accessibility / CGWindow / ScreenCaptureKit) with `su -c` device commands. Python 3 standard library only; no pip dependencies.

- Donor analysed: `~/src/donors/peekaboo` v4.9.9, HEAD `b69713f02dec` (read-only).
- Full tool-by-tool mapping and honest non-portable list: [`analysis/donor/peekaboo-matrix.md`](../../analysis/donor/peekaboo-matrix.md).

## Tools

| Tool | Backend | Device slot |
|------|---------|-------------|
| `screenshot` | `su -c screencap -p <file>` → PNG base64 + width/height/bytes/sha256 | always allowed |
| `ui_tree` | `su -c uiautomator dump` → JSON tree + interactive nodes with tap centers (one bounded retry on transient "could not get idle state") | always allowed |
| `window` | `su -c dumpsys window displays` (focus) + `dumpsys window windows` (full blocks with `Frames:` bounds) | always allowed |
| `apps` | `su -c pm list packages` (+ filter, third-party only) | always allowed |
| `analyze` | image metadata + sha256 + real local OCR (`tesseract`) + optional hook | always allowed |
| `sleep` | local sleep, ≤ 60 s | always allowed |
| `tap` | `su -c input tap x y` | **requires `H2_DEVICE_SLOT=1`** |
| `swipe` | `su -c input swipe x1 y1 x2 y2 ms` | **requires `H2_DEVICE_SLOT=1`** |
| `long_press` | `su -c input swipe x y x y ms` | **requires `H2_DEVICE_SLOT=1`** |
| `type` | `su -c input text` (ASCII, spaces→`%s`) / clipboard paste for non-ASCII | **requires `H2_DEVICE_SLOT=1`** |
| `key` | `su -c input keyevent <code|KEYCODE_NAME>` | **requires `H2_DEVICE_SLOT=1`** |
| `start` | `su -c am start -W -n <component>` (launcher resolved from package) | **requires `H2_DEVICE_SLOT=1`** |

The slot guard matches `scripts/game-selftest.sh:4` / `scripts/qa/device_evidence.sh:40-41`: input tools refuse with `{"ok":false,"error":{"code":"permission",...},"dispatched":false}` and never touch `input`.

## Run

```bash
# MCP stdio server (what opencode launches)
python3 /data/data/com.termux/files/home/src/wc3rust/tools/peekaboo-android/server.py

# Manual smoke through the real stdio protocol
python3 mcp_call.py tools/list
python3 mcp_call.py screenshot
python3 mcp_call.py ui_tree '{"max_nodes":300}'
python3 mcp_call.py window '{"max_windows":8}'
python3 mcp_call.py analyze '{"path":"/path/shot.png"}'
python3 mcp_call.py tap '{"x":540,"y":1200}'   # refused without the slot

# Tests (38 unit tests, no device access)
cd tools/peekaboo-android && python3 -m unittest discover -s tests -t .
```

## Registration (opencode)

`~/.config/opencode/opencode.json`:

```json
"peekaboo-android": {
  "type": "local",
  "command": ["/data/data/com.termux/files/usr/bin/python3",
              "/data/data/com.termux/files/home/src/wc3rust/tools/peekaboo-android/server.py"],
  "enabled": true,
  "codemode": true,
  "timeout": 180000
}
```

Do **not** bake `H2_DEVICE_SLOT=1` into the registration: the slot is granted per work session by the tech-lead (environment of the process that launches the MCP server), exactly like the device scripts.

## Contract

- Newline-delimited JSON-RPC 2.0 on stdio (stdout is protocol-only; diagnostics go to stderr).
- `initialize`, `notifications/initialized`, `ping`, `tools/list`, `tools/call`, plus empty `resources/list` / `prompts/list`.
- Every successful result is `{"ok":true,...}` in a `text` content block; `screenshot` adds an `image` block (`image/png`, base64).
- Every failure is `{"ok":false,"error":{"code","message","detail"},"dispatched":bool,"retry_safe":bool}` with `isError:true`.
- Error codes: `permission` (slot absent or `su` denied, stderr included), `invalid_argument`, `unknown_tool`, `timeout`, `unavailable`, `ui_unavailable`, `ui_parse_failed`, `screenshot_failed`, `resolve_failed`, `start_failed`, `not_found`, `input_too_large`, `unsupported_text_encoding`, `internal`.
- No silent fallbacks: a failed dump/command returns the error object with rc + stderr.

## Audit log

Every call (read and action) appends one JSON line to `~/.local/state/peekaboo-android/actions.log` (mode 0600):

```json
{"ts":"2026-11-16T17:36:56+0300","tool":"tap","args":{"x":540,"y":1201},"result":{"ok":false,"code":"permission","message":"tap refused: H2_DEVICE_SLOT != 1 (the tech-lead has not granted the device slot)"},"duration_ms":0.0,"slot":false,"pid":26927}
```

Text arguments are truncated to 200 characters in the log.

## Text entry semantics

- ASCII: `input text` with spaces encoded as `%s` (Android `InputShellCommand`); `\n` becomes `KEYCODE_ENTER` (66). A literal `%s` in the text is typed as two calls (`%` then `s`) because the shell command has no escape for it.
- Non-ASCII: `cmd clipboard settext` + `KEYCODE_PASTE` (279). If `cmd clipboard` is not implemented on the build (observed on this Android 16 device: `No shell command implementation.`), the tool returns `unsupported_text_encoding` with the probe output and a remedy — it never silently drops characters.
- Text is injected into whatever field currently has focus; there is no element targeting (Android shell has no AX focus API).

## analyze

Returns `format`, `width`, `height`, `bytes`, `sha256`, and OCR text from `tesseract` when installed (`pkg install tesseract`; override with `PEEKABOO_ANDROID_TESSERACT`). `lang` defaults to `eng`; this device also has `rus`, so pass `"lang":"eng+rus"` for Cyrillic UI. Optional external analyzer: `PEEKABOO_ANDROID_ANALYZE_CMD="/path/tool --flag"` is executed with the image path appended; stdout/stderr/rc are returned verbatim. `question` is echoed; `answer` stays `null` with `answer_note` because this tool has no vision model — it never fabricates an answer.

## Deliberate gaps

Element IDs/AX actions, `set_value`, `select_text`, window manipulation, menu/dock/space, clipboard read/write, video capture, nested agent, `shell` are not ported. Reasons per tool are in the donor matrix; none is stubbed.
