#!/usr/bin/env python3
"""Reference-ledger schema and licensing policy tests."""
import json
import unittest
from pathlib import Path
from jsonschema import Draft202012Validator

ROOT = Path(__file__).parent

class ReferenceLedgerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.schema = json.loads((ROOT / "reference_ledger.schema.json").read_text(encoding="utf-8"))
        cls.example = json.loads((ROOT / "reference_ledger.example.json").read_text(encoding="utf-8"))
        cls.ledger = json.loads((ROOT / "reference_ledger.json").read_text(encoding="utf-8"))
        Draft202012Validator.check_schema(cls.schema)
        cls.validator = Draft202012Validator(cls.schema)

    def test_example_is_valid(self):
        self.validator.validate(self.example)

    def test_populated_donor_ledger_is_valid_and_complete(self):
        self.validator.validate(self.ledger)
        self.assertGreaterEqual(len(self.ledger["references"]), 15)
        self.assertEqual(len({item["id"] for item in self.ledger["references"]}), len(self.ledger["references"]))

    def test_unknown_license_cannot_be_conditioned(self):
        doc = json.loads(json.dumps(self.example))
        item = doc["references"][0]
        item["license"] = "UNKNOWN"
        item["allowed_use"] = "pixel_conditioning"
        item["conditioning_allowed"] = True
        with self.assertRaises(Exception):
            self.validator.validate(doc)

    def test_unknown_license_is_allowed_as_text_only(self):
        doc = json.loads(json.dumps(self.example))
        item = doc["references"][0]
        item["license"] = "UNKNOWN"
        item["allowed_use"] = "textual_reference"
        item["conditioning_allowed"] = False
        self.validator.validate(doc)

    def test_proprietary_reference_cannot_be_pixel_conditioning(self):
        doc = json.loads(json.dumps(self.example))
        item = doc["references"][0]
        item["license"] = "PROPRIETARY"
        item["allowed_use"] = "pixel_conditioning"
        item["conditioning_allowed"] = True
        with self.assertRaises(Exception):
            self.validator.validate(doc)

    def test_required_provenance_fields(self):
        doc = json.loads(json.dumps(self.example))
        del doc["references"][0]["redistribution"]
        with self.assertRaises(Exception):
            self.validator.validate(doc)

if __name__ == "__main__":
    unittest.main()
