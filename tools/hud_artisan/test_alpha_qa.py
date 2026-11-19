import tempfile
import unittest
from pathlib import Path

from PIL import Image, ImageDraw

import alpha_qa


class AlphaQATests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)

    def tearDown(self):
        self.temp.cleanup()

    def save(self, name, image):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        image.save(path, format="PNG")
        return path

    def test_transparent_frame_is_measured_without_mutation(self):
        image = Image.new("RGBA", (64, 64), (120, 80, 30, 255))
        ImageDraw.Draw(image).rectangle((20, 20, 43, 43), fill=(0, 0, 0, 0))
        path = self.save("frame.png", image)
        before = alpha_qa._sha256(path)
        item = alpha_qa.inspect_png(path, self.root)
        self.assertGreater(item["alpha"]["transparent_ratio"], 0)
        self.assertEqual(item["alpha"]["bbox"], [0, 0, 64, 64])
        self.assertEqual(alpha_qa._sha256(path), before)

    def test_opaque_image_flags_missing_required_alpha(self):
        path = self.save("opaque.png", Image.new("RGB", (16, 16), (20, 30, 40)))
        item = alpha_qa.inspect_png(path, self.root)
        self.assertIn("alpha_required_but_opaque", alpha_qa.classify(item, True, 0.01))

    def test_magenta_fringe_is_detected(self):
        image = Image.new("RGBA", (16, 16), (40, 90, 50, 255))
        image.putpixel((8, 8), (255, 0, 255, 255))
        item = alpha_qa.inspect_png(self.save("fringe.png", image), self.root)
        self.assertIn("possible_chroma_key_fringe", alpha_qa.classify(item, False, 0.001))

    def test_key_tolerance_above_85_does_not_overflow_sum_distance(self):
        from PIL import Image
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            path=root/"black.png"
            Image.new("RGBA",(4,4),(0,0,0,255)).save(path)
            report=alpha_qa.inspect_png(path,root,key_tolerance=100)
            self.assertEqual(report["chroma_key"]["near_key_visible_pixels"],0)

    def test_fully_transparent_image_is_flagged(self):
        item = alpha_qa.inspect_png(
            self.save("empty.png", Image.new("RGBA", (8, 8), (0, 0, 0, 0))), self.root)
        self.assertIn("fully_transparent", alpha_qa.classify(item, False, 0.01))

    def test_semi_transparent_magenta_and_hidden_rgb_are_reported(self):
        image = Image.new("RGBA", (8, 8), (0, 0, 0, 0))
        image.putpixel((2, 2), (255, 0, 255, 96))
        image.putpixel((3, 3), (20, 30, 40, 0))
        item = alpha_qa.inspect_png(self.save("halo.png", image), self.root)
        self.assertEqual(item["chroma_key"]["near_key_semi_transparent_pixels"], 1)
        self.assertEqual(item["chroma_key"]["transparent_pixels_with_hidden_rgb"], 1)

    def test_build_report_counts_errors_and_flags(self):
        self.save("good.png", Image.new("RGBA", (8, 8), (10, 20, 30, 255)))
        self.save("bad.png", Image.new("RGBA", (8, 8), (0, 0, 0, 0)))
        report = alpha_qa.build_report(list(self.root.glob("*.png")), self.root)
        self.assertEqual(report["summary"]["scanned"], 2)
        self.assertEqual(report["summary"]["flagged"], 1)
        self.assertFalse(report["summary"]["pass"])

    def test_invalid_ratio_rejected(self):
        with self.assertRaises(ValueError):
            alpha_qa.build_report([], self.root, max_key_ratio=1.1)

    def test_symlink_input_is_rejected(self):
        target = self.save("target.png", Image.new("RGBA", (4, 4), (1, 2, 3, 255)))
        link = self.root / "linked.png"
        link.symlink_to(target)
        with self.assertRaisesRegex(ValueError, "non-symlink"):
            alpha_qa.inspect_png(link, self.root)


if __name__ == "__main__":
    unittest.main()
