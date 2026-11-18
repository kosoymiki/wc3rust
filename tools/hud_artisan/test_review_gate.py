import hashlib, json, tempfile, unittest
from pathlib import Path
import review_gate

def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
class ReviewGateTests(unittest.TestCase):
    def fixture(self, root):
        manifest=root/"manifest.json";jobs=root/"jobs.json";pack=root/"pack";pack.mkdir()
        image=pack/"icons"/"gold.png";image.parent.mkdir();image.write_bytes(b"fake-png-bytes-for-hash-binding")
        m={"assets":[{"name":"icon_gold","file":"icons/gold.png"}]}
        manifest.write_text(json.dumps(m))
        j={"source_manifest_sha256":sha(manifest),"jobs":[{"job_id":"0123456789abcdef","asset":"icon_gold","output":"icons/gold.png"}]}
        jobs.write_text(json.dumps(j))
        reviewers=[{"reviewer_id":"art-director-1","model":"vision-model-A","role":"art_director","independent":True},
                   {"reviewer_id":"red-team-1","model":"vision-model-B","role":"red_team","independent":True}]
        scores={k:10 for k in review_gate.CRITERIA}
        reviews={"schema_version":1,"manifest_sha256":sha(manifest),"jobs_sha256":sha(jobs),"reviewers":reviewers,
          "reviews":{"0123456789abcdef":[
            {"reviewer_id":r["reviewer_id"],"image_sha256":sha(image),"scores":scores,"blockers":[],"improvements":[],
             "verdict":"approve","confidence":0.99,"rationale":"The silhouette reads clearly at native size; material response, lighting and value hierarchy are coherent. No production defects remain."}
            for r in reviewers]}}
        rp=root/"reviews.json";rp.write_text(json.dumps(reviews))
        return manifest,jobs,pack,rp,image,reviews
    def test_independent_dual_10_of_10_reviews_pass(self):
        with tempfile.TemporaryDirectory() as d:
            m,j,p,r,*_=self.fixture(Path(d))
            result=review_gate.evaluate(m,j,p,r)
            self.assertEqual(result["result"],"PASS")
    def test_one_nine_out_of_ten_rejects(self):
        with tempfile.TemporaryDirectory() as d:
            m,j,p,r,_,doc=self.fixture(Path(d))
            doc["reviews"]["0123456789abcdef"][1]["scores"]["craftsmanship"]=9
            r.write_text(json.dumps(doc))
            self.assertEqual(review_gate.evaluate(m,j,p,r)["result"],"REJECT")
    def test_stale_image_hash_rejects(self):
        with tempfile.TemporaryDirectory() as d:
            m,j,p,r,_,doc=self.fixture(Path(d))
            doc["reviews"]["0123456789abcdef"][0]["image_sha256"]="0"*64
            r.write_text(json.dumps(doc))
            self.assertEqual(review_gate.evaluate(m,j,p,r)["result"],"REJECT")
    def test_same_model_cannot_pose_as_independent_reviewers(self):
        with tempfile.TemporaryDirectory() as d:
            m,j,p,r,_,doc=self.fixture(Path(d))
            doc["reviewers"][1]["model"]=doc["reviewers"][0]["model"]
            r.write_text(json.dumps(doc))
            with self.assertRaisesRegex(ValueError,"distinct IDs and model"):
                review_gate.evaluate(m,j,p,r)
    def test_omitted_asset_review_rejects(self):
        with tempfile.TemporaryDirectory() as d:
            m,j,p,r,_,doc=self.fixture(Path(d))
            doc["reviews"]={}
            r.write_text(json.dumps(doc))
            self.assertEqual(review_gate.evaluate(m,j,p,r)["result"],"REJECT")
if __name__=="__main__": unittest.main()
