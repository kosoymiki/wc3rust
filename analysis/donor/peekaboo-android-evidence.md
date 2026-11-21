# peekaboo-android — device evidence (ledger 458, 2026-11-16)

All calls below went through the real MCP stdio server (`python3 tools/peekaboo-android/server.py`) via `mcp_call.py`, i.e. newline-delimited JSON-RPC 2.0.
Device: RMX3709, Android 16 (SDK 36), `wm size` override 1080x2414. Root: `su -c 'id'` → `uid=0(root) gid=0(root) groups=0(root) context=u:r:ksu:s0`.

## 1. Unit tests (fake runner, no device)

```
$ cd tools/peekaboo-android && python3 -m unittest discover -s tests -t .
Ran 38 tests in 0.927s

OK
```
`python3 -m py_compile server.py mcp_call.py peekaboo_android/*.py tests/*.py` → `PYCOMPILE_OK`.

## 2. Screenshot (verbatim MCP text block)

```json
{"ok": true, "path": "/data/data/com.termux/files/home/.local/state/peekaboo-android/shots/shot-20261116-174219-32073.png", "format": "png", "width": 1080, "height": 2414, "bytes": 560827, "sha256": "1757d66478ec7b70670ad55744b6f1c1d0fa22930f0ce3df431e593bb5290e19"}
```
Second content block: `{"type":"image","mimeType":"image/png"}` with the base64 PNG (≈ 4/3 of the byte size).

## 3. ui_tree (header + first 2 interactive nodes; full tree in the tool result)

```json
{"ok": true, "node_count": 57, "interactive_count": 25, "truncated": false}
[{"cls": "androidx.recyclerview.widget.RecyclerView", "id": "", "pkg": "org.telegram.messenger.web", "text": "", "desc": "", "clickable": false, "long_clickable": true, "scrollable": true, "bounds": [0, 0, 1080, 2414], "center": [540, 1207], "depth": 8}, {"cls": "android.view.ViewGroup", "id": "", "pkg": "org.telegram.messenger.web", "text": "Голосовое сообщение, 1 сек., Прослушано, 10,6 KB\nОтправлено в 17:41, Прочитано\n", "desc": "", "clickable": false, "long_clickable": false, "scrollable": false, "bounds": [0, 0, 1080, 98], "center": [540, 49], "depth": 9}]
```
Full result: 22 KB with the complete node tree (`node_count` nodes, `interactive` list with bounds/centers). `uiautomator` can answer `ERROR: could not get idle state.` on an animated screen; the backend retries once after 0.6 s (observed 17:41:11 failure before the fix, 17:41:23 success after).

## 4. window (verbatim)

```json
{"ok": true, "focus": {"hash": "49dfe11", "user": 0, "component": "org.telegram.messenger.web/org.telegram.messenger.DefaultIcon", "package": "org.telegram.messenger.web", "activity": "org.telegram.messenger.DefaultIcon", "raw": "Window{49dfe11 u0 org.telegram.messenger.web/org.telegram.messenger.DefaultIcon}"}, "focused_app": {"hash": "253133067", "user": 0, "task": 1709, "component": "org.telegram.messenger.web/org.telegram.messenger.DefaultIcon", "package": "org.telegram.messenger.web", "activity": "org.telegram.messenger.DefaultIcon"}, "window_count": 24, "visible_windows": [{"hash": "fddf98", "user": 0, "package": "ScreenDecorOverlayBottom", "activity": null, "component": "ScreenDecorOverlayBottom", "type": "NAVIGATION_BAR_PANEL", "bounds": [0, 2135, 1080, 2414], "title": null}, {"hash": "8a34bbd", "user": 0, "package": "ScreenDecorOverlay", "activity": null, "component": "ScreenDecorOverlay", "type": "NAVIGATION_BAR_PANEL", "bounds": [0, 0, 1080, 279], "title": null}, {"hash": "42c4953", "user": 0, "package": "NavigationBar_displayId_0", "activity": null, "component": "NavigationBar_displayId_0", "type": "NAVIGATION_BAR", "bounds": [0, 2282, 1080, 2414], "title": null}, {"hash": "d26d761", "user": 0, "package": "StatusBar", "activity": null, "component": "StatusBar", "type": "STATUS_BAR", "bounds": [0, 0, 1080, 120], "title": null}, {"hash": "49dfe11", "user": 0, "package": "org.telegram.messenger.web", "activity": "org.telegram.messenger.DefaultIcon", "component": "org.telegram.messenger.web/org.telegram.messenger.DefaultIcon", "type": "BASE_APPLICATION", "bounds": [0, 0, 1080, 2414], "title": null}, {"hash": "1f5bfb7", "user": 0, "package": "com.android.launcher", "activity": "com.android.launcher.Launcher", "component": "com.android.launcher/com.android.launcher.Launcher", "type": "BASE_APPLICATION", "bounds": [0, 0, 1080, 2414], "title": null}], "truncated": false}
```

