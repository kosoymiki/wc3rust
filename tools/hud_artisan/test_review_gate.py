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
        reviewers = [
            {"reviewer_id": "art-director-1", "model": "vision-model-A", "role": "art_director", "independent": True},
            {"reviewer_id": "red-team-1", "model": "vision-model-B", "role": "red_team", "independent": True},
        ]
        scores = {key: 10 for key in review_gate.CRITERIA}
        review_doc = {
            "schema_version": 1,
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
        return manifest, jobs, pack, review_path, image, review_doc, jid

    def test_independent_dual_10_of_10_reviews_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest, jobs, pack, reviews, *_ = self.fixture(Path(directory))
            result = review_gate.evaluate(manifest, jobs, pack, reviews)
            self.assertEqual(result["result"], "PASS")

    def test_one_nine_out_of_ten_rejects(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest, jobs, pack, reviews, _, doc, jid = self.fixture(Path(directory))
            doc["reviews"][jid][1]["scores"]["craftsmanship"] = 9
            reviews.write_text(json.dumps(doc))
            self.assertEqual(review_gate.evaluate(manifest, jobs, pack, reviews)["result"], "REJECT")

    def test_stale_image_hash_rejects(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest, jobs, pack, reviews, _, doc, jid = self.fixture(Path(directory))
            doc["reviews"][jid][0]["image_sha256"] = "0" * 64
            reviews.write_text(json.dumps(doc))
            self.assertEqual(review_gate.evaluate(manifest, jobs, pack, reviews)["result"], "REJECT")

    def test_same_model_cannot_pose_as_independent_reviewers(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest, jobs, pack, reviews, _, doc, _ = self.fixture(Path(directory))
            doc["reviewers"][1]["model"] = doc["reviewers"][0]["model"]
            reviews.write_text(json.dumps(doc))
            with self.assertRaisesRegex(ValueError, "distinct IDs, model identifiers and roles"):
                review_gate.evaluate(manifest, jobs, pack, reviews)

    def test_omitted_asset_review_rejects(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest, jobs, pack, reviews, _, doc, _ = self.fixture(Path(directory))
            doc["reviews"] = {}
            reviews.write_text(json.dumps(doc))
            self.assertEqual(review_gate.evaluate(manifest, jobs, pack, reviews)["result"], "REJECT")

    def test_manifest_path_traversal_is_rejected_before_review(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest, jobs, pack, reviews, _, _, _ = self.fixture(Path(directory))
            manifest_doc = json.loads(manifest.read_text())
            manifest_doc["assets"][0]["file"] = "../outside.png"
            manifest.write_text(json.dumps(manifest_doc))
            with self.assertRaisesRegex(ValueError, "manifest/jobs plan verification failed"):
                review_gate.evaluate(manifest, jobs, pack, reviews)

    def test_symlinked_asset_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest, jobs, pack, reviews, image, _, _ = self.fixture(Path(directory))
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
            manifest, jobs, pack, reviews, _, doc, _ = self.fixture(Path(directory))
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
            manifest, jobs, pack, reviews, _, doc, _ = self.fixture(Path(directory))
            jobs_doc = json.loads(jobs.read_text())
            jobs_doc["jobs"][0]["output"] = "other.png"
            jobs.write_text(json.dumps(jobs_doc))
            doc["jobs_sha256"] = sha(jobs)
            reviews.write_text(json.dumps(doc))
            with self.assertRaisesRegex(ValueError, "manifest/jobs plan verification failed"):
                review_gate.evaluate(manifest, jobs, pack, reviews)


if __name__ == "__main__":
    unittest.main()
