import unittest

import numpy as np
from PIL import Image

import alpha_matte


class AlphaMatteTests(unittest.TestCase):
    def test_prepare_input_contract(self):
        image = Image.new("RGB", (64, 32), (128, 128, 128))
        prepared = alpha_matte.prepare_input(image)
        self.assertEqual(prepared.shape, (1, 3, 320, 320))
        self.assertEqual(prepared.dtype, np.float32)
        self.assertTrue(np.isfinite(prepared).all())
        expected_red = (128 / 255 - alpha_matte.MEAN[0]) / alpha_matte.STD[0]
        self.assertAlmostEqual(float(prepared[0, 0, 0, 0]), expected_red, places=4)

    def test_prepare_input_respects_custom_size(self):
        prepared = alpha_matte.prepare_input(Image.new("RGB", (8, 8), (0, 0, 0)), size=64)
        self.assertEqual(prepared.shape, (1, 3, 64, 64))

    def test_mask_from_prediction_normalizes_and_resizes(self):
        pred = np.array([[0.0, 1.0], [2.0, 3.0]], dtype=np.float32)
        mask = alpha_matte.mask_from_prediction(pred, (8, 8))
        self.assertEqual(mask.size, (8, 8))
        self.assertEqual(mask.mode, "L")
        self.assertEqual(mask.getextrema(), (0, 255))

    def test_constant_mask_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "constant mask"):
            alpha_matte.mask_from_prediction(np.zeros((4, 4), dtype=np.float32), (4, 4))

    def test_matte_rgba_applies_the_mask(self):
        class _Input:
            name = "input.1"

        class _Session:
            def get_inputs(self):
                return [_Input()]

            def run(self, outputs, feed):
                self.assert_feed = feed
                ramp = np.linspace(0.0, 1.0, 320 * 320, dtype=np.float32)
                return [ramp.reshape(1, 1, 320, 320)]

        session = _Session()
        image = Image.new("RGB", (96, 96), (10, 20, 30))
        out = alpha_matte.matte_rgba(image, session=session)
        self.assertEqual(out.size, (96, 96))
        self.assertEqual(out.mode, "RGBA")
        self.assertEqual(out.getpixel((0, 0))[:3], (10, 20, 30))
        low, high = out.getchannel("A").getextrema()
        self.assertEqual(low, 0)
        self.assertGreaterEqual(high, 250)  # Lanczos resize may stop at 254
        self.assertIn("input.1", session.assert_feed)

    def test_missing_model_is_reported(self):
        with self.assertRaises(FileNotFoundError):
            alpha_matte.get_session("/nonexistent/u2net.onnx")


if __name__ == "__main__":
    unittest.main()
