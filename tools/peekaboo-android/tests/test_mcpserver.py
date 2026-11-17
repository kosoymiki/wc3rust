"""MCP protocol tests: dispatch, guard, result shape, stdio round-trip."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from peekaboo_android.backend import AndroidBackend, RawRun
from peekaboo_android.guard import DeviceSlotGuard
from peekaboo_android.mcpserver import McpServer, TOOL_DEFINITIONS, main

sys.path.insert(0, str(Path(__file__).resolve().parent))
from test_backend import FakeRunner, make_png  # noqa: E402

TOOL_DIR = Path(__file__).resolve().parent.parent


class ServerTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.state = Path(self.tmp.name) / "state"
        self.saved_slot = os.environ.pop("H2_DEVICE_SLOT", None)

    def tearDown(self):
        if self.saved_slot is not None:
            os.environ["H2_DEVICE_SLOT"] = self.saved_slot
        os.environ.pop("H2_DEVICE_SLOT", None)
        self.tmp.cleanup()

    def build(self, handler=None):
        handler = handler or (lambda argv: RawRun(0, b"", ""))
        runner = FakeRunner(handler)
        backend = AndroidBackend(runner=runner, state_dir=self.state)
        return McpServer(backend=backend, guard=DeviceSlotGuard(self.state)), runner

    # ---------------------------------------------------------------- catalog
    def test_tools_list_has_contract_tools(self):
        server, _ = self.build()
        response = server.handle({"jsonrpc": "2.0", "id": 1, "method": "tools/list"})
        names = [tool["name"] for tool in response["result"]["tools"]]
        self.assertEqual(
            names,
            ["screenshot", "ui_tree", "tap", "swipe", "long_press", "type", "key", "window", "apps", "start", "analyze", "sleep"],
        )
        for tool in TOOL_DEFINITIONS:
            self.assertIn("description", tool)
            self.assertEqual(tool["inputSchema"]["type"], "object")

    def test_initialize_echoes_supported_protocol(self):
        server, _ = self.build()
        response = server.handle(
            {"jsonrpc": "2.0", "id": 0, "method": "initialize",
             "params": {"protocolVersion": "2024-11-05", "clientInfo": {"name": "t"}}}
        )
        self.assertEqual(response["result"]["protocolVersion"], "2024-11-05")
        self.assertEqual(response["result"]["serverInfo"]["name"], "peekaboo-android")
        self.assertIn("tools", response["result"]["capabilities"])

    def test_unknown_method_is_jsonrpc_error(self):
        server, _ = self.build()
        response = server.handle({"jsonrpc": "2.0", "id": 7, "method": "bogus/method"})
        self.assertEqual(response["error"]["code"], -32601)

    def test_notification_gets_no_response(self):
        server, _ = self.build()
        self.assertIsNone(server.handle({"jsonrpc": "2.0", "method": "notifications/initialized"}))

    # ------------------------------------------------------------------ guard
    def test_action_tool_refused_without_slot_and_not_dispatched(self):
        server, runner = self.build()
        result = server.dispatch("tap", {"x": 10, "y": 20})
        self.assertFalse(result["ok"])
        self.assertEqual(result["error"]["code"], "permission")
        self.assertFalse(result["dispatched"])
        self.assertEqual(runner.calls, [])

    def test_read_tool_allowed_without_slot(self):
        payload = b"package:com.termux\n"

        def handler(argv):
            return RawRun(0, payload, "")

        server, runner = self.build(handler)
        result = server.dispatch("apps", {"filter": "termux"})
        self.assertTrue(result["ok"])
        self.assertEqual(result["packages"], ["com.termux"])

    def test_action_tool_runs_with_slot(self):
        os.environ["H2_DEVICE_SLOT"] = "1"
        server, runner = self.build()
        result = server.dispatch("tap", {"x": 10, "y": 20})
        self.assertTrue(result["ok"])
        self.assertEqual(runner.calls, [["input", "tap", "10", "20"]])

    def test_invalid_argument_is_rejected_before_dispatch(self):
        os.environ["H2_DEVICE_SLOT"] = "1"
        server, runner = self.build()
        result = server.dispatch("tap", {"x": "10", "y": 20})
        self.assertEqual(result["error"]["code"], "invalid_argument")
        self.assertEqual(runner.calls, [])

    # ------------------------------------------------------------- audit log
    def test_every_call_is_logged(self):
        os.environ["H2_DEVICE_SLOT"] = "1"
        server, _ = self.build()
        server.dispatch("tap", {"x": 10, "y": 20})
        server.dispatch("tap", {"x": 11, "y": 21})
        lines = (self.state / "actions.log").read_text(encoding="utf-8").strip().splitlines()
        self.assertEqual(len(lines), 2)
        entry = json.loads(lines[0])
        self.assertEqual(entry["tool"], "tap")
        self.assertEqual(entry["args"], {"x": 10, "y": 20})
        self.assertTrue(entry["result"]["ok"])
        self.assertTrue(entry["slot"])

    def test_long_text_is_truncated_in_log(self):
        os.environ["H2_DEVICE_SLOT"] = "1"
        server, _ = self.build()
        server.dispatch("type", {"text": "x" * 500})
        entry = json.loads((self.state / "actions.log").read_text(encoding="utf-8").splitlines()[0])
        self.assertEqual(len(entry["args"]["text"]), 201)
        self.assertTrue(entry["args"]["text"].endswith("…"))

    # ------------------------------------------------------------ result shape
    def test_screenshot_result_has_image_block(self):
        png = make_png(3, 5)

        def handler(argv):
            if argv[:2] == ["screencap", "-p"]:
                Path(argv[2]).write_bytes(png)
            return RawRun(0, b"", "")

        server, _ = self.build(handler)
        response = server.handle(
            {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "screenshot", "arguments": {}}}
        )
        content = response["result"]["content"]
        self.assertEqual(content[0]["type"], "text")
        payload = json.loads(content[0]["text"])
        self.assertEqual((payload["width"], payload["height"]), (3, 5))
        self.assertNotIn("_png", payload)
        self.assertEqual(content[1]["type"], "image")
        self.assertEqual(content[1]["mimeType"], "image/png")
        self.assertFalse(response["result"]["isError"])

    def test_tool_error_is_marked_isError(self):
        server, _ = self.build()
        response = server.handle(
            {"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "tap", "arguments": {"x": 1, "y": 2}}}
        )
        self.assertTrue(response["result"]["isError"])
        error = json.loads(response["result"]["content"][0]["text"])["error"]
        self.assertEqual(error["code"], "permission")

    def test_unknown_tool_reports_unknown_tool(self):
        server, _ = self.build()
        result = server.dispatch("nope", {})
        self.assertEqual(result["error"]["code"], "unknown_tool")

    # ---------------------------------------------------------------- stdio
    def test_stdio_round_trip(self):
        env = dict(os.environ)
        env.pop("H2_DEVICE_SLOT", None)
        env["PEEKABOO_ANDROID_STATE_DIR"] = str(self.state)
        proc = subprocess.run(
            [sys.executable, str(TOOL_DIR / "server.py")],
            input="\n".join(
                [
                    json.dumps({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
                    json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}),
                    json.dumps({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
                    json.dumps({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                                "params": {"name": "tap", "arguments": {"x": 1, "y": 2}}}),
                ]
            )
            + "\n",
            capture_output=True,
            text=True,
            timeout=30,
            env=env,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        responses = [json.loads(line) for line in proc.stdout.splitlines() if line.strip()]
        self.assertEqual([item["id"] for item in responses], [1, 2, 3])
        self.assertEqual(len(responses[1]["result"]["tools"]), 12)
        self.assertTrue(responses[2]["result"]["isError"])
        self.assertIn("peekaboo-android", proc.stderr)


if __name__ == "__main__":
    unittest.main()
