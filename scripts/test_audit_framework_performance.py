import base64
import contextlib
import csv
import gzip
import hashlib
import io
import json
from pathlib import Path
import statistics
import subprocess
import unittest
from unittest.mock import patch

from audit_framework_performance import MEASUREMENTS, command, verify_samples


class NativeReportValidation(unittest.TestCase):
    def test_each_configuration_requires_three_distinct_samples(self):
        complete = [{"operation": op, "sample": str(sample), "milliseconds": "1"}
                    for op in ["a", "b"] for sample in range(3)]
        verify_samples(complete)
        with self.assertRaisesRegex(RuntimeError, "incomplete"):
            verify_samples(complete[:-1])

    def test_duplicate_sample_is_not_complete(self):
        rows = [{"operation": "a", "sample": sample} for sample in ["0", "1", "1", "2"]]
        with self.assertRaisesRegex(RuntimeError, "duplicated"):
            verify_samples(rows)

    def test_operation_counts_cannot_change_between_samples(self):
        rows = [{"operation": "a", "sample": str(i), "observed": str(i)} for i in range(3)]
        with self.assertRaisesRegex(RuntimeError, "incomplete"):
            verify_samples(rows)

    def test_empty_and_duplicate_allocation_rows_are_rejected(self):
        with self.assertRaisesRegex(RuntimeError, "no metrics"):
            verify_samples([])
        row = {"case": "capture", "allocations": "42"}
        verify_samples([row])
        with self.assertRaisesRegex(RuntimeError, "duplicated"):
            verify_samples([row, row])

    def test_child_failure_is_propagated_with_diagnostics(self):
        child = subprocess.CompletedProcess(["benchmark"], 3, "partial metrics", "failure")
        with patch("audit_framework_performance.subprocess.run", return_value=child):
            with contextlib.redirect_stdout(io.StringIO()) as out:
                with self.assertRaisesRegex(RuntimeError, r"command failed \(3\)"):
                    command(["benchmark"], {})
        self.assertIn("partial metrics", out.getvalue())
        self.assertIn("failure", out.getvalue())


class SavedAuditEvidence(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        archive = json.loads(Path(__file__).with_name("framework_audit").joinpath("results.json").read_text())
        if archive["format"] != "gzip+base64":
            raise ValueError("unsupported evidence encoding")
        cls.files = {}
        for name, entry in archive["files"].items():
            data = gzip.decompress(base64.b64decode("".join(entry["gzip_base64"]), validate=True))
            if len(data) != entry["bytes"] or hashlib.sha256(data).hexdigest() != entry["sha256"]:
                raise ValueError(f"corrupt evidence: {name}")
            cls.files[name] = data.decode()

    def test_raw_runs_preserve_samples_and_identical_semantic_work(self):
        ignored = MEASUREMENTS | {"phase", "round", "sample"}
        for name, expected_rows in [("metrics.csv", 7704), ("repeat_metrics.csv", 7704),
                                    ("lag_threshold_metrics.csv", 288)]:
            rows = list(csv.DictReader(io.StringIO(self.files[name])))
            self.assertEqual(len(rows), expected_rows)
            runs, configurations = {}, {}
            for row in rows:
                row = {k: v for k, v in row.items() if v != ""}
                runs.setdefault((row["suite"], row["phase"], row["round"]), []).append(row)
                key = tuple(sorted((k, v) for k, v in row.items() if k not in ignored))
                configurations.setdefault(key, {}).setdefault(row["phase"], 0)
                configurations[key][row["phase"]] += 1
            for run in runs.values():
                verify_samples(run)
            for phases in configurations.values():
                self.assertEqual(set(phases), {"before", "after"})
                self.assertEqual(phases["before"], phases["after"])

    def test_every_saved_median_is_recomputed_from_all_matching_raw_samples(self):
        rows = list(csv.DictReader(io.StringIO(self.files["metrics.csv"])))
        ignored = MEASUREMENTS | {"phase", "round", "sample"}
        groups = {}
        for row in rows:
            key = tuple(sorted((k, v) for k, v in row.items() if k not in ignored and v != ""))
            groups.setdefault(key, {}).setdefault(row["phase"], []).append(row)
        summary = json.loads(self.files["summary.json"])
        self.assertEqual(len(summary), 826)
        statistics_keys = {"metric", "speedup"} | {phase + suffix for phase in ["before", "after"]
                                                  for suffix in ["", "_min", "_max", "_samples"]}
        for item in summary:
            key = tuple(sorted((k, v) for k, v in item.items() if k not in statistics_keys))
            for phase in ["before", "after"]:
                values = [float(row[item["metric"]]) for row in groups[key][phase]]
                self.assertEqual(item[phase], statistics.median(values))
                self.assertEqual(item[phase + "_samples"], len(values))
                self.assertEqual(item[phase + "_min"], min(values))
                self.assertEqual(item[phase + "_max"], max(values))


if __name__ == "__main__":
    unittest.main(defaultTest=[NativeReportValidation.__name__, SavedAuditEvidence.__name__])
