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

    def test_comfyui_http_redirects_are_rejected(self):
        import threading
        from http.server import BaseHTTPRequestHandler, HTTPServer
        from urllib import error, request

        class RedirectHandler(BaseHTTPRequestHandler):
            def do_GET(self):
                self.send_response(302)
                self.send_header("Location", "/redirect-target")
                self.end_headers()
            def log_message(self, *_args):
                pass

        server=HTTPServer(("127.0.0.1",0),RedirectHandler)
        thread=threading.Thread(target=server.serve_forever,daemon=True)
        thread.start()
        try:
            base=f"http://127.0.0.1:{server.server_port}"
            req=request.Request(base+"/")
            with self.assertRaises(error.HTTPError) as caught:
                adapter.open_http(req,base,2)
            self.assertEqual(caught.exception.code,302)
        finally:
            server.shutdown()
            thread.join(timeout=2)
            server.server_close()

    def test_read_json_rejects_symlink_inputs(self):
        import tempfile
        from pathlib import Path

        with tempfile.TemporaryDirectory() as directory:
            source=Path(directory)/"source.json"
            source.write_text("{}",encoding="utf-8")
            link=Path(directory)/"alias.json"
            link.symlink_to(source)
            with self.assertRaisesRegex(adapter.ComfyError,"non-symlink"):
                adapter.read_json(link)

    def test_workflow_sha256_pin_accepts_exact_file_and_rejects_drift(self):
        import hashlib
        import tempfile
        from pathlib import Path

        with tempfile.TemporaryDirectory() as directory:
            workflow = Path(directory) / "workflow.json"
            workflow.write_text('{"nodes":[]}', encoding="utf-8")
            digest = hashlib.sha256(workflow.read_bytes()).hexdigest()
            self.assertEqual(adapter.verify_workflow_sha256(workflow, digest), digest)
            workflow.write_text('{"nodes":[{"id":1}]}', encoding="utf-8")
            with self.assertRaisesRegex(adapter.ComfyError, "SHA-256 mismatch"):
                adapter.verify_workflow_sha256(workflow, digest)

    def test_workflow_sha256_pin_rejects_malformed_digest_and_symlink(self):
        import tempfile
        from pathlib import Path

        with tempfile.TemporaryDirectory() as directory:
            workflow = Path(directory) / "workflow.json"
            workflow.write_text("{}", encoding="utf-8")
            with self.assertRaisesRegex(adapter.ComfyError, "64 hexadecimal"):
                adapter.verify_workflow_sha256(workflow, "not-a-hash")
            link = Path(directory) / "link.json"
            link.symlink_to(workflow)
            with self.assertRaisesRegex(adapter.ComfyError, "non-symlink"):
                adapter.verify_workflow_sha256(link)

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

    def test_saved_canvas_workflow_converts_to_api_prompt(self):
        workflow = {
            "nodes": [
                {"id": 67, "type": "CLIPTextEncode", "mode": 0,
                 "inputs": [{"name": "clip", "link": 1}], "widgets_values": ["asset prompt"]},
                {"id": 69, "type": "ModelSamplingAuraFlow", "mode": 0,
                 "inputs": [{"name": "model", "link": 2}], "widgets_values": [3]},
                {"id": 70, "type": "KSampler", "mode": 0,
                 "inputs": [{"name": "model", "link": 3}, {"name": "positive", "link": 4},
                            {"name": "negative", "link": 5}, {"name": "latent_image", "link": 6}],
                 "widgets_values": [42, "fixed", 8, 1, "res_multistep", "simple", 1]},
                {"id": 9, "type": "SaveImage", "mode": 0,
                 "inputs": [{"name": "images", "link": 7}], "widgets_values": ["ComfyUI"]},
            ],
            "links": [
                [1, 62, 0, 67, 0, "CLIP"], [2, 66, 0, 69, 0, "MODEL"],
                [3, 69, 0, 70, 0, "MODEL"], [4, 67, 0, 70, 1, "CONDITIONING"],
                [5, 71, 0, 70, 2, "CONDITIONING"], [6, 68, 0, 70, 3, "LATENT"],
                [7, 65, 0, 9, 0, "IMAGE"],
            ],
        }
        object_info = {
            "CLIPTextEncode": {
                "input_order": {"required": ["text", "clip"]},
                "input": {"required": {"text": ["STRING", {"multiline": True}], "clip": ["CLIP", {"tooltip": "clip"}]}},
            },
            "ModelSamplingAuraFlow": {
                "input_order": {"required": ["model", "shift"], "optional": ["sampling"]},
                "input": {"required": {"model": ["MODEL", {"tooltip": "model"}],
                                       "shift": ["FLOAT", {"default": 1.73}]},
                          "optional": {"sampling": ["SAMPLING"]}},
            },
            "KSampler": {
                "input_order": {"required": ["model", "seed", "steps", "cfg", "sampler_name",
                                             "scheduler", "positive", "negative", "latent_image", "denoise"]},
                "input": {"required": {
                    "model": ["MODEL", {}], "seed": ["INT", {"control_after_generate": True}],
                    "steps": ["INT", {}], "cfg": ["FLOAT", {}],
                    "sampler_name": [["euler", "res_multistep"], {}], "scheduler": [["simple"], {}],
                    "positive": ["CONDITIONING", {}], "negative": ["CONDITIONING", {}],
                    "latent_image": ["LATENT", {}], "denoise": ["FLOAT", {}],
                }},
            },
            "SaveImage": {
                "input_order": {"required": ["images", "filename_prefix"]},
                "input": {"required": {"images": ["IMAGE", {}], "filename_prefix": ["STRING", {}]}},
            },
        }
        api = adapter.ui_workflow_to_api(workflow, object_info)
        self.assertEqual(api["67"]["inputs"]["text"], "asset prompt")
        self.assertEqual(api["69"]["inputs"], {"model": ["66", 0], "shift": 3})
        self.assertEqual(api["70"]["inputs"]["seed"], 42)
        self.assertEqual(api["70"]["inputs"]["steps"], 8)
        self.assertEqual(api["70"]["inputs"]["sampler_name"], "res_multistep")
        self.assertEqual(api["70"]["inputs"]["denoise"], 1)
        self.assertNotIn("sampling", api["69"]["inputs"])
        self.assertEqual(api["9"]["inputs"]["filename_prefix"], "ComfyUI")

    def test_canvas_converter_rejects_unmapped_widget_values(self):
        workflow = {"nodes": [{"id": 1, "type": "SaveImage", "mode": 0,
                               "inputs": [{"name": "images", "link": None}],
                               "widgets_values": ["prefix", "unexpected"]}], "links": []}
        info = {"SaveImage": {"input_order": {"required": ["images", "filename_prefix"]},
                              "input": {"required": {"images": ["IMAGE", {}],
                                                     "filename_prefix": ["STRING", {}]}}}}
        with self.assertRaisesRegex(adapter.ComfyError, "unmapped widget"):
            adapter.ui_workflow_to_api(workflow, info)

    def test_chroma_key_removes_only_border_connected_background(self):
        from PIL import Image, ImageDraw
        image = Image.new("RGBA", (32, 32), (255, 0, 255, 255))
        ImageDraw.Draw(image).rectangle((8, 8, 23, 23), fill=(30, 150, 60, 255))
        image.putpixel((15, 15), (255, 0, 255, 255))
        result = adapter.chroma_key_background(image, (255, 0, 255), 8)
        self.assertEqual(result.getpixel((0, 0))[3], 0)
        self.assertEqual(result.getpixel((10, 10))[3], 255)
        self.assertEqual(result.getpixel((15, 15))[3], 255)

    def test_frame_aperture_is_cut_out(self):
        from PIL import Image
        image = Image.new("RGBA", (100, 100), (120, 80, 30, 255))
        result = adapter.enforce_frame_aperture(image)
        self.assertEqual(result.getpixel((50, 50))[3], 0)
        self.assertEqual(result.getpixel((10, 10))[3], 255)


if __name__ == "__main__":
    unittest.main()
