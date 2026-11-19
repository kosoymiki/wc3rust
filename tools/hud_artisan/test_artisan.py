#!/usr/bin/env python3
"""HUD Artisan tests: python -m unittest discover -s tools/hud_artisan -p 'test_*.py'"""
import importlib.util,json,tempfile,unittest,struct,zlib,hashlib,sys,os,io,threading
from jsonschema import Draft202012Validator, RefResolver
from pathlib import Path
SPEC=importlib.util.spec_from_file_location("artisan",Path(__file__).with_name("generate.py"))
artisan=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(artisan)
def png(w,h,rgba):
    def chunk(t,b):
        x=t+b;return struct.pack(">I",len(b))+x+struct.pack(">I",zlib.crc32(x)&0xffffffff)
    raw=b"".join(b"\0"+bytes(rgba*w) for _ in range(h))
    return b"\x89PNG\r\n\x1a\n"+chunk(b"IHDR",struct.pack(">IIBBBBB",w,h,8,6,0,0,0))+chunk(b"IDAT",zlib.compress(raw))+chunk(b"IEND",b"")
class Tests(unittest.TestCase):
    def test_main_cli_qa_delegates_to_integrated_production_qa(self):
        from unittest import mock
        import production_qa
        with mock.patch.object(production_qa,"main",return_value=0) as run_qa:
            result=artisan.main(["qa","--manifest","manifest.json","--pack","pack",
                                 "--out","qa-out","--jobs","jobs.json","--strict"])
        self.assertEqual(result,0)
        run_qa.assert_called_once_with(["--manifest","manifest.json","--pack","pack",
                                        "--out","qa-out","--jobs","jobs.json","--strict"])

    def test_published_adapter_schemas_validate_examples(self):
        base=Path(__file__).parent
        pairs=[
            ("adapter.schema.json","adapter.example.json"),
            ("comfyui.schema.json","comfyui.example.json"),
            ("comfyui.schema.json","comfyui.termux.example.json"),
            ("adapter.schema.json","adapter.termux.example.json"),
            ("job.schema.json",None),
            ("provenance.schema.json",None),
            ("plan.schema.json",None),
        ]
        for schema_name,example_name in pairs:
            schema=json.loads((base/schema_name).read_text())
            Draft202012Validator.check_schema(schema)
            if example_name:
                example=json.loads((base/example_name).read_text())
                Draft202012Validator(schema).validate(example)
    def test_png_parse(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/"a.png";p.write_bytes(png(17,11,(1,2,3,255)))
            self.assertEqual(artisan.png_info(p),(17,11,8,6))
    def test_runtime_provenance_extracts_comfyui_models_sampler_and_hash(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            workflow=root/"workflow.json"
            workflow.write_text(json.dumps({"nodes":[
                {"id":66,"type":"UNETLoader","widgets_values":["z_image_turbo_bf16.safetensors","default"]},
                {"id":70,"type":"KSampler","widgets_values":[123,"fixed",9,1.0,"euler","simple",1.0]}
            ]}))
            runtime=artisan.runtime_provenance({
                "workflow_file":str(workflow),
                "workflow_sha256":hashlib.sha256(workflow.read_bytes()).hexdigest(),
                "workflow_format":"ui",
                "node_inputs":{"positive_prompt":{"node_id":"67","input":"text"}},
                "output_node_id":"9"
            },root/"adapter.json")
            self.assertEqual(runtime["workflow_sha256"],hashlib.sha256(workflow.read_bytes()).hexdigest())
            self.assertEqual(runtime["model_nodes"][0]["selected"][0],"z_image_turbo_bf16.safetensors")
            self.assertEqual(runtime["sampler_nodes"][0]["node_id"],"70")
            self.assertEqual(runtime["output_node_id"],"9")

    def test_runtime_provenance_rejects_workflow_drift_and_symlink(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d); workflow=root/"workflow.json"; workflow.write_text('{"nodes":[]}')
            wrong="0"*64
            with self.assertRaisesRegex(ValueError,"SHA-256 mismatch"):
                artisan.runtime_provenance({"workflow_file":str(workflow),"workflow_sha256":wrong},root/"adapter.json")
            link=root/"link.json"; link.symlink_to(workflow)
            with self.assertRaisesRegex(ValueError,"non-symlink"):
                artisan.runtime_provenance({"workflow_file":str(link)},root/"adapter.json")
    def test_crc_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/"a.png";b=bytearray(png(8,8,(1,2,3,255)));b[29]^=1;p.write_bytes(b)
            with self.assertRaises(ValueError):artisan.png_info(p)
    def test_png_rejects_misordered_ihdr(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/"bad.png";good=png(8,8,(1,2,3,255));pos=8;chunks=[]
            while pos<len(good):
                n=struct.unpack(">I",good[pos:pos+4])[0]
                end=pos+12+n;chunks.append(good[pos:end]);pos=end
            p.write_bytes(good[:8]+chunks[1]+chunks[0]+chunks[2])
            with self.assertRaisesRegex(ValueError,"IHDR must be the first"):
                artisan.png_info(p)
    def test_atlas_cell_can_reuse_declared_manifest_asset(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);m=root/"manifest.json"
            icon={"name":"icon_a","file":"icons/a.png","w":8,"h":8,
                  "insets":{"l":0,"t":0,"r":0,"b":0},"states":["normal"]}
            atlas={"name":"atlas","file":"atlas/commands.png","w":16,"h":16,
                   "insets":{"l":0,"t":0,"r":0,"b":0},"states":[]}
            doc={"design_height":768,"reference_resolution":{"landscape":[1920,1080],"portrait":[1080,1920]},
             "safe_area_1080p":{"landscape":[1,1,1,1],"portrait":[1,1,1,1]},
             "command_card":{"columns":4,"rows":3,"cell":[64,64],"gap":4},
             "control_groups":{"count":10,"cell":[44,44]},
             "atlas":{"commandbuttons":{"file":"atlas/commands.png","entries":[{"id":"icon_a","x":0,"y":0,"w":8,"h":8}]}},
             "assets":[icon,atlas],"font_files":["a.ttf","b.ttf","c.ttf","d.ttf","e.ttf"]}
            m.write_text(json.dumps(doc));out=root/"plan"
            self.assertEqual(artisan.plan(m,out,"classic-modern",7,False),0)
            payload=json.loads((out/"jobs.json").read_text())
            self.assertEqual(payload["job_count"],1)
            self.assertEqual(payload["atlas"]["cells"][0]["source_file"],"icons/a.png")
            self.assertEqual(payload["atlas"]["cells"][0]["source_asset"],"icon_a")
            artisan.verify_plan(m,out/"jobs.json")
    def test_atlas_rejects_mismatched_source_dimensions_and_self_reference(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);m=root/"manifest.json"
            icon={"name":"icon_a","file":"icons/a.png","w":16,"h":16,
                  "insets":{"l":0,"t":0,"r":0,"b":0},"states":["normal"]}
            atlas={"name":"atlas","file":"atlas/commands.png","w":32,"h":32,
                   "insets":{"l":0,"t":0,"r":0,"b":0},"states":[]}
            def document(source,cell_w=8,cell_h=8):
                return {"design_height":768,"reference_resolution":{"landscape":[1920,1080],"portrait":[1080,1920]},
                 "safe_area_1080p":{"landscape":[1,1,1,1],"portrait":[1,1,1,1]},
                 "command_card":{"columns":4,"rows":3,"cell":[64,64],"gap":4},
                 "control_groups":{"count":10,"cell":[44,44]},
                 "atlas":{"commandbuttons":{"file":"atlas/commands.png","entries":[
                    {"id":"cell_a","source_asset":source,"x":0,"y":0,"w":cell_w,"h":cell_h}]}},
                 "assets":[icon,atlas],"font_files":["a.ttf","b.ttf","c.ttf","d.ttf","e.ttf"]}
            m.write_text(json.dumps(document("icon_a")))
            with self.assertRaisesRegex(ValueError,"source asset icon_a is 16x16"):
                artisan.plan(m,root/"plan-bad-size","classic-modern",2,False)
            m.write_text(json.dumps(document("atlas")))
            with self.assertRaisesRegex(ValueError,"atlas cannot use itself"):
                artisan.plan(m,root/"plan-self","classic-modern",2,False)
    def test_symlink_alias_rejected_even_when_target_is_inside_root(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);target=root/"real.png";alias=root/"alias.png"
            target.write_bytes(b"x")
            try:
                alias.symlink_to(target)
            except (OSError,NotImplementedError):
                self.skipTest("symlink creation is unavailable on this platform")
            with self.assertRaisesRegex(ValueError,"symlink path is not allowed"):
                artisan.safe_join(root,"alias.png")
            with self.assertRaisesRegex(ValueError,"symlink path is not allowed"):
                artisan.safe_join_any(root,"alias.png")
            root_alias=root/"root-alias"
            try:
                root_alias.symlink_to(root,target_is_directory=True)
            except (OSError,NotImplementedError):
                self.skipTest("directory symlink creation is unavailable on this platform")
            with self.assertRaisesRegex(ValueError,"root directory must not be a symlink"):
                artisan.safe_join_any(root_alias,"asset.png")
    def test_traversal_rejected(self):
        for x in ("../x.png","/tmp/x.png",""): 
            with self.assertRaises(ValueError):artisan.safe_rel(x)
    def test_plan_rejects_symlinked_output_paths(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            panel={"name":"panel","file":"panel.png","w":8,"h":8,
                   "insets":{"l":0,"t":0,"r":0,"b":0}}
            atlas={"name":"atlas","file":"atlas.png","w":8,"h":8,
                   "insets":{"l":0,"t":0,"r":0,"b":0}}
            doc={"design_height":768,"reference_resolution":{},"safe_area_1080p":{},
                 "command_card":{},"control_groups":{},
                 "atlas":{"commandbuttons":{"file":"atlas.png","entries":[
                     {"id":"cell","x":0,"y":0,"w":4,"h":4}]}},
                 "assets":[panel,atlas],"font_files":["a.ttf","b.ttf","c.ttf","d.ttf","e.ttf"]}
            manifest=root/"manifest.json";manifest.write_text(json.dumps(doc))
            real_dir=root/"real-plan";real_dir.mkdir()
            linked_dir=root/"linked-plan"
            try:
                linked_dir.symlink_to(real_dir,target_is_directory=True)
            except (OSError,NotImplementedError):
                self.skipTest("directory symlink creation unavailable")
            with self.assertRaisesRegex(ValueError,"must not be a symlink"):
                artisan.plan(manifest,linked_dir,"classic-modern",1,False)
            out=root/"plan";out.mkdir()
            target=root/"protected.json";target.write_text("do-not-overwrite")
            try:
                (out/"jobs.json").symlink_to(target)
            except (OSError,NotImplementedError):
                self.skipTest("file symlink creation unavailable")
            with self.assertRaisesRegex(ValueError,"must not be symlinks"):
                artisan.plan(manifest,out,"classic-modern",1,False)
            self.assertEqual(target.read_text(),"do-not-overwrite")

    def test_manifest_insets_reject_strings_and_booleans(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);manifest=root/"manifest.json"
            panel={"name":"panel","file":"panel.png","w":8,"h":8,
                   "insets":{"l":0,"t":0,"r":0,"b":0}}
            atlas={"name":"atlas","file":"atlas.png","w":8,"h":8,
                   "insets":{"l":0,"t":0,"r":0,"b":0}}
            doc={"design_height":768,"reference_resolution":{},"safe_area_1080p":{},
                 "command_card":{},"control_groups":{},
                 "atlas":{"commandbuttons":{"file":"atlas.png","entries":[
                     {"id":"cell","x":0,"y":0,"w":4,"h":4}]}},
                 "assets":[panel,atlas],"font_files":["a.ttf","b.ttf","c.ttf","d.ttf","e.ttf"]}
            for invalid in (True,"2"):
                doc["assets"][0]["insets"]["l"]=invalid
                manifest.write_text(json.dumps(doc))
                with self.subTest(value=invalid):
                    with self.assertRaisesRegex(ValueError,"insets values must be numbers"):
                        artisan.load_manifest(manifest)

    def test_render_dimensions_are_multiple_of_sixteen_and_preserve_extreme_ratios(self):
        self.assertEqual(artisan.render_dimensions(64, 8), (1024, 128))
        self.assertEqual(artisan.render_dimensions(720, 32), (1440, 64))
        self.assertEqual(artisan.render_dimensions(512, 384), (1024, 768))
        self.assertEqual(artisan.render_dimensions(1024, 512), (1024, 512))
        for width, height in ((64, 8), (720, 32), (44, 44), (512, 384)):
            rw, rh = artisan.render_dimensions(width, height)
            self.assertEqual(rw % 16, 0)
            self.assertEqual(rh % 16, 0)
            self.assertGreaterEqual(rw, 16)
            self.assertGreaterEqual(rh, 16)

    def test_jobs_include_native_and_render_dimensions(self):
        asset={"name":"icon_gold_normal_64","file":"icons/gold.png","w":64,"h":64,
               "insets":{"l":0,"t":0,"r":0,"b":0},"states":["normal"]}
        job=artisan.make_job(asset,"classic-modern",1)
        self.assertEqual((job["width"],job["height"]),(64,64))
        self.assertEqual((job["render_width"],job["render_height"]),(1024,1024))
        self.assertIn("#FF00FF",job["prompt"])

    def test_semantic_prompt(self):
        a={"name":"icon_gold_normal_32","file":"icons/gold.png","w":32,"h":32,"insets":{"l":0,"t":0,"r":0,"b":0},"states":["normal"]}
        j=artisan.make_job(a,"classic-modern",9);self.assertTrue(j["requires_alpha"]);self.assertIn("9-slice",j["prompt"])
    def test_job_identity_tracks_prompt_revision(self):
        asset={"name":"icon_fire_spell_32","file":"icons/fire.png","w":32,"h":32,
               "insets":{"l":0,"t":0,"r":0,"b":0},"states":["normal"]}
        first=artisan.make_job(asset,"classic-modern",11)
        repeated=artisan.make_job(asset,"classic-modern",11)
        changed=artisan.make_job(asset,"undead-obsidian",11)
        self.assertEqual(first["job_id"],repeated["job_id"])
        self.assertEqual(first["prompt_sha256"],repeated["prompt_sha256"])
        self.assertNotEqual(first["job_id"],changed["job_id"])
        self.assertNotEqual(first["prompt_sha256"],changed["prompt_sha256"])
    def test_manifest_reference_requires_existing_file_and_license(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);m=root/"manifest.json"
            a={"name":"panel","file":"panel.png","w":8,"h":8,"insets":{"l":0,"t":0,"r":0,"b":0},
               "references":[{"source":"reference.png","license":"CC0-1.0","purpose":"material guidance","file":"reference.png"}]}
            at={"name":"atlas","file":"atlas.png","w":8,"h":8,"insets":{"l":0,"t":0,"r":0,"b":0}}
            doc={"design_height":768,"reference_resolution":{},"safe_area_1080p":{},"command_card":{},"control_groups":{},
              "atlas":{"commandbuttons":{"file":"atlas.png","entries":[{"id":"a","x":0,"y":0,"w":4,"h":4}]}},
              "assets":[a,at],"font_files":["a.ttf","b.ttf","c.ttf","d.ttf","e.ttf"]}
            m.write_text(json.dumps(doc))
            with self.assertRaisesRegex(ValueError,"reference 0 file does not exist"):
                artisan.load_manifest(m)
            (root/"reference.png").write_bytes(b"not decoded as an art image here")
            loaded=artisan.load_manifest(m)
            job=artisan.make_job(loaded["assets"][0],"classic-modern",1)
            self.assertEqual(job["references"],a["references"])
            self.assertIn("CC0-1.0",job["prompt"])
    def test_manifest_semantics_override_filename_heuristics(self):
        asset={"name":"ability_q_custom_32","file":"icons/custom.png","w":32,"h":32,
               "insets":{"l":0,"t":0,"r":0,"b":0},"states":["normal"],
               "semantic":"a three-pronged frost spear wrapping around a crimson heart",
               "faction":"northern frost cult","visual_tags":["ice","piercing","healing"]}
        prompt=artisan.make_job(asset,"classic-modern",9)["prompt"]
        self.assertIn("three-pronged frost spear",prompt)
        self.assertIn("northern frost cult",prompt)
        self.assertIn("piercing",prompt)
        self.assertNotIn("generic placeholder rune",prompt)
    def test_semantic_subjects_and_state_language(self):
        icon={"name":"icon_frost_spell_32","file":"icons/frost.png","w":32,"h":32,
              "insets":{"l":0,"t":0,"r":0,"b":0},"states":["hover","disabled"]}
        job=artisan.make_job(icon,"classic-modern",17)
        self.assertIn("frost crystal",job["prompt"])
        self.assertIn("hover state",job["prompt"])
        self.assertIn("disabled state",job["prompt"])
        self.assertNotIn("generic placeholder rune",job["prompt"])
    def test_9slice_regions_are_in_prompt(self):
        a={"name":"menu_panel_normal","file":"panel.png","w":512,"h":384,
           "insets":{"l":24,"t":32,"r":24,"b":32},"states":["normal"]}
        p=artisan.make_job(a,"classic-modern",1)["prompt"]
        for token in ("L24","T32","R24","B32","central field calm"):
            self.assertIn(token,p)
    def test_inactive_state_does_not_receive_active_treatment(self):
        a={"name":"control_group_button_inactive_44","file":"groups/inactive.png","w":44,"h":44,
           "insets":{"l":2,"t":2,"r":2,"b":2},"states":["inactive"]}
        prompt=artisan.make_job(a,"classic-modern",3)["prompt"]
        self.assertIn("inactive state",prompt)
        self.assertNotIn("active state: stronger",prompt)
    def test_real_final_hud_manifest_compiles_full_plan(self):
        repo_root=Path(__file__).resolve().parents[2]
        manifest=repo_root/"analysis"/"hudskin"/"final"/"wc3rust-hud-final"/"manifest.json"
        self.assertTrue(manifest.is_file(),"the authoritative final HUD manifest must be present in the branch")
        doc=artisan.load_manifest(manifest,engine_contract=True)
        self.assertEqual(len(doc["assets"]),92)
        self.assertEqual(len(doc["atlas"]["commandbuttons"]["entries"]),20)
        jobs,atlas_plan=artisan.compile_jobs(doc,"classic-modern",20261119)
        self.assertEqual(len(jobs),91,"the declared atlas must reuse existing command icon assets, not synthesize duplicate jobs")
        self.assertEqual(len(atlas_plan["cells"]),20)
        self.assertTrue(all(entry.get("source_asset") for entry in doc["atlas"]["commandbuttons"]["entries"]))
        for job in jobs:
            self.assertEqual(job["render_width"] % 16,0,job["asset"])
            self.assertEqual(job["render_height"] % 16,0,job["asset"])
            self.assertLessEqual(max(job["render_width"],job["render_height"]),4096,job["asset"])
            self.assertIn(f"render canvas {job['render_width']}x{job['render_height']}",job["prompt"])

    def test_engine_profile_reference_resolution_matches_runtime_contract(self):
        profile=json.loads((Path(__file__).with_name("engine_profile.json")).read_text())
        self.assertEqual(profile["reference_resolution"],
                         {"landscape":[1920,1080],"portrait":[1080,1920]})
        self.assertEqual(profile["safe_area_1080p"],
                         {"landscape":[48,80,48,48],"portrait":[24,96,24,64]})
        self.assertEqual(profile["asset_count_required"],92)
        self.assertEqual(profile["commandbuttons_atlas"]["entries"],20)

    def test_engine_contract_exact_manifest(self):
        assets=[]
        for i,name in enumerate(artisan.ENGINE_ROLES):
            w,h=artisan.ENGINE_ROLE_DIMENSIONS[name]
            assets.append({"name":name,"file":f"assets/{i}.png","w":w,"h":h})
        assets.append({"name":"commandbuttons_atlas","file":"atlas/commandbuttons.png","w":2048,"h":2048})
        for i in range(67):
            assets.append({"name":f"extra_{i}","file":f"extra/{i}.png","w":64,"h":64})
        for i in range(20):
            assets.append({"name":f"command_icon_{i:02d}","file":f"icons/{i:02d}.png","w":64,"h":64})
        # Keep the 92-asset contract: replace twenty generic extras with source icons.
        assets=assets[:24]+assets[44:]
        atlas_entries=[{"id":f"command_{i:02d}","x":(i-1)*64,"y":0,"w":64,"h":64,
                        "source_asset":f"command_icon_{i-1:02d}"} for i in range(1,21)]
        doc={"design_height":768,"reference_resolution":{"landscape":[1920,1080],"portrait":[1080,1920]},
          "safe_area_1080p":{"landscape":[48,80,48,48],"portrait":[24,96,24,64]},
          "command_card":{"columns":4,"rows":3,"cell":[64,64],"gap":4},
          "control_groups":{"count":10,"cell":[44,44]},
          "font_files":list(artisan.ENGINE_FONT_FILES),
          "assets":assets,"atlas":{"commandbuttons":{"file":"atlas/commandbuttons.png","size":[2048,2048],
            "columns":32,"rows":32,"cell":[64,64],"padding":0,"rotation":False,"entries":atlas_entries}}}
        self.assertEqual(len(assets),92)
        artisan.validate_engine_contract(doc)
        wrong=next(a for a in assets if a["name"]=="button_glow_64")
        wrong["w"]=63
        with self.assertRaisesRegex(ValueError,"button_glow_64 must be 64x64"):
            artisan.validate_engine_contract(doc)
        wrong["w"]=64
        doc["atlas"]["commandbuttons"]["entries"].pop()
        with self.assertRaisesRegex(ValueError,"20 entries"):
            artisan.validate_engine_contract(doc)
        doc["atlas"]["commandbuttons"]["entries"].append(atlas_entries[-1])
        doc["atlas"]["commandbuttons"]["entries"][0]["x"]=1
        with self.assertRaisesRegex(ValueError,"grid-aligned"):
            artisan.validate_engine_contract(doc)
        doc["atlas"]["commandbuttons"]["entries"][0]["x"]=0
        doc["font_files"].pop()
        with self.assertRaisesRegex(ValueError,"five named HUD font"):
            artisan.validate_engine_contract(doc)
    def test_font_path_traversal_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            m=Path(d)/"m.json"
            a={"name":"a","file":"a.png","w":8,"h":8,"insets":{"l":0,"t":0,"r":0,"b":0}}
            atlas={"name":"atlas","file":"atlas.png","w":8,"h":8,"insets":{"l":0,"t":0,"r":0,"b":0}}
            doc={"design_height":768,"reference_resolution":{},"safe_area_1080p":{},"command_card":{},"control_groups":{},
              "atlas":{"commandbuttons":{"file":"atlas.png","entries":[{"id":"a","x":0,"y":0,"w":4,"h":4}]}},
              "assets":[a,atlas],"font_files":["../evil.ttf","b.ttf","c.ttf","d.ttf","e.ttf"]}
            m.write_text(json.dumps(doc))
            with self.assertRaisesRegex(ValueError,"safe basenames"):
                artisan.load_manifest(m)
    def test_engine_roles(self):self.assertEqual(len(artisan.ENGINE_ROLES),24)
    def test_manifest_and_plan(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);m=root/"manifest.json"
            a={"name":"panel","file":"panels/panel.png","w":64,"h":64,"insets":{"l":8,"t":8,"r":8,"b":8},"states":["normal"]}
            at={"name":"atlas","file":"atlas/cmd.png","w":320,"h":256,"insets":{"l":0,"t":0,"r":0,"b":0},"states":[]}
            doc={"design_height":768,"reference_resolution":{"landscape":[1920,1080],"portrait":[1080,1920]},
             "safe_area_1080p":{"landscape":[1,1,1,1],"portrait":[1,1,1,1]},"command_card":{"columns":4,"rows":3,"cell":[64,64],"gap":4},
             "control_groups":{"count":10,"cell":[44,44]},"atlas":{"commandbuttons":{"file":"atlas/cmd.png","columns":5,"rows":4,"cell":[64,64],"entries":[{"id":"a","x":0,"y":0,"w":64,"h":64}]}},
             "assets":[a,at],"font_files":["a.ttf","b.ttf","c.ttf","d.ttf","e.ttf"]}
            m.write_text(json.dumps(doc));self.assertEqual(len(artisan.load_manifest(m)["assets"]),2)
            out=root/"plan";self.assertEqual(artisan.plan(m,out,"classic-modern",12,False),0)
            plan_data=json.loads((out/"jobs.json").read_text())
            self.assertEqual(plan_data["job_count"],2)
            plan_schema=json.loads((Path(__file__).with_name("plan.schema.json")).read_text())
            job_schema=json.loads((Path(__file__).with_name("job.schema.json")).read_text())
            resolver=RefResolver.from_schema(plan_schema,store={job_schema["$id"]:job_schema})
            Draft202012Validator(plan_schema,resolver=resolver).validate(plan_data)
            plan_doc,manifest_doc=artisan.verify_plan(m,out/"jobs.json")
            self.assertEqual(plan_doc["source_manifest_sha256"],hashlib.sha256(m.read_bytes()).hexdigest())
            tampered=json.loads((out/"jobs.json").read_text())
            tampered["atlas"]["cells"][0]["x"]=1
            (out/"jobs.json").write_text(json.dumps(tampered))
            with self.assertRaisesRegex(ValueError,"atlas geometry"):
                artisan.verify_plan(m,out/"jobs.json")
            artisan.plan(m,out,"classic-modern",12,False)
            doc["atlas"]["commandbuttons"]["entries"].append({"id":"overlap","x":0,"y":0,"w":64,"h":64})
            m.write_text(json.dumps(doc))
            with self.assertRaisesRegex(ValueError,"overlap"):
                artisan.plan(m,root/"bad-plan","classic-modern",12,False)
    def test_provider_dispatch_ingest_atlas_and_provenance(self):
        import shutil
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);manifest=root/"manifest.json";drop=root/"drop";out=root/"pack"
            panel={"name":"panel","file":"panels/panel.png","w":16,"h":16,"insets":{"l":2,"t":2,"r":2,"b":2},"states":["normal"]}
            atlas={"name":"atlas","file":"atlas/cmd.png","w":16,"h":16,"insets":{"l":0,"t":0,"r":0,"b":0},"states":[]}
            doc={"design_height":768,"reference_resolution":{"landscape":[1920,1080],"portrait":[1080,1920]},
             "safe_area_1080p":{"landscape":[1,1,1,1],"portrait":[1,1,1,1]},"command_card":{"columns":4,"rows":3,"cell":[64,64],"gap":4},
             "control_groups":{"count":10,"cell":[44,44]},"atlas":{"commandbuttons":{"file":"atlas/cmd.png","columns":1,"rows":1,
             "cell":[8,8],"entries":[{"id":"icon_a","x":0,"y":0,"w":8,"h":8}]}},
             "assets":[panel,atlas],"font_files":["a.ttf","b.ttf","c.ttf","d.ttf","e.ttf"]}
            manifest.write_text(json.dumps(doc))
            jobs_dir=root/"jobs";artisan.plan(manifest,jobs_dir,"classic-modern",3,False)
            jobs=jobs_dir/"jobs.json";payload=json.loads(jobs.read_text())
            self.assertEqual(payload["job_count"],2)
            adapter_script=root/"provider_adapter.py"
            counter=root/"provider-calls.log"
            adapter_script.write_text(
                "import json,sys,os\nfrom PIL import Image,ImageDraw\n"
                f"open({str(counter)!r},'a').write('x')\n"
                "print(os.environ.get('HUD_ARTISAN_TEST_SECRET',''))\n"
                "if os.environ.get('HUD_ARTISAN_TEST_SECRET') == 'FAIL-SELECTIVE-REPAIR': raise SystemExit(7)\n"
                "j=json.load(open(sys.argv[1],encoding='utf-8'))\n"
                "w,h=j['width'],j['height']\n"
                "im=Image.new('RGBA',(w,h),(0,0,0,0) if j['requires_alpha'] else (40,35,30,255))\n"
                "d=ImageDraw.Draw(im)\n"
                "if j['requires_alpha']: d.ellipse((1,1,max(1,w-2),max(1,h-2)),fill=(220,150,40,255))\n"
                "else: d.line((0,0,w-1,h-1),fill=(90,80,65,255),width=1)\n"
                "im.save(sys.argv[2],format='PNG')\n",encoding="utf-8")
            adapter=root/"adapter.json"
            adapter.write_text(json.dumps({"schema_version":1,"provider":"test-provider","model":"test-model",
              "license":"clean-room test fixture","working_directory":str(root),
              "command":[sys.executable,str(adapter_script),"{job_json}","{output_path}"],
              "environment":["HUD_ARTISAN_TEST_SECRET"],"timeout_seconds":20,"retries":1,"max_parallel":2}))
            os.environ["HUD_ARTISAN_TEST_SECRET"]="TOP-SECRET-DO-NOT-LOG"
            try:
                self.assertEqual(artisan.dispatch(manifest,jobs,adapter,drop,2),0)
                first_call_count=len(counter.read_text())
                self.assertEqual(first_call_count,payload["job_count"])
                self.assertEqual(artisan.dispatch(manifest,jobs,adapter,drop,2,resume=True),0)
                self.assertEqual(len(counter.read_text()),first_call_count,
                                 "resume must not call the renderer for hash-verified outputs")
                resume_logs="".join(path.read_text() for path in (drop/"reports"/"worker").glob("*.log"))
                self.assertIn("RESUME verified",resume_logs)
                # Corrupting one job's checkpoint binding regenerates only that job.
                checkpoint=drop/"reports"/"worker-results.json"
                checkpoint_doc=json.loads(checkpoint.read_text())
                checkpoint_doc["results"][0]["prompt_sha256"]="0"*64
                checkpoint.write_text(json.dumps(checkpoint_doc))
                self.assertEqual(artisan.dispatch(manifest,jobs,adapter,drop,2,resume=True),0)
                self.assertEqual(len(counter.read_text()),first_call_count+1)
                # A renderer-script revision invalidates the entire adapter fingerprint.
                adapter_script.write_text(adapter_script.read_text()+"\n# renderer revision changed\n")
                self.assertEqual(artisan.dispatch(manifest,jobs,adapter,drop,2,resume=True),0)
                self.assertEqual(len(counter.read_text()),first_call_count+1+payload["job_count"])
                # QA-driven repair changes only the selected job seed/id and carries forward
                # untouched provenance only after verifying the original PNG hash.
                qa_report=root/"production-qa.json"
                qa_report.write_text(json.dumps({
                    "tool":"HUD Artisan production QA","schema_version":1,
                    "manifest_sha256":artisan.sha(manifest.read_bytes()),
                    "regeneration_candidates":[{"asset":"panel","reason":"fixture QA finding"}]
                }))
                repair_dir=root/"repair-plan"
                self.assertEqual(artisan.repair_plan(manifest,jobs,qa_report,repair_dir,"test-repair-r1"),0)
                repair_jobs=repair_dir/"jobs.json"
                repaired_plan,_=artisan.verify_plan(manifest,repair_jobs)
                base=Path(__file__).parent
                plan_schema=json.loads((base/"plan.schema.json").read_text())
                job_schema=json.loads((base/"job.schema.json").read_text())
                resolver=RefResolver.from_schema(plan_schema,store={job_schema["$id"]:job_schema})
                Draft202012Validator(plan_schema,resolver=resolver).validate(repaired_plan)
                original_panel=next(x for x in payload["jobs"] if x["asset"]=="panel")
                repaired_panel=next(x for x in repaired_plan["jobs"] if x["asset"]=="panel")
                self.assertNotEqual(repaired_panel["job_id"],original_panel["job_id"])
                self.assertNotEqual(repaired_panel["seed"],original_panel["seed"])
                before_repair_calls=len(counter.read_text())
                self.assertEqual(artisan.dispatch(manifest,repair_jobs,adapter,drop,1,only=["panel"]),0)
                self.assertEqual(len(counter.read_text()),before_repair_calls+1,
                                 "selective repair must render only the chosen asset")
                repaired_prov=json.loads((drop/"provenance.json").read_text())
                self.assertEqual({x["job_id"] for x in repaired_prov["jobs"]},
                                 {x["job_id"] for x in repaired_plan["jobs"]})
                # A failed selective attempt must not destroy the last full-batch
                # receipt; a retry of that same selected asset can still verify all
                # untouched outputs and complete the new receipt.
                os.environ["HUD_ARTISAN_TEST_SECRET"]="FAIL-SELECTIVE-REPAIR"
                self.assertEqual(artisan.dispatch(manifest,repair_jobs,adapter,drop,1,only=["panel"]),1)
                self.assertTrue((drop/"provenance.json").is_file(),
                                "failed selective repair must retain prior full-batch provenance")
                os.environ["HUD_ARTISAN_TEST_SECRET"]="TOP-SECRET-DO-NOT-LOG"
                self.assertEqual(artisan.dispatch(manifest,repair_jobs,adapter,drop,1,only=["panel"]),0)
                retried_prov=json.loads((drop/"provenance.json").read_text())
                self.assertEqual({x["job_id"] for x in retried_prov["jobs"]},
                                 {x["job_id"] for x in repaired_plan["jobs"]})
                jobs=repair_jobs
            finally:
                os.environ.pop("HUD_ARTISAN_TEST_SECRET",None)
            self.assertTrue((drop/"provenance.json").is_file())
            provenance_schema=json.loads((Path(__file__).with_name("provenance.schema.json")).read_text())
            Draft202012Validator(provenance_schema).validate(json.loads((drop/"provenance.json").read_text()))
            logs=list((drop/"reports"/"worker").glob("*.log"))
            self.assertTrue(logs)
            combined="".join(path.read_text() for path in logs)
            self.assertIn("[REDACTED]",combined)
            self.assertNotIn("TOP-SECRET-DO-NOT-LOG",combined)
            (drop/"fonts").mkdir()
            for name in doc["font_files"]:(drop/"fonts"/name).write_bytes(b"font-dependency-placeholder")
            provenance_path=drop/"provenance.json"
            valid_provenance=provenance_path.read_text()
            malformed=json.loads(valid_provenance)
            malformed["adapter_config_sha256"]="not-a-sha256"
            provenance_path.write_text(json.dumps(malformed))
            rejected_out=root/"rejected-pack"
            self.assertEqual(artisan.ingest(manifest,jobs,drop,rejected_out,"strict",False),1)
            self.assertFalse(rejected_out.exists(),"invalid provenance must never promote a pack")
            provenance_path.write_text(valid_provenance)
            self.assertEqual(artisan.ingest(manifest,jobs,drop,out,"strict",False),0)
            self.assertEqual(artisan.png_info(out/"atlas/cmd.png")[:2],(16,16))
            self.assertEqual(artisan.validate(manifest,out,False,False),0)
            # Reuse the original local pack's fonts without copying them into the renderer drop.
            font_source=root/"original-fonts";font_source.mkdir()
            for name in doc["font_files"]:(font_source/name).write_bytes(b"original-font-binary")
            shutil.rmtree(drop/"fonts")
            font_pack=root/"font-source-pack"
            self.assertEqual(artisan.ingest(manifest,jobs,drop,font_pack,"strict",False,font_source),0)
            self.assertTrue(all((font_pack/"fonts"/name).is_file() for name in doc["font_files"]))
            # A failed subsequent batch must invalidate prior success evidence.
            failing= root/"failing-adapter.json"
            failing.write_text(json.dumps({"schema_version":1,"provider":"test-provider","model":"test-model",
              "license":"clean-room test fixture","working_directory":str(root),
              "command":[sys.executable,"-c","raise SystemExit(7)","{job_json}","{output_path}"],
              "environment":[],"timeout_seconds":5,"retries":0,"max_parallel":1}))
            self.assertEqual(artisan.dispatch(manifest,jobs,failing,drop,1),1)
            self.assertFalse((drop/"provenance.json").exists(),
                             "failed dispatch must not leave stale provenance from a prior successful batch")
            self.assertTrue((drop/"reports"/"worker-results.json").exists(),
                             "worker report should be refreshed for the failed run")
    def test_ingest_failure_never_promotes_partial_pack(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);manifest=root/"manifest.json";drop=root/"drop";drop.mkdir()
            panel={"name":"panel","file":"panels/panel.png","w":16,"h":16,
                   "insets":{"l":2,"t":2,"r":2,"b":2},"states":["normal"]}
            atlas={"name":"atlas","file":"atlas/cmd.png","w":16,"h":16,
                   "insets":{"l":0,"t":0,"r":0,"b":0},"states":[]}
            doc={"design_height":768,"reference_resolution":{"landscape":[1920,1080],"portrait":[1080,1920]},
             "safe_area_1080p":{"landscape":[1,1,1,1],"portrait":[1,1,1,1]},
             "command_card":{"columns":4,"rows":3,"cell":[64,64],"gap":4},
             "control_groups":{"count":10,"cell":[44,44]},
             "atlas":{"commandbuttons":{"file":"atlas/cmd.png","columns":1,"rows":1,
               "cell":[8,8],"entries":[{"id":"icon_a","x":0,"y":0,"w":8,"h":8}]}},
             "assets":[panel,atlas],"font_files":["a.ttf","b.ttf","c.ttf","d.ttf","e.ttf"]}
            manifest.write_text(json.dumps(doc))
            jobs_dir=root/"jobs"
            self.assertEqual(artisan.plan(manifest,jobs_dir,"classic-modern",3,False),0)
            out=root/"pack"
            self.assertEqual(artisan.ingest(manifest,jobs_dir/"jobs.json",drop,out,"strict",False),1)
            self.assertFalse(out.exists(),"failed ingest must not expose a partial output pack")
            self.assertEqual(list(root.glob(".pack.hudartisan-*")),[],
                             "failed ingestion must remove temporary staging directories")
    def test_comfyui_adapter_end_to_end_with_mocked_api(self):
        from PIL import Image
        from contextlib import redirect_stdout,redirect_stderr
        import importlib.util
        spec=importlib.util.spec_from_file_location("comfy_adapter",Path(__file__).with_name("comfyui_adapter.py"))
        comfy=importlib.util.module_from_spec(spec);spec.loader.exec_module(comfy)
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);calls=[]
            image=Image.new("RGBA",(16,16),(0,0,0,0))
            for y in range(4,12):
                for x in range(4,12):image.putpixel((x,y),(190,130,40,255))
            stream=io.BytesIO();image.save(stream,format="PNG");image_bytes=stream.getvalue()
            def fake_http_json(base,endpoint,payload=None,timeout=60):
                calls.append((endpoint,payload,timeout))
                if endpoint=="/prompt":return {"prompt_id":"fake-prompt-1"}
                if endpoint=="/history/fake-prompt-1":
                    return {"fake-prompt-1":{"status":{"status_str":"success","completed":True},"outputs":{"9":{"images":[
                        {"filename":"frame.png","subfolder":"","type":"output"}]}}}}
                raise AssertionError(f"unexpected ComfyUI endpoint: {endpoint}")
            def fake_download_image(base,image_meta,timeout):
                self.assertEqual(image_meta["filename"],"frame.png")
                return image_bytes
            real_download_image=comfy.download_image
            comfy.http_json=fake_http_json
            comfy.download_image=fake_download_image
            workflow={"6":{"class_type":"CLIPTextEncode","inputs":{"text":"old-positive"}},
              "7":{"class_type":"CLIPTextEncode","inputs":{"text":"old-negative"}},
              "5":{"class_type":"EmptySD3LatentImage","inputs":{"width":512,"height":512}},
              "3":{"class_type":"KSampler","inputs":{"seed":1}},
              "9":{"class_type":"SaveImage","inputs":{"images":["8",0]}}}
            (root/"workflow.json").write_text(json.dumps(workflow))
            config={"schema_version":1,"server":"http://127.0.0.1:8188","workflow_file":"workflow.json",
              "node_inputs":{"positive_prompt":{"node_id":"6","input":"text"},
                "negative_prompt":{"node_id":"7","input":"text"},"width":{"node_id":"5","input":"width"},
                "height":{"node_id":"5","input":"height"},"seed":{"node_id":"3","input":"seed"}},
              "output_node_id":"9","request_timeout_seconds":5,"max_wait_seconds":5,"poll_interval_seconds":0.2}
            config_path=root/"comfy.json";config_path.write_text(json.dumps(config))
            job={"job_id":"a1b2c3d4e5f60718","prompt":"a sculpted gold emblem","negative_prompt":"text, watermark",
              "width":8,"height":8,"render_width":16,"render_height":16,
              "seed":123456789,"family":"icon","requires_alpha":True}
            job_path=root/"job.json";job_path.write_text(json.dumps(job))
            output=root/"out.png";stderr=io.StringIO()
            with redirect_stdout(io.StringIO()),redirect_stderr(stderr):
                rc=comfy.main(["--config",str(config_path),"--job-json",str(job_path),"--output",str(output)])
            self.assertEqual(rc,0,stderr.getvalue())
            self.assertEqual([call[0] for call in calls],["/prompt","/history/fake-prompt-1"])
            sent=calls[0][1]["prompt"]
            self.assertEqual(sent["6"]["inputs"]["text"],job["prompt"])
            self.assertEqual(sent["7"]["inputs"]["text"],job["negative_prompt"])
            self.assertEqual(sent["5"]["inputs"]["width"],16)
            self.assertEqual(sent["5"]["inputs"]["height"],16)
            self.assertEqual(sent["3"]["inputs"]["seed"],job["seed"])
            self.assertEqual(artisan.png_info(output),(8,8,8,6))
            from unittest import mock
            comfy.download_image=real_download_image
            with mock.patch.object(comfy,"open_http",return_value=io.BytesIO(image_bytes)) as open_local:
                downloaded=comfy.download_image("http://127.0.0.1:8188",
                    {"filename":"frame.png","subfolder":"","type":"output"},5)
            self.assertEqual(downloaded,image_bytes)
            open_local.assert_called_once()
    def test_duplicate_name_path_guard(self):
        with tempfile.TemporaryDirectory() as d:
            m=Path(d)/"m.json";a={"name":"a","file":"same.png","w":1,"h":1,"insets":{"l":0,"t":0,"r":0,"b":0}}
            doc={"design_height":1,"reference_resolution":{},"safe_area_1080p":{},"command_card":{},"control_groups":{},
             "atlas":{"commandbuttons":{"entries":[{"id":"x"}]}},"assets":[a,{**a,"name":"b"}],"font_files":["a","b","c","d","e"]}
            m.write_text(json.dumps(doc))
            with self.assertRaises(ValueError):artisan.load_manifest(m)

    def test_prompt_embeds_premium_art_direction_and_family_rules(self):
        icon={"name":"icon_gold","file":"gold.png","w":64,"h":64,
              "insets":{"l":0,"t":0,"r":0,"b":0},"states":["normal"]}
        job=artisan.make_job(icon,"classic-modern",73)
        self.assertIn("Premium hand-authored real-time-strategy game UI illustration",job["prompt"])
        self.assertIn("Distinct from every sibling icon",job["prompt"])
        self.assertIn("readable at native target size",job["prompt"])
        self.assertIn("one dominant silhouette",job["prompt"])
        panel={**icon,"name":"minimap_frame_normal_216"}
        frame=artisan.make_job(panel,"classic-modern",73)
        self.assertIn("fully transparent unobstructed map aperture",frame["prompt"])
    def test_semantic_family_order_and_transparency_contract(self):
        self.assertEqual(artisan.family("icon_minimap_scout"),"icon")
        self.assertEqual(artisan.family("minimap_header_normal_216x20"),"hud-strip")
        self.assertEqual(artisan.family("minimap_frame_normal_216"),"minimap-frame")
        self.assertEqual(artisan.family("portrait_frame_hero_128"),"portrait-frame")
        icon={"name":"portrait_frame_hero_128","file":"portrait.png","w":128,"h":128,
              "insets":{"l":16,"t":16,"r":16,"b":16},"states":["normal"]}
        job=artisan.make_job(icon,"classic-modern",73)
        self.assertTrue(job["requires_alpha"])
        self.assertIn("same flat solid chroma-key magenta matte",job["prompt"])
        self.assertEqual(job["seed"],artisan.make_job(icon,"classic-modern",73)["seed"])
        sibling={**icon,"name":"minimap_frame_normal_216"}
        self.assertNotEqual(job["seed"],artisan.make_job(sibling,"classic-modern",73)["seed"])
        header={**icon,"name":"minimap_header_normal_216x20"}
        self.assertFalse(artisan.make_job(header,"classic-modern",73)["requires_alpha"])
    def test_center_aperture_metric_detects_filled_frame(self):
        from PIL import Image,ImageDraw
        valid=Image.new("RGBA",(100,100),(0,0,0,0));draw=ImageDraw.Draw(valid)
        draw.rectangle((0,0,99,99),fill=(150,100,40,255))
        draw.rectangle((18,18,81,81),fill=(0,0,0,0))
        self.assertGreaterEqual(artisan.center_transparent_fraction(valid),0.97)
        invalid=Image.new("RGBA",(100,100),(120,80,30,255))
        self.assertLess(artisan.center_transparent_fraction(invalid),0.97)
    def test_comfyui_loopback_download_uses_proxy_bypass_transport(self):
        import importlib.util
        from unittest import mock
        spec=importlib.util.spec_from_file_location("comfy_adapter",Path(__file__).with_name("comfyui_adapter.py"))
        comfy=importlib.util.module_from_spec(spec);spec.loader.exec_module(comfy)
        request=comfy.request.Request("http://127.0.0.1:8188/view")
        opener=mock.Mock()
        with mock.patch.object(comfy.request,"build_opener",return_value=opener) as build:
            with mock.patch.object(comfy.request,"urlopen") as urlopen:
                comfy.open_http(request,"http://127.0.0.1:8188",5)
                urlopen.assert_not_called()
        build.assert_called_once()
        handler=build.call_args.args[0]
        self.assertIsInstance(handler,comfy.request.ProxyHandler)
        self.assertEqual(handler.proxies,{})
        opener.open.assert_called_once_with(request,timeout=5)
if __name__=="__main__":unittest.main()
