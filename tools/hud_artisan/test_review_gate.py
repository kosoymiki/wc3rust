import hashlib
import json
import tempfile
import unittest
from pathlib import Path

import generate
import review_gate


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


class ReviewGateTests(unittest.TestCase):
    def fixture(self, root):
        manifest = root / "manifest.json"
        pack = root / "pack"
        pack.mkdir()
        image = pack / "icons" / "gold.png"
        image.parent.mkdir()
        image.write_bytes(b"fake-png-bytes-for-hash-binding")
        manifest_doc = {
            "design_height": 768,
            "reference_resolution": {"landscape": [1920, 1080], "portrait": [1080, 1920]},
            "safe_area_1080p": {"landscape": [1, 1, 1, 1], "portrait": [1, 1, 1, 1]},
            "command_card": {"columns": 4, "rows": 3, "cell": [64, 64], "gap": 4},
            "control_groups": {"count": 10, "cell": [44, 44]},
            "atlas": {"commandbuttons": {
                "file": "atlas/cmd.png", "columns": 1, "rows": 1, "cell": [32, 32],
                "entries": [{"id": "gold", "x": 0, "y": 0, "w": 32, "h": 32,
                             "source_asset": "icon_gold"}],
            }},
            "assets": [
                {"name": "icon_gold", "file": "icons/gold.png", "w": 32, "h": 32,
                 "insets": {"l": 0, "t": 0, "r": 0, "b": 0}, "states": ["normal"]},
                {"name": "atlas", "file": "atlas/cmd.png", "w": 32, "h": 32,
                 "insets": {"l": 0, "t": 0, "r": 0, "b": 0}, "states": []},
            ],
            "font_files": ["a.ttf", "b.ttf", "c.ttf", "d.ttf", "e.ttf"],
        }
        manifest.write_text(json.dumps(manifest_doc))
        plan_dir = root / "plan"
        self.assertEqual(generate.plan(manifest, plan_dir, "classic-modern", 3, False), 0)
        jobs = plan_dir / "jobs.json"
        jid = json.loads(jobs.read_text())["jobs"][0]["job_id"]
        prompt = (
            "Inspect the exact final PNG at native size and reduced scale. Judge semantic read, "
            "silhouette, materials, lighting, value hierarchy, craftsmanship, style consistency, "
            "engine fitness and originality. Return concrete defects; do not inflate scores."
        )
        raw_art = root / "art-director-response.md"
        raw_art.write_text("Independent art direction review. The icon is clear at native size and has no visible defects.")
        raw_red = root / "red-team-response.md"
        raw_red.write_text("Adversarial review. I checked alpha, silhouette, edge halos, crop, semantics and engine fit.")
        reviewers = [
            {
                "reviewer_id": "art-director-1", "provider": "OpenCode", "model": "vision-model-A",
                "role": "art_director", "independent": True, "session_id": "session-art-0001",
                "review_prompt": prompt, "review_prompt_sha256": hashlib.sha256(prompt.encode()).hexdigest(),
                "raw_response_file": raw_art.name, "raw_response_sha256": sha(raw_art),
            },
            {
                "reviewer_id": "red-team-1", "provider": "OpenCode", "model": "vision-model-B",
                "role": "red_team", "independent": True, "session_id": "session-red-0002",
                "review_prompt": prompt, "review_prompt_sha256": hashlib.sha256(prompt.encode()).hexdigest(),
                "raw_response_file": raw_red.name, "raw_response_sha256": sha(raw_red),
            },
        ]
        scores = {key: 10 for key in review_gate.CRITERIA}
        review_doc = {
            "schema_version": 2,
            "manifest_sha256": sha(manifest),
            "jobs_sha256": sha(jobs),
            "reviewers": reviewers,
            "reviews": {
                jid: [
                    {
                        "reviewer_id": reviewer["reviewer_id"],
                        "image_sha256": sha(image),
                        "scores": scores,
                        "blockers": [],
                        "improvements": [],
                        "verdict": "approve",
                        "confidence": 0.99,
                        "rationale": (
                            "The silhouette reads clearly at native size; material response, "
                            "lighting and value hierarchy are coherent. No production defects remain."
                        ),
                    }
                    for reviewer in reviewers
                ]
            },
        }
        review_path = root / "reviews.json"
        review_path.write_text(json.dumps(review_doc))
        return manifest, jobs, pack, review_path, image, review_doc, jid, raw_art, raw_red

    def test_independent_dual_10_of_10_reviews_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest, jobs, pack, reviews, *_ = self.fixture(Path(directory))
            result = review_gate.evaluate(manifest, jobs, pack, reviews)
            self.assertEqual(result["result"], "PASS")
            self.assertEqual(result["schema_version"], 2)

    def test_one_nine_out_of_ten_rejects(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest, jobs, pack, reviews, _, doc, jid, *_ = self.fixture(Path(directory))
            doc["reviews"][jid][1]["scores"]["craftsmanship"] = 9
            reviews.write_text(json.dumps(doc))
            self.assertEqual(review_gate.evaluate(manifest, jobs, pack, reviews)["result"], "REJECT")

    def test_stale_image_hash_rejects(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest, jobs, pack, reviews, _, doc, jid, *_ = self.fixture(Path(directory))
            doc["reviews"][jid][0]["image_sha256"] = "0" * 64
            reviews.write_text(json.dumps(doc))
            self.assertEqual(review_gate.evaluate(manifest, jobs, pack, reviews)["result"], "REJECT")

    def test_same_model_cannot_pose_as_independent_reviewers(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest, jobs, pack, reviews, _, doc, *_ = self.fixture(Path(directory))
            doc["reviewers"][1]["model"] = doc["reviewers"][0]["model"]
            reviews.write_text(json.dumps(doc))
            with self.assertRaisesRegex(ValueError, "distinct IDs, model identifiers, roles and session IDs"):
                review_gate.evaluate(manifest, jobs, pack, reviews)

    def test_duplicate_session_id_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest, jobs, pack, reviews, _, doc, *_ = self.fixture(Path(directory))
            doc["reviewers"][1]["session_id"] = doc["reviewers"][0]["session_id"]
            reviews.write_text(json.dumps(doc))
            with self.assertRaisesRegex(ValueError, "distinct IDs, model identifiers, roles and session IDs"):
                review_gate.evaluate(manifest, jobs, pack, reviews)

    def test_review_prompt_hash_mismatch_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest, jobs, pack, reviews, _, doc, *_ = self.fixture(Path(directory))
            doc["reviewers"][0]["review_prompt"] += " Tampered."
            reviews.write_text(json.dumps(doc))
            with self.assertRaisesRegex(ValueError, "review prompt hash mismatch"):
                review_gate.evaluate(manifest, jobs, pack, reviews)

    def test_raw_response_hash_mismatch_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest, jobs, pack, reviews, _, _, _, raw_art, _ = self.fixture(Path(directory))
            raw_art.write_text("tampered after the review was recorded")
            with self.assertRaisesRegex(ValueError, "raw reviewer response hash mismatch"):
                review_gate.evaluate(manifest, jobs, pack, reviews)

    def test_raw_response_path_traversal_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest, jobs, pack, reviews, _, doc, *_ = self.fixture(Path(directory))
            for unsafe in ("../outside.md", "C:/outside.md"):
                doc["reviewers"][0]["raw_response_file"] = unsafe
                reviews.write_text(json.dumps(doc))
                with self.subTest(path=unsafe):
                    with self.assertRaisesRegex(ValueError, "unsafe reviewer evidence path"):
                        review_gate.evaluate(manifest, jobs, pack, reviews)

    def test_omitted_asset_review_rejects(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest, jobs, pack, reviews, _, doc, *_ = self.fixture(Path(directory))
            doc["reviews"] = {}
            reviews.write_text(json.dumps(doc))
            self.assertEqual(review_gate.evaluate(manifest, jobs, pack, reviews)["result"], "REJECT")

    def test_manifest_path_traversal_is_rejected_before_review(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest, jobs, pack, reviews, _, _, *_ = self.fixture(Path(directory))
            manifest_doc = json.loads(manifest.read_text())
            manifest_doc["assets"][0]["file"] = "../outside.png"
            manifest.write_text(json.dumps(manifest_doc))
            with self.assertRaisesRegex(ValueError, "manifest/jobs plan verification failed"):
                review_gate.evaluate(manifest, jobs, pack, reviews)

    def test_symlinked_asset_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest, jobs, pack, reviews, image, _, *_ = self.fixture(Path(directory))
            outside = Path(directory) / "outside.png"
            outside.write_bytes(b"outside")
            image.unlink()
            try:
                image.symlink_to(outside)
            except (OSError, NotImplementedError):
                self.skipTest("symlink creation unavailable")
            result = review_gate.evaluate(manifest, jobs, pack, reviews)
            self.assertEqual(result["result"], "REJECT")
            self.assertTrue(any("symlink is not allowed" in failure for failure in result["failures"]))

    def test_tampered_or_incomplete_job_plan_cannot_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest, jobs, pack, reviews, _, doc, *_ = self.fixture(Path(directory))
            jobs_doc = json.loads(jobs.read_text())
            jobs_doc["jobs"] = []
            jobs_doc["job_count"] = 0
            jobs.write_text(json.dumps(jobs_doc))
            doc["jobs_sha256"] = sha(jobs)
            reviews.write_text(json.dumps(doc))
            with self.assertRaisesRegex(ValueError, "manifest/jobs plan verification failed"):
                review_gate.evaluate(manifest, jobs, pack, reviews)

    def test_wrong_job_output_cannot_review_different_file(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest, jobs, pack, reviews, _, doc, *_ = self.fixture(Path(directory))
            jobs_doc = json.loads(jobs.read_text())
            jobs_doc["jobs"][0]["output"] = "other.png"
            jobs.write_text(json.dumps(jobs_doc))
            doc["jobs_sha256"] = sha(jobs)
            reviews.write_text(json.dumps(doc))
            with self.assertRaisesRegex(ValueError, "manifest/jobs plan verification failed"):
                review_gate.evaluate(manifest, jobs, pack, reviews)


if __name__ == "__main__":
    unittest.main()
