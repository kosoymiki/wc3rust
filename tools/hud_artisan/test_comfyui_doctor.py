import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import comfyui_doctor as doctor


class ComfyDoctorTests(unittest.TestCase):
    def fixture(self, root):
        workflow = {
            "6": {"class_type": "CLIPTextEncode", "inputs": {"text": "old-positive"}},
            "7": {"class_type": "CLIPTextEncode", "inputs": {"text": "old-negative"}},
            "5": {"class_type": "EmptySD3LatentImage", "inputs": {"width": 512, "height": 512, "batch_size": 1}},
            "3": {"class_type": "KSampler", "inputs": {"seed": 42}},
            "9": {"class_type": "SaveImage", "inputs": {"images": ["8", 0]}},
        }
        workflow_path = root / "workflow.json"
        workflow_path.write_text(json.dumps(workflow))
        config = {
            "schema_version": 1,
            "server": "http://127.0.0.1:8188",
            "workflow_file": str(workflow_path),
            "workflow_format": "api",
            "node_inputs": {
                "positive_prompt": {"node_id": "6", "input": "text"},
                "negative_prompt": {"node_id": "7", "input": "text"},
                "width": {"node_id": "5", "input": "width"},
                "height": {"node_id": "5", "input": "height"},
                "seed": {"node_id": "3", "input": "seed"},
            },
            "output_node_id": "9",
        }
        config_path = root / "comfyui.json"
        config_path.write_text(json.dumps(config))
        object_info = {
            "CLIPTextEncode": {"input": {"required": {"text": ["STRING"]}}},
            "EmptySD3LatentImage": {"input": {"required": {
                "width": ["INT"], "height": ["INT"], "batch_size": ["INT"]}}},
            "KSampler": {"input": {"required": {"seed": ["INT"]}}},
            "SaveImage": {"input": {"required": {"images": ["IMAGE"]}}},
        }
        return config_path, object_info

    def test_read_only_doctor_accepts_working_api_workflow(self):
        with tempfile.TemporaryDirectory() as directory:
            config_path, object_info = self.fixture(Path(directory))
            def fake_http(base, endpoint, payload=None, timeout=60):
                if endpoint == "/system_stats":
                    return {"system": {"comfyui_version": "0.46.0", "python_version": "3.14",
                                       "pytorch_version": "2.15", "os": "android",
                                       "ram_total": 1000, "ram_free": 500},
                            "devices": [{"name": "cpu", "type": "cpu"}]}
                if endpoint == "/queue":
                    return {"queue_running": [], "queue_pending": []}
                if endpoint == "/object_info":
                    return object_info
                raise AssertionError(endpoint)
            with mock.patch.object(doctor.adapter, "http_json", side_effect=fake_http):
                report = doctor.inspect_runtime(config_path)
            self.assertEqual(report["status"], "PASS")
            self.assertEqual(report["comfyui"]["version"], "0.46.0")
            self.assertEqual(report["workflow"]["api_node_count"], 5)
            self.assertEqual(report["queue"], {"running": 0, "pending": 0})
            self.assertTrue(report["read_only"])

    def test_doctor_rejects_symlinked_workflow_before_hashing(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            config_path, object_info=self.fixture(root)
            workflow_path=root/"workflow.json"
            alias=root/"workflow-link.json"
            alias.symlink_to(workflow_path)
            config=json.loads(config_path.read_text())
            config["workflow_file"]=str(alias)
            config_path.write_text(json.dumps(config))
            def fake_http(base, endpoint, payload=None, timeout=60):
                if endpoint=="/system_stats":
                    return {"system":{},"devices":[]}
                if endpoint=="/queue":
                    return {"queue_running":[],"queue_pending":[]}
                if endpoint=="/object_info":
                    return object_info
                raise AssertionError(endpoint)
            with mock.patch.object(doctor.adapter,"http_json",side_effect=fake_http):
                with self.assertRaisesRegex(doctor.adapter.ComfyError,"non-symlink"):
                    doctor.inspect_runtime(config_path)

    def test_doctor_reports_bad_mapping_without_submitting_jobs(self):
        with tempfile.TemporaryDirectory() as directory:
            config_path, object_info = self.fixture(Path(directory))
            config = json.loads(config_path.read_text())
            config["node_inputs"]["seed"]["input"] = "missing_seed"
            config_path.write_text(json.dumps(config))
            def fake_http(base, endpoint, payload=None, timeout=60):
                if endpoint == "/system_stats":
                    return {"system": {}, "devices": []}
                if endpoint == "/queue":
                    return {"queue_running": [], "queue_pending": []}
                if endpoint == "/object_info":
                    return object_info
                raise AssertionError("doctor must not submit or poll a prompt")
            with mock.patch.object(doctor.adapter, "http_json", side_effect=fake_http):
                report = doctor.inspect_runtime(config_path)
            self.assertEqual(report["status"], "FAIL")
            self.assertTrue(any("mapped input" in item for item in report["errors"]))

    def test_dynamic_model_sentinel_and_active_queue_are_warnings(self):
        with tempfile.TemporaryDirectory() as directory:
            config_path, object_info = self.fixture(Path(directory))
            workflow_path = Path(directory) / "workflow.json"
            workflow = json.loads(workflow_path.read_text())
            workflow["63"] = {"class_type": "VAELoader", "inputs": {"vae_name": "ae.safetensors"}}
            workflow_path.write_text(json.dumps(workflow))
            object_info["VAELoader"] = {"input": {"required": {"vae_name": [["pixel_space"]]}}}
            def fake_http(base, endpoint, payload=None, timeout=60):
                if endpoint == "/system_stats":
                    return {"system": {"comfyui_version": "0.46.0"}, "devices": []}
                if endpoint == "/queue":
                    return {"queue_running": [["active-prompt"]], "queue_pending": []}
                if endpoint == "/object_info":
                    return object_info
                raise AssertionError(endpoint)
            with mock.patch.object(doctor.adapter, "http_json", side_effect=fake_http):
                report = doctor.inspect_runtime(config_path)
            self.assertEqual(report["status"], "PASS")
            self.assertEqual(report["queue"]["running"], 1)
            self.assertTrue(any("no concrete model enumeration" in item for item in report["warnings"]))
            self.assertTrue(any("queue is active" in item for item in report["warnings"]))


if __name__ == "__main__":
    unittest.main()
