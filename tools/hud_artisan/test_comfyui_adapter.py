import unittest
from unittest import mock

import comfyui_adapter as adapter


class ComfyAdapterTests(unittest.TestCase):
    def test_loopback_http_and_remote_https_policy(self):
        self.assertEqual(adapter.validate_server_url("http://127.0.0.1:8188"), "http://127.0.0.1:8188")
        self.assertEqual(adapter.validate_server_url("https://comfy.example:8188"), "https://comfy.example:8188")
        with self.assertRaisesRegex(adapter.ComfyError, "must use HTTPS"):
            adapter.validate_server_url("http://comfy.example:8188")

    def test_server_url_rejects_embedded_credentials_and_paths(self):
        for value in ("http://user:pass@127.0.0.1:8188", "http://127.0.0.1:8188/api"):
            with self.subTest(value=value):
                with self.assertRaises(adapter.ComfyError):
                    adapter.validate_server_url(value)

    def test_job_dimensions_use_total_pixel_budget(self):
        job = {
            "job_id": "0123456789abcdef", "prompt": "a panel", "negative_prompt": "text",
            "width": 8192, "height": 1, "seed": 42,
        }
        adapter.validate_job(job)
        job["height"] = 8192
        with self.assertRaisesRegex(adapter.ComfyError, "pixel range"):
            adapter.validate_job(job)

    def test_job_rejects_boolean_dimensions_and_seed(self):
        job = {
            "job_id": "0123456789abcdef", "prompt": "a panel", "negative_prompt": "text",
            "width": True, "height": 64, "seed": 42,
        }
        with self.assertRaises(adapter.ComfyError):
            adapter.validate_job(job)
        job["width"] = 64
        job["seed"] = True
        with self.assertRaisesRegex(adapter.ComfyError, "unsigned 32-bit"):
            adapter.validate_job(job)

    @mock.patch.object(adapter, "http_json")
    def test_wait_for_output_requires_completed_saveimage(self, http_json):
        http_json.return_value = {
            "prompt-1": {
                "status": {"status_str": "success", "completed": True},
                "outputs": {"9": {"images": [{"filename": "result.png", "type": "output"}]}},
            }
        }
        result = adapter.wait_for_output("http://127.0.0.1:8188", "prompt-1", "9", 5, 5, 0.2)
        self.assertEqual(result["filename"], "result.png")

    @mock.patch.object(adapter, "http_json")
    def test_wait_for_output_rejects_multiple_images(self, http_json):
        http_json.return_value = {
            "prompt-1": {
                "status": {"status_str": "success", "completed": True},
                "outputs": {"9": {"images": [
                    {"filename": "a.png", "type": "output"},
                    {"filename": "b.png", "type": "output"},
                ]}},
            }
        }
        with self.assertRaisesRegex(adapter.ComfyError, "exactly one"):
            adapter.wait_for_output("http://127.0.0.1:8188", "prompt-1", "9", 5, 5, 0.2)

    @mock.patch.object(adapter, "http_json")
    def test_wait_for_output_surfaces_comfy_execution_error(self, http_json):
        http_json.return_value = {
            "prompt-1": {"status": {"status_str": "error", "messages": [["execution_error", {}]]}}
        }
        with self.assertRaisesRegex(adapter.ComfyError, "generation failed"):
            adapter.wait_for_output("http://127.0.0.1:8188", "prompt-1", "9", 5, 5, 0.2)

    def test_image_download_rejects_traversal_before_network(self):
        for metadata in (
            {"filename": "../escape.png", "type": "output"},
            {"filename": "image.png", "type": "output", "subfolder": "../escape"},
            {"filename": "image.png", "type": "temp"},
        ):
            with self.subTest(metadata=metadata):
                with self.assertRaises(adapter.ComfyError):
                    adapter.download_image("http://127.0.0.1:8188", metadata, 1)


if __name__ == "__main__":
    unittest.main()
