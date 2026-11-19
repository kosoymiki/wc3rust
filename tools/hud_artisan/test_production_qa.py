import json
import tempfile
import unittest
from pathlib import Path

from PIL import Image, ImageDraw

import production_qa as qa


class ProductionQATests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.pack = self.root / "pack"
        self.pack.mkdir()

    def tearDown(self):
        self.tmp.cleanup()

    def make_state(self, name, inset, color):
        im = Image.new("RGBA", (64, 64), (0, 0, 0, 0))
        ImageDraw.Draw(im).rounded_rectangle((inset, inset, 63-inset, 63-inset),
                                             radius=8, fill=color)
        path = self.pack / f"{name}.png"
        im.save(path)
        return {"name": name, "file": f"{name}.png", "w": 64, "h": 64,
                "states": [name.rsplit("_", 1)[-1]], "insets": {"l": 4, "t": 4, "r": 4, "b": 4}}

    def test_state_key_and_pair_metrics(self):
        self.assertEqual(qa.state_key("button_border_pressed_64"), ("button_border_64", "pressed"))
        a = self.make_state("button_normal", 5, (200, 140, 40, 255))
        b = self.make_state("button_pressed", 7, (120, 70, 20, 255))
        metrics = qa.state_pair_metrics(self.pack/a["file"], self.pack/b["file"])
        self.assertGreater(metrics["silhouette_iou"], 0.45)
        self.assertGreater(metrics["mean_visible_difference"], 0.01)

    def test_checker_composite_and_three_contact_sheets(self):
        a = self.make_state("icon_scout_normal", 8, (20, 180, 70, 255))
        paths = qa.render_contact_sheets([a], self.pack, self.root/"sheets")
        self.assertEqual(len(paths), 3)
        self.assertTrue(all(Path(path).is_file() for path in paths))

    def test_run_writes_state_pair_and_transparency_findings(self):
        a = self.make_state("button_normal", 6, (180, 130, 40, 255))
        b = self.make_state("button_pressed", 6, (180, 130, 40, 255))
        empty = Image.new("RGBA", (16, 16), (0, 0, 0, 0))
        empty.save(self.pack/"empty.png")
        manifest = {"assets": [a, b, {"name":"empty_icon", "file":"empty.png", "w":16, "h":16}]}
        mp = self.root/"manifest.json"
        mp.write_text(json.dumps(manifest))
        report = qa.run(mp, self.pack, self.root/"out", None, 0.45, 0.015, 0.45)
        self.assertEqual(report["summary"]["state_families"], 1)
        self.assertIn("button_normal", [x["asset"] for x in report["regeneration_candidates"]])
        self.assertIn("empty_icon", [x["asset"] for x in report["regeneration_candidates"]])

    def test_nine_slice_stretch_zone_metrics(self):
        image = Image.new("RGBA", (64, 64), (20, 30, 40, 255))
        draw = ImageDraw.Draw(image)
        draw.rectangle((8, 0, 55, 7), fill=(60, 70, 80, 255))
        path = self.pack/"panel.png"
        image.save(path)
        metrics = qa.nine_slice_metrics(path, {"l":8,"t":8,"r":8,"b":8})
        self.assertTrue(metrics["sliceable"])
        self.assertIn("top_stretch", metrics["stretch_zones"])
        self.assertIsNotNone(metrics["stretch_zones"]["top_stretch"]["mean_neighbor_gradient"])

    def test_symlink_asset_rejected_as_read_error(self):
        target = self.pack/"target.png"
        Image.new("RGBA", (8,8), (1,2,3,255)).save(target)
        (self.pack/"linked.png").symlink_to(target)
        manifest = {"assets":[{"name":"linked","file":"linked.png","w":8,"h":8}]}
        mp=self.root/"manifest.json"; mp.write_text(json.dumps(manifest))
        report=qa.run(mp,self.pack,self.root/"out",None,0.45,0.015,0.45)
        self.assertEqual(report["summary"]["read_errors"],1)


if __name__ == "__main__":
    unittest.main()
