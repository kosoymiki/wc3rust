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
        self.assertTrue(Path(report["review_gallery"]).is_file())
        gallery_text=Path(report["review_gallery"]).read_text()
        self.assertIn("button_normal",gallery_text)
        self.assertIn("button_normal", [x["asset"] for x in report["regeneration_candidates"]])
        self.assertIn("empty_icon", [x["asset"] for x in report["regeneration_candidates"]])
        empty_record=next(x for x in report["assets"] if x["name"]=="empty_icon")
        self.assertIn("fully_transparent",empty_record["alpha_qa"]["findings"])
        empty_candidate=next(x for x in report["regeneration_candidates"] if x["asset"]=="empty_icon")
        self.assertEqual(empty_candidate["reason"],["fully_transparent"])

    def test_atlas_cell_alpha_audit_and_repair_job_mapping(self):
        atlas = Image.new("RGBA", (16, 8), (0, 0, 0, 0))
        ImageDraw.Draw(atlas).rectangle((1, 1, 6, 6), fill=(210, 130, 35, 255))
        atlas.save(self.pack/"atlas.png")
        manifest = {"atlas":{"commandbuttons":{"file":"atlas.png","entries":[
            {"id":"visible","x":0,"y":0,"w":8,"h":8},
            {"id":"empty","x":8,"y":0,"w":8,"h":8}
        ]}}}
        plan = {"jobs":[{"asset":"command_icon_empty","job_id":"0123456789abcdef",
                         "output":"atlas_sources/empty.png"}],
                "atlas":{"cells":[{"id":"empty","source_file":"atlas_sources/empty.png"}]}}
        report = qa.atlas_qa(manifest,self.pack,self.root/"out",plan)
        self.assertEqual(report["status"],"ok")
        self.assertEqual(report["scanned"],2)
        self.assertEqual(report["findings"][0]["reason"],"fully_transparent_atlas_cell")
        self.assertEqual(report["findings"][0]["asset"],"command_icon_empty")
        self.assertTrue(Path(report["contact_sheet"]).is_file())

    def test_atlas_source_asset_maps_findings_without_jobs_plan(self):
        atlas=Image.new("RGBA",(16,8),(0,0,0,0))
        ImageDraw.Draw(atlas).rectangle((8,0,15,7),fill=(200,120,30,255))
        atlas.save(self.pack/"atlas.png")
        manifest={"assets":[{"name":"command_icon_01","file":"source.png","w":8,"h":8}],
            "atlas":{"commandbuttons":{"file":"atlas.png","cell":[8,8],"entries":[
                {"id":"command_01","x":0,"y":0,"w":8,"h":8,"source_asset":"command_icon_01"},
                {"id":"command_02","x":8,"y":0,"w":8,"h":8}
            ]}}}
        Image.new("RGBA",(8,8),(0,0,0,0)).save(self.pack/"source.png")
        report=qa.atlas_qa(manifest,self.pack,self.root/"atlas-source",None)
        self.assertEqual(report["findings"][0]["asset"],"command_icon_01")
        self.assertEqual(report["cells"][0]["source_asset"],"command_icon_01")

    def test_atlas_cell_geometry_must_match_manifest(self):
        atlas=Image.new("RGBA",(16,8),(100,100,100,255));atlas.save(self.pack/"atlas.png")
        report=qa.atlas_qa({"atlas":{"commandbuttons":{"file":"atlas.png","cell":[8,8],"entries":[
            {"id":"wrong-size","x":0,"y":0,"w":16,"h":8}
        ]}}},self.pack,self.root/"atlas-size",None)
        self.assertEqual(report["scanned"],0)
        self.assertIn("differ from declared cell",report["findings"][0]["reason"])

    def test_offline_gallery_escapes_names_and_uses_relative_assets(self):
        out=self.root/"gallery"
        thumbs=out/"thumbnails";thumbs.mkdir(parents=True)
        Image.new("RGB",(16,16),(80,90,100)).save(thumbs/"safe.png")
        gallery=qa.write_review_gallery(
            [{"name":"<script>alert(1)</script>","width":16,"height":16,
              "alpha_qa":{"alpha":{"transparent_ratio":0.0,"semi_transparent_ratio":0.0},"findings":[]}}],
            {"<script>alert(1)</script>":"thumbnails/safe.png"},[],
            {"status":"not_configured","scanned":0},
            {"scanned":1,"errors":[],"summary":{"state_families":0,"flagged_assets":0}},out)
        html=Path(gallery).read_text()
        self.assertIn("&lt;script&gt;alert(1)&lt;/script&gt;",html)
        self.assertNotIn("<script>alert(1)</script>",html)
        self.assertIn('src="thumbnails/safe.png"',html)

    def test_state_key_uses_rightmost_state_token(self):
        self.assertEqual(qa.state_key("button_active_disabled"),("button_active","disabled"))
        self.assertEqual(qa.state_key("ability_icon_pressed"),("ability_icon","pressed"))

    def test_atlas_overlap_is_reported(self):
        Image.new("RGBA",(16,8),(0,0,0,0)).save(self.pack/"atlas.png")
        report=qa.atlas_qa({"atlas":{"commandbuttons":{"file":"atlas.png","entries":[
            {"id":"one","x":0,"y":0,"w":10,"h":8},
            {"id":"two","x":8,"y":0,"w":8,"h":8}
        ]}}},self.pack,self.root/"atlas-out",None)
        self.assertEqual(report["scanned"],1)
        self.assertTrue(any("overlaps previously declared cell" in item["reason"] for item in report["findings"]))

    def test_empty_unmapped_atlas_cell_cannot_false_pass_production_qa(self):
        opaque=self.make_state("plain_icon",4,(140,100,40,255))
        Image.new("RGBA",(8,8),(0,0,0,0)).save(self.pack/"atlas.png")
        mp=self.root/"manifest.json"
        mp.write_text(json.dumps({"assets":[opaque],"atlas":{"commandbuttons":{"file":"atlas.png","entries":[
            {"id":"empty","x":0,"y":0,"w":8,"h":8}
        ]}}}))
        report=qa.run(mp,self.pack,self.root/"qa-atlas",None,0.45,0.015,0.45)
        self.assertEqual(report["summary"]["atlas_findings"],1)
        self.assertFalse(report["summary"]["pass"])

    def test_manifest_dimension_mismatch_is_a_read_error(self):
        image=Image.new("RGBA",(64,64),(120,80,30,255));image.save(self.pack/"wrong.png")
        manifest={"assets":[{"name":"plain_icon","file":"wrong.png","w":32,"h":32}]}
        mp=self.root/"manifest.json";mp.write_text(json.dumps(manifest))
        report=qa.run(mp,self.pack,self.root/"dimension-qa",None,0.45,0.015,0.45)
        self.assertEqual(report["summary"]["read_errors"],1)
        self.assertFalse(report["summary"]["pass"])
        self.assertIn("differ from manifest",report["errors"][0]["error"])

    def test_alpha_requirements_are_inferred_from_asset_semantics(self):
        opaque=Image.new("RGBA",(64,64),(120,80,30,255));opaque.save(self.pack/"icon.png")
        frame=Image.new("RGBA",(64,64),(0,0,0,0))
        ImageDraw.Draw(frame).rectangle((20,20,44,44),fill=(140,110,70,255))
        frame.save(self.pack/"portrait.png")
        manifest={"assets":[
            {"name":"icon_test_normal","file":"icon.png","w":64,"h":64},
            {"name":"portrait_frame_normal_64","file":"portrait.png","w":64,"h":64}
        ]}
        mp=self.root/"manifest.json";mp.write_text(json.dumps(manifest))
        report=qa.run(mp,self.pack,self.root/"alpha-semantic-qa",None,0.45,0.015,0.45)
        icon=next(row for row in report["assets"] if row["name"]=="icon_test_normal")
        portrait=next(row for row in report["assets"] if row["name"]=="portrait_frame_normal_64")
        self.assertIn("alpha_required_but_opaque",icon["alpha_qa"]["findings"])
        self.assertIn("center_aperture_not_transparent",portrait["alpha_qa"]["findings"])
        self.assertFalse(report["summary"]["pass"])

    def test_qa_rejects_invalid_state_thresholds(self):
        asset=self.make_state("plain_icon",4,(140,100,40,255))
        mp=self.root/"manifest.json";mp.write_text(json.dumps({"assets":[asset]}))
        with self.assertRaisesRegex(ValueError,"state_iou_min"):
            qa.run(mp,self.pack,self.root/"bad-thresholds",None,1.5,0.015,0.45)

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

    def test_visual_qa_rejects_symlink_output_directories(self):
        target=self.root/"target";target.mkdir()
        link=self.root/"linked";link.symlink_to(target,target_is_directory=True)
        asset=self.make_state("icon_test_normal",4,(120,80,30,255))
        with self.assertRaisesRegex(ValueError,"regular directory"):
            qa.render_contact_sheets([asset],self.pack,link)
        with self.assertRaisesRegex(ValueError,"regular directory"):
            qa.render_asset_thumbnails([asset],self.pack,link)

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
