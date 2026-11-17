"""Unit tests for the Android backend (no device access; su is faked)."""

from __future__ import annotations

import shlex
import struct
import tempfile
import unittest
import zlib
from pathlib import Path

from peekaboo_android.backend import (
    AndroidBackend,
    DeviceError,
    RawRun,
    parse_am_start,
    parse_dumpsys_window,
    png_size,
    su_command,
)

UI_XML = b"""<?xml version='1.0' encoding='UTF-8' standalone='yes' ?>
<hierarchy rotation="0">
  <node index="0" text="" resource-id="" class="android.widget.FrameLayout" package="org.wc3rust.p0" content-desc="" clickable="false" enabled="true" focused="false" scrollable="false" long-clickable="false" selected="false" bounds="[0,0][1080,2414]">
    <node index="0" text="Gold" resource-id="org.wc3rust.p0:id/gold" class="android.widget.TextView" package="org.wc3rust.p0" content-desc="" clickable="false" enabled="true" focused="false" scrollable="false" long-clickable="false" selected="false" bounds="[10,20][110,60]" />
    <node index="1" text="" resource-id="org.wc3rust.p0:id/hud_slot_1" class="android.widget.ImageButton" package="org.wc3rust.p0" content-desc="Training slot" clickable="true" enabled="true" focused="false" scrollable="false" long-clickable="true" selected="false" bounds="[900,2200][1060,2380]" />
  </node>
</hierarchy>
"""

DUMPSYS = """
  mCurrentFocus=Window{aa7f720 u0 org.wc3rust.p0/org.libsdl.app.SDLActivity}
  mFocusedApp=ActivityRecord{180737328 u0 org.wc3rust.p0/.SDLActivity t1652}

  Window #0 Window{aa7f720 u0 org.wc3rust.p0/org.libsdl.app.SDLActivity}:
    mDisplayId=0
    mBounds=[0,0][1080,2414]
    mHasSurface=true
    isVisible=true
    mAttrs={(0,0)(fillxfill) ty=APPLICATION}

  Window #1 Window{bb00123 u0 com.termux/com.termux.app.TermuxActivity}:
    mBounds=[0,0][1080,2414]
    mHasSurface=true
    isVisible=false
    mAttrs={(0,0)(fillxfill) ty=APPLICATION}
"""

AM_START = """Starting: Intent { cmp=org.wc3rust.p0/org.libsdl.app.SDLActivity }
Status: ok
LaunchState: WARM
Activity: org.wc3rust.p0/org.libsdl.app.SDLActivity
ThisTime: 145
TotalTime: 145
WaitTime: 160
"""


def make_png(width: int, height: int) -> bytes:
    raw = b"".join(b"\x00" + b"\x00\x00\x00" * width for _ in range(height))

    def chunk(tag: bytes, payload: bytes) -> bytes:
        return struct.pack(">I", len(payload)) + tag + payload + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")


class FakeRunner:
    def __init__(self, handler):
        self.handler = handler
        self.calls: list[list[str]] = []

    def __call__(self, argv, timeout):
        self.calls.append(list(argv))
        return self.handler(argv)


class BackendTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.state = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def backend(self, handler):
        runner = FakeRunner(handler)
        return AndroidBackend(runner=runner, state_dir=self.state), runner

    # ------------------------------------------------------------- commands
    def test_su_command_quotes_every_token(self):
        argv = ["input", "text", "a; rm -rf ~ && echo $HOME"]
        wrapped = su_command(argv)
        self.assertEqual(wrapped[:2], ["su", "-c"])
        self.assertEqual(shlex.split(wrapped[2]), argv)

    def test_text_argv_spaces_become_placeholder(self):
        self.assertEqual(AndroidBackend.text_argv("Hello world"), ["input", "text", "Hello%sworld"])
        self.assertEqual(AndroidBackend.text_argv("a'b"), ["input", "text", "a'b"])

    def test_type_ascii_dispatches_input_text(self):
        backend, runner = self.backend(lambda argv: RawRun(0, b"", ""))
        result = backend.type_text("hello world")
        self.assertTrue(result["ok"])
        self.assertEqual(runner.calls, [["input", "text", "hello%sworld"]])
        self.assertEqual(result["modes"], ["input_text"])

    def test_type_multiline_presses_enter(self):
        backend, runner = self.backend(lambda argv: RawRun(0, b"", ""))
        backend.type_text("a\n\nb")
        self.assertEqual(
            runner.calls,
            [
                ["input", "text", "a"],
                ["input", "keyevent", "66"],
                ["input", "keyevent", "66"],
                ["input", "text", "b"],
            ],
        )

    def test_type_literal_placeholder_is_split_not_clipboarded(self):
        backend, runner = self.backend(lambda argv: RawRun(0, b"", ""))
        result = backend.type_text("100%s done")
        self.assertEqual(result["modes"], ["input_text"])
        self.assertEqual(
            runner.calls,
            [
                ["input", "text", "100"],
                ["input", "text", "%"],
                ["input", "text", "s"],
                ["input", "text", "%sdone"],
            ],
        )

    def test_type_unicode_without_clipboard_is_explicit_error(self):
        backend, runner = self.backend(lambda argv: RawRun(0, b"", ""))
        with self.assertRaises(DeviceError) as ctx:
            backend.type_text("привет")
        self.assertEqual(ctx.exception.code, "unsupported_text_encoding")
        self.assertEqual(runner.calls, [["cmd", "clipboard"]])

    def test_type_unicode_with_clipboard_uses_paste(self):
        def handler(argv):
            if argv == ["cmd", "clipboard"]:
                return RawRun(0, b"usage: cmd clipboard settext <text>", "")
            return RawRun(0, b"", "")

        backend, runner = self.backend(handler)
        result = backend.type_text("привет")
        self.assertEqual(result["modes"], ["clipboard_paste"])
        self.assertIn(["cmd", "clipboard", "settext", "привет"], runner.calls)
        self.assertIn(["input", "keyevent", "279"], runner.calls)

    # --------------------------------------------------------------- uiautomator
    def test_ui_tree_parses_nodes_and_centers(self):
        def handler(argv):
            if argv[:2] == ["uiautomator", "dump"]:
                return RawRun(0, b"UI hierchary dumped to: /data/local/tmp/x.xml", "")
            if argv[:1] == ["cat"]:
                return RawRun(0, UI_XML, "")
            return RawRun(0, b"", "")

        backend, runner = self.backend(handler)
        result = backend.ui_tree()
        self.assertTrue(result["ok"])
        self.assertEqual(result["node_count"], 3)
        self.assertGreaterEqual(result["interactive_count"], 2)
        slot = next(node for node in result["interactive"] if node["id"].endswith("hud_slot_1"))
        self.assertEqual(slot["bounds"], [900, 2200, 1060, 2380])
        self.assertEqual(slot["center"], [980, 2290])
        self.assertTrue(slot["clickable"])
        self.assertEqual(runner.calls[0][0], "rm")
        self.assertEqual(runner.calls[1][:2], ["uiautomator", "dump"])

    def test_ui_tree_package_filter_keeps_matching_subtree(self):
        def handler(argv):
            if argv[:2] == ["uiautomator", "dump"]:
                return RawRun(0, b"UI hierchary dumped to: x", "")
            if argv[:1] == ["cat"]:
                return RawRun(0, UI_XML, "")
            return RawRun(0, b"", "")

        backend, _ = self.backend(handler)
        result = backend.ui_tree(package="org.wc3rust.p0")
        self.assertTrue(result["ok"])
        self.assertEqual(result["tree"]["pkg"], "org.wc3rust.p0")

    def test_ui_tree_retries_once_on_transient_idle_failure(self):
        calls = {"dump": 0}

        def handler(argv):
            if argv[:2] == ["uiautomator", "dump"]:
                calls["dump"] += 1
                if calls["dump"] == 1:
                    return RawRun(0, b"ERROR: could not get idle state.", "")
                return RawRun(0, b"UI hierchary dumped to: x", "")
            if argv[:1] == ["cat"]:
                return RawRun(0, UI_XML, "")
            return RawRun(0, b"", "")

        backend, runner = self.backend(handler)
        result = backend.ui_tree()
        self.assertTrue(result["ok"])
        self.assertEqual(calls["dump"], 2)

    def test_ui_tree_permission_error_surfaces_permission_code(self):
        def handler(argv):
            if argv[:2] == ["uiautomator", "dump"]:
                return RawRun(1, b"", "java.lang.SecurityException: permission denied")
            return RawRun(0, b"", "")

        backend, _ = self.backend(handler)
        with self.assertRaises(DeviceError) as ctx:
            backend.ui_tree()
        self.assertEqual(ctx.exception.code, "permission")
        self.assertIn("permission denied", ctx.exception.detail["stderr"])

    # ------------------------------------------------------------------ window
    def test_window_parse_focus_and_visibility(self):
        parsed = parse_dumpsys_window(DUMPSYS)
        self.assertEqual(parsed["focus"]["component"], "org.wc3rust.p0/org.libsdl.app.SDLActivity")
        self.assertEqual(parsed["focus"]["package"], "org.wc3rust.p0")
        self.assertEqual(parsed["focused_app"]["package"], "org.wc3rust.p0")
        self.assertEqual(parsed["focused_app"]["activity"], "org.wc3rust.p0.SDLActivity")
        self.assertEqual(len(parsed["visible_windows"]), 1)
        self.assertEqual(parsed["visible_windows"][0]["type"], "APPLICATION")
        self.assertEqual(parsed["visible_windows"][0]["bounds"], [0, 0, 1080, 2414])

    def test_window_parse_frames_fallback(self):
        parsed = parse_dumpsys_window(DUMPSYS.replace("mBounds=[0,0][1080,2414]", "Frames: parent=[0,0][1080,2414] display=[0,0][1080,2414] frame=[0,0][1080,2414]"))
        self.assertEqual(parsed["visible_windows"][0]["bounds"], [0, 0, 1080, 2414])

    def test_window_tool_uses_full_window_section(self):
        focus_only = "mCurrentFocus=Window{aa7f720 u0 org.wc3rust.p0/org.libsdl.app.SDLActivity}"

        def handler(argv):
            if argv == ["dumpsys", "window", "displays"]:
                return RawRun(0, focus_only.encode(), "")
            return RawRun(0, DUMPSYS.encode(), "")

        backend, runner = self.backend(handler)
        parsed = backend.window(max_windows=5)
        self.assertEqual(
            runner.calls,
            [["dumpsys", "window", "displays"], ["dumpsys", "window", "windows"]],
        )
        self.assertEqual(parsed["focus"]["package"], "org.wc3rust.p0")
        self.assertEqual(parsed["focus"]["activity"], "org.libsdl.app.SDLActivity")

    def test_window_focus_dump_failure_is_reported(self):
        def handler(argv):
            if argv == ["dumpsys", "window", "displays"]:
                return RawRun(1, b"", "permission denied")
            return RawRun(0, DUMPSYS.encode(), "")

        backend, _ = self.backend(handler)
        parsed = backend.window()
        self.assertEqual(parsed["focus_error"]["rc"], 1)
        self.assertEqual(len(parsed["visible_windows"]), 1)

    def test_am_start_parse(self):
        parsed = parse_am_start(AM_START)
        self.assertEqual(parsed["status"], "ok")
        self.assertEqual(parsed["total_time"], 145)
        self.assertEqual(parsed["activity"], "org.wc3rust.p0/org.libsdl.app.SDLActivity")

    def test_start_resolves_package_then_launches(self):
        def handler(argv):
            if argv[:3] == ["cmd", "package", "resolve-activity"]:
                return RawRun(0, b"priority=0\norg.wc3rust.p0/org.libsdl.app.SDLActivity\n", "")
            if argv[:2] == ["am", "start"]:
                return RawRun(0, AM_START.encode(), "")
            return RawRun(0, b"", "")

        backend, runner = self.backend(handler)
        result = backend.start("org.wc3rust.p0")
        self.assertEqual(result["component"], "org.wc3rust.p0/org.libsdl.app.SDLActivity")
        self.assertEqual(result["status"], "ok")
        self.assertTrue(result["dispatched"])
        self.assertEqual(runner.calls[-1][:2], ["am", "start"])

    # ---------------------------------------------------------------- apps
    def test_apps_filters_packages(self):
        payload = b"package:com.termux\npackage:org.wc3rust.p0\npackage:com.android.settings\n"

        backend, _ = self.backend(lambda argv: RawRun(0, payload, ""))
        result = backend.apps(filter_text="hero")
        self.assertEqual(result["packages"], ["org.wc3rust.p0"])
        self.assertEqual(result["total"], 3)
        self.assertEqual(result["matched"], 1)

    # ------------------------------------------------------------ screenshot
    def test_screenshot_writes_png_and_returns_dimensions(self):
        png = make_png(4, 6)

        def handler(argv):
            if argv[:2] == ["screencap", "-p"]:
                Path(argv[2]).write_bytes(png)
                return RawRun(0, b"", "")
            return RawRun(0, b"", "")

        backend, runner = self.backend(handler)
        result = backend.screenshot(str(self.state / "shot.png"))
        self.assertEqual((result["width"], result["height"]), (4, 6))
        self.assertEqual(result["bytes"], len(png))
        self.assertEqual(len(result["sha256"]), 64)
        self.assertEqual(result["_png"], png)
        self.assertEqual(runner.calls[1][:2], ["chmod", "0644"])

    def test_png_size_rejects_non_png(self):
        self.assertIsNone(png_size(b"not a png"))

    # --------------------------------------------------------------- analyze
    def test_analyze_metadata_without_ocr(self):
        png = make_png(5, 7)
        target = self.state / "img.png"
        target.write_bytes(png)
        backend, _ = self.backend(lambda argv: RawRun(0, b"", ""))
        result = backend.analyze(str(target), ocr=False)
        self.assertEqual(result["format"], "png")
        self.assertEqual((result["width"], result["height"]), (5, 7))
        self.assertEqual(result["bytes"], len(png))
        self.assertIsNone(result["answer"])
        self.assertIsNone(result["ocr"]["text"])

    def test_analyze_missing_file_raises_not_found(self):
        backend, _ = self.backend(lambda argv: RawRun(0, b"", ""))
        with self.assertRaises(DeviceError) as ctx:
            backend.analyze(str(self.state / "nope.png"))
        self.assertEqual(ctx.exception.code, "not_found")

    def test_analyze_caps_input_size(self):
        target = self.state / "big.bin"
        target.write_bytes(b"\0" * (10 * 1024 * 1024 + 1))
        backend, _ = self.backend(lambda argv: RawRun(0, b"", ""))
        with self.assertRaises(DeviceError) as ctx:
            backend.analyze(str(target))
        self.assertEqual(ctx.exception.code, "input_too_large")

    # --------------------------------------------------------------- errors
    def test_failed_su_maps_to_permission_with_stderr(self):
        backend, _ = self.backend(lambda argv: RawRun(1, b"", "su: permission denied"))
        with self.assertRaises(DeviceError) as ctx:
            backend.tap(1, 2)
        self.assertEqual(ctx.exception.code, "permission")
        self.assertIn("su: permission denied", ctx.exception.detail["stderr"])


if __name__ == "__main__":
    unittest.main()
