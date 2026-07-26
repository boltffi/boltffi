from __future__ import annotations

import unittest

from jmh_to_benchmark_run import build_variant


class JmhMetricTests(unittest.TestCase):
    def test_uses_raw_samples_for_standard_deviation(self) -> None:
        entry = {
            "benchmark": "example.Benchmark.boltffi_java_noop",
            "mode": "avgt",
            "primaryMetric": {
                "score": 2.0,
                "scoreError": 99.0,
                "scoreConfidence": [1.0, 3.0],
                "scorePercentiles": {},
                "scoreUnit": "ns/op",
                "rawData": [[1.0, 2.0, 3.0]],
            },
        }
        subject = {
            "tool": {
                "name": "boltffi",
                "version": "0",
                "git_sha": None,
                "crate_version": "0",
            },
            "ffi": {},
            "attributes": {"subject_key": "boltffi_java"},
        }

        variant = build_variant(
            entry=entry,
            subject_prefix="boltffi_java",
            subject_config=subject,
            git={"commit_sha": "local"},
            rust_details=None,
            profile="release",
        )

        self.assertEqual(1.0, variant["metrics"]["std_dev"])

    def test_single_sample_has_no_standard_deviation(self) -> None:
        entry = {
            "benchmark": "example.Benchmark.boltffi_java_noop",
            "mode": "avgt",
            "primaryMetric": {
                "score": 2.0,
                "scoreError": 99.0,
                "scoreConfidence": [1.0, 3.0],
                "scorePercentiles": {},
                "scoreUnit": "ns/op",
                "rawData": [[2.0]],
            },
        }
        subject = {
            "tool": {
                "name": "boltffi",
                "version": "0",
                "git_sha": None,
                "crate_version": "0",
            },
            "ffi": {},
            "attributes": {"subject_key": "boltffi_java"},
        }

        variant = build_variant(
            entry=entry,
            subject_prefix="boltffi_java",
            subject_config=subject,
            git={"commit_sha": "local"},
            rust_details=None,
            profile="release",
        )

        self.assertIsNone(variant["metrics"]["std_dev"])


if __name__ == "__main__":
    unittest.main()