## 5. apps (verbatim)

```json
{"ok": true, "total": 470, "matched": 1, "third_party_only": false, "filter": "wc3rust", "truncated": false, "packages": ["org.herofolends.p0"]}
```

## 6. analyze — metadata + real local OCR (verbatim, OCR text included)

```json
{"ok": true, "path": "/data/data/com.termux/files/home/.local/state/peekaboo-android/shots/shot-20261116-174219-32073.png", "format": "png", "width": 1080, "height": 2414, "bytes": 560827, "sha256": "1757d66478ec7b70670ad55744b6f1c1d0fa22930f0ce3df431e593bb5290e19", "question": "what is visible on this screen?", "answer": null, "ocr": {"requested": true, "engine": "tesseract", "text": "17:42 @ H »>- R Rl a4\n\n* rieyaraet\n\n< &’ ] ) Bal-m (BpaT) Qo\n\nHy KpacTb TOYHO HE CTOUT 7 4\n\n00:02\n\n17:42 &\n\nMoxHO 3apaboTatb AeHEer {7.4-\n\n° ----------- A\n00:01\n\nN kymnTb 740\n\n1742 &/\n\n° ~~~~~ o\n00:02e\n\n17:42 &/\n\n) ICoo6L|_LeHV|e 2 (&", "lang": "eng", "truncated": false}, "answer_note": "no vision model configured; OCR text and metadata returned"}
```
OCR runs `tesseract` (installed in Termux); `answer` is `null` with an explicit note because no vision model is configured — no fake answers. The same shot with `"lang":"eng+rus"` reads the Cyrillic chat text correctly:

```json
{"engine": "tesseract", "lang": "eng+rus", "text": "17:42 @ н ›- #З Rl 9) 4\n\n* печатает\n\n< &_ ] ) Ваня (Брат) ч :\n\nНу красть точно не CTOUT 7 4\n\n00:02\n\n17:42 м7\n\nМожно заработать денег 1712\n\n° ----------- A\n00:01\n\nИ купить 740\n\n…"}
```

## 7. Guard proof — tap and start without H2_DEVICE_SLOT (verbatim)

```json
{"ok": false, "error": {"code": "permission", "message": "tap refused: H2_DEVICE_SLOT != 1 (the tech-lead has not granted the device slot)", "detail": {"tool": "tap", "h2_device_slot": null, "grant": "start the server (or the calling shell) with H2_DEVICE_SLOT=1"}}, "dispatched": false, "retry_safe": true}
```

```json
{"ok": false, "error": {"code": "permission", "message": "start refused: H2_DEVICE_SLOT != 1 (the tech-lead has not granted the device slot)", "detail": {"tool": "start", "h2_device_slot": null, "grant": "start the server (or the calling shell) with H2_DEVICE_SLOT=1"}}, "dispatched": false, "retry_safe": true}
```
Both were refused before any `su` call (`dispatched:false`; the fake-runner tests assert `runner.calls == []` for the same path).

## 8. Read-only proof of the start resolution path

```
$ su -c 'cmd package resolve-activity --brief -c android.intent.category.LAUNCHER org.herofolends.p0'
priority=0 preferredOrder=0 match=0x108000 specificIndex=-1 isDefault=false
org.herofolends.p0/org.libsdl.app.SDLActivity
```

## 9. Audit log tail (verbatim, 8 lines)

```json
{"ts": "2026-11-16T17:42:09+0300", "tool": "apps", "args": {"filter": "wc3rust"}, "result": {"ok": true, "matched": 1}, "duration_ms": 66.2, "slot": false, "pid": 31910}
{"ts": "2026-11-16T17:42:20+0300", "tool": "screenshot", "args": {}, "result": {"ok": true, "width": 1080, "height": 2414}, "duration_ms": 654.9, "slot": false, "pid": 32073}
{"ts": "2026-11-16T17:42:23+0300", "tool": "ui_tree", "args": {"max_nodes": 500, "max_interactive": 30}, "result": {"ok": true, "node_count": 57, "interactive_count": 25}, "duration_ms": 2287.3, "slot": false, "pid": 32097}
{"ts": "2026-11-16T17:42:23+0300", "tool": "window", "args": {"max_windows": 8}, "result": {"ok": true, "window_count": 24, "focus": "org.telegram.messenger.web/org.telegram.messenger.DefaultIcon"}, "duration_ms": 137.4, "slot": false, "pid": 32135}
{"ts": "2026-11-16T17:42:24+0300", "tool": "apps", "args": {"filter": "wc3rust"}, "result": {"ok": true, "matched": 1}, "duration_ms": 74.1, "slot": false, "pid": 32149}
{"ts": "2026-11-16T17:42:27+0300", "tool": "analyze", "args": {"path": "/data/data/com.termux/files/home/.local/state/peekaboo-android/shots/shot-20261116-174219-32073.png", "question": "what is visible on this screen?"}, "result": {"ok": true, "width": 1080, "height": 2414}, "duration_ms": 3059.1, "slot": false, "pid": 32155}
{"ts": "2026-11-16T17:42:27+0300", "tool": "tap", "args": {"x": 540, "y": 1201}, "result": {"ok": false, "code": "permission", "message": "tap refused: H2_DEVICE_SLOT != 1 (the tech-lead has not granted the device slot)"}, "duration_ms": 0.0, "slot": false, "pid": 32207}
{"ts": "2026-11-16T17:42:28+0300", "tool": "start", "args": {"target": "org.herofolends.p0"}, "result": {"ok": false, "code": "permission", "message": "start refused: H2_DEVICE_SLOT != 1 (the tech-lead has not granted the device slot)"}, "duration_ms": 0.1, "slot": false, "pid": 32214}
```

## 10. Registration

`~/.config/opencode/opencode.json` (backup `opencode.json.bak-20261116-peekaboo`):

```diff
+    "peekaboo-android": {
+      "type": "local",
+      "command": [
+        "/data/data/com.termux/files/usr/bin/python3",
+        "/data/data/com.termux/files/home/src/herofolends-rs/tools/peekaboo-android/server.py"
+      ],
+      "enabled": true,
+      "codemode": true,
+      "timeout": 180000
+    }
```
`python3 -m json.tool opencode.json` → `JSON_OK`. The exact registered command answers `initialize` with `serverInfo.name = "peekaboo-android"` and `tools/list` with 12 tools.

## 11. Not possible in this session (explicit)

- `tap`/`swipe`/`long_press`/`type`/`key`/`start` were **not executed against the device**: `H2_DEVICE_SLOT` is unset (guard refused, evidence above) and the device was in active operator use — focus moved from Termux to Telegram between calls, and the game (`org.herofolends.p0`) was never foreground. Per the mission, the HUD tap was only to run if the game was foreground.
- `cmd clipboard` on this build prints `No shell command implementation.`, so non-ASCII typing returns `unsupported_text_encoding` instead of silently dropping characters (unit-tested with a fake clipboard).
- Donor repo untouched: `git -C ~/src/donors/peekaboo status --porcelain` → empty.
