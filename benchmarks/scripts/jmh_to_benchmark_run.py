#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import statistics
from collections import defaultdict
from pathlib import Path
from typing import Any

from benchmark_schema import (
    REPO_ROOT,
    artifacts,
    build_run_id,
    ci_context,
    collector_context,
    crate_version,
    git_context,
    host_context,
    infer_descriptor,
    java_toolchain_from_jmh,
    kotlin_toolchain,
    rust_toolchain,
    sha256_file,
    utc_now,
)


BOLTFFI_VERSION = crate_version(REPO_ROOT / "boltffi/Cargo.toml")

SUITES = {
    "kotlin-jvm": {
        "suite_name": "kotlin-jvm-jmh",
        "language": "kotlin",
        "subject_order": ["boltffi", "uniffi"],
        "toolchains": lambda entry, rust_details: {
            "rust": rust_details,
            "swift": None,
            "kotlin": kotlin_toolchain(REPO_ROOT / "benchmarks/harnesses/kotlin-jvm-bench/build.gradle.kts"),
            "java": java_toolchain_from_jmh(entry.get("jvm"), entry.get("jdkVersion"), entry.get("vmName")),
            "node": None,
            "wasm": None,
        },
        "subjects": {
            "boltffi": {
                "tool": {
                    "name": "boltffi",
                    "version": BOLTFFI_VERSION,
                    "git_sha": "repository_head",
                    "crate_version": BOLTFFI_VERSION,
                },
                "ffi": {
                    "bridge": "boltffi",
                    "transport": "jni",
                    "ownership_model": None,
                    "attributes": {
                        "host_language": "kotlin",
                        "binding_runtime": "jni",
                    },
                },
                "attributes": {
                    "subject_key": "boltffi",
                    "binding_package": "BenchBoltFFI",
                },
            },
            "uniffi": {
                "tool": {
                    "name": "uniffi",
                    "version": "0.31.0",
                    "git_sha": None,
                    "crate_version": None,
                },
                "ffi": {
                    "bridge": "uniffi",
                    "transport": "jna",
                    "ownership_model": None,
                    "attributes": {
                        "host_language": "kotlin",
                        "binding_runtime": "jna",
                    },
                },
                "attributes": {
                    "subject_key": "uniffi",
                    "binding_package": "BenchUniffi",
                },
            },
        },
    },
    "java-jvm": {
        "suite_name": "java-jvm-jmh",
        "language": "java",
        "subject_order": ["boltffi_java", "uniffi_java"],
        "toolchains": lambda entry, rust_details: {
            "rust": rust_details,
            "swift": None,
            "kotlin": None,
            "java": java_toolchain_from_jmh(entry.get("jvm"), entry.get("jdkVersion"), entry.get("vmName")),
            "node": None,
            "wasm": None,
        },
        "subjects": {
            "boltffi_java": {
                "tool": {
                    "name": "boltffi",
                    "version": BOLTFFI_VERSION,
                    "git_sha": "repository_head",
                    "crate_version": BOLTFFI_VERSION,
                },
                "ffi": {
                    "bridge": "boltffi",
                    "transport": "jni",
                    "ownership_model": None,
                    "attributes": {
                        "host_language": "java",
                        "binding_runtime": "jni",
                    },
                },
                "attributes": {
                    "subject_key": "boltffi_java",
                    "binding_package": "com.example.bench_boltffi",
                    "generator": "binding_ir",
                },
            },
            "uniffi_java": {
                "tool": {
                    "name": "uniffi",
                    "version": "0.31.0",
                    "git_sha": None,
                    "crate_version": None,
                },
                "ffi": {
                    "bridge": "uniffi",
                    "transport": "ffm",
                    "ownership_model": None,
                    "attributes": {
                        "host_language": "java",
                        "binding_runtime": "foreign_function_memory_api",
                    },
                },
                "attributes": {
                    "subject_key": "uniffi_java",
                    "binding_package": "uniffi.demo",
                },
            },
        },
    },
}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--suite", choices=sorted(SUITES), required=True)
    parser.add_argument("--results", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--artifact", action="append", type=Path, default=[])
    parser.add_argument("--profile", default="release")
    args = parser.parse_args()

    config = SUITES[args.suite]
    results = json.loads(args.results.read_text())
    if not results:
        raise SystemExit(f"no JMH results found in {args.results}")

    git = git_context()
    collected_at = utc_now()
    rust_details = rust_toolchain()
    first_entry = results[0]
    grouped_cases: dict[str, list[dict[str, Any]]] = defaultdict(list)

    for entry in results:
        subject_prefix, case_name = split_subject(entry["benchmark"], config["subjects"])
        descriptor = infer_descriptor(case_name, "jvm", config["language"])
        grouped_cases[case_name].append(
            {
                "descriptor": descriptor,
                "variant": build_variant(
                    entry=entry,
                    subject_prefix=subject_prefix,
                    subject_config=config["subjects"][subject_prefix],
                    git=git,
                    rust_details=rust_details,
                    profile=args.profile,
                ),
            }
        )

    benchmarks = []
    for case_name in sorted(grouped_cases):
        case_entries = grouped_cases[case_name]
        benchmarks.append(
            {
                "descriptor": case_entries[0]["descriptor"],
                "variants": sorted_variants(case_entries, config["subject_order"]),
                "notes": [],
            }
        )

    run = {
        "schema_version": "benchmark_run_v1",
        "run_id": build_run_id(config["suite_name"], collected_at, git["commit_sha"]),
        "collected_at": collected_at,
        "provenance": {
            "repository": git,
            "collector": collector_context(
                invocation=f"jmh_to_benchmark_run.py --suite {args.suite} --results {args.results}"
            ),
            "artifacts": artifacts([args.results, *args.artifact]),
        },
        "environment": {
            "host": host_context(),
            "toolchains": config["toolchains"](first_entry, rust_details),
            "runtime": {
                "engine": "jvm",
                "version": first_entry.get("vmVersion"),
                "platform": "jvm",
                "attributes": build_runtime_attributes(first_entry),
            },
            "ci": ci_context(),
        },
        "suite": {
            "name": config["suite_name"],
            "harness": "jmh",
            "platform": "jvm",
            "language": config["language"],
            "profile": args.profile,
            "tags": [],
            "attributes": build_suite_attributes(args.results, results),
        },
        "benchmarks": benchmarks,
        "notes": [],
    }

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(run, indent=2) + "\n")


def build_variant(
    *,
    entry: dict[str, Any],
    subject_prefix: str,
    subject_config: dict[str, Any],
    git: dict[str, Any],
    rust_details: dict[str, Any] | None,
    profile: str,
) -> dict[str, Any]:
    primary_metric = entry["primaryMetric"]
    raw_samples = flatten_raw_samples(primary_metric)
    confidence_interval = primary_metric.get("scoreConfidence")

    variant_notes = []
    if confidence_interval and len(confidence_interval) == 2:
        variant_notes.append(
            "jmh_score_confidence="
            f"[{float(confidence_interval[0]):.6f}, {float(confidence_interval[1]):.6f}] "
            f"{primary_metric.get('scoreUnit')}"
        )

    tool_identity = dict(subject_config["tool"])
    if tool_identity["git_sha"] == "repository_head":
        tool_identity["git_sha"] = git["commit_sha"]

    return {
        "subject": {
            "tool": tool_identity,
            "build": {
                "compiler_name": "rustc",
                "compiler_version": rust_details["rustc_version"] if rust_details else None,
                "target": rust_details["target_triple"] if rust_details else None,
                "profile": profile,
                "optimization": "release" if profile == "release" else "debug",
                "features": ["uniffi"] if subject_prefix.startswith("uniffi") else [],
                "flags": ["--release"] if profile == "release" else [],
            },
            "ffi": subject_config["ffi"],
            "attributes": {
                **subject_config["attributes"],
                "benchmark_class": benchmark_class(entry["benchmark"]),
                "benchmark_method": benchmark_method(entry["benchmark"]),
            },
        },
        "metrics": {
            "unit": metric_unit(primary_metric["scoreUnit"]),
            "estimator": estimator_for_mode(entry.get("mode")),
            "value": float(primary_metric["score"]),
            "std_dev": sample_standard_deviation(raw_samples),
            "min": min(raw_samples) if raw_samples else None,
            "max": max(raw_samples) if raw_samples else None,
            "percentiles": {
                key: float(value)
                for key, value in primary_metric.get("scorePercentiles", {}).items()
            },
        },
        "sampling": {
            "warmup_iterations": entry.get("warmupIterations"),
            "measurement_iterations": entry.get("measurementIterations"),
            "sample_count": len(raw_samples) if raw_samples else None,
            "total_operations": None,
        },
        "notes": variant_notes,
    }


def split_subject(benchmark_name: str, subjects: dict[str, Any]) -> tuple[str, str]:
    method_name = benchmark_method(benchmark_name)
    for prefix in sorted(subjects, key=len, reverse=True):
        marker = f"{prefix}_"
        if method_name.startswith(marker):
            return prefix, method_name[len(marker):]
    raise SystemExit(f"unable to determine subject for {benchmark_name}")


def flatten_raw_samples(primary_metric: dict[str, Any]) -> list[float]:
    raw_data = primary_metric.get("rawData") or []
    flattened_samples = [
        float(sample)
        for measurement_group in raw_data
        for sample in measurement_group
    ]
    if flattened_samples:
        return flattened_samples
    return [float(primary_metric["score"])]


def sample_standard_deviation(samples: list[float]) -> float | None:
    return statistics.stdev(samples) if len(samples) > 1 else None


def build_runtime_attributes(entry: dict[str, Any]) -> dict[str, Any]:
    jvm_arguments = entry.get("jvmArgs") or []
    return {
        "jmh_version": entry.get("jmhVersion"),
        "mode": entry.get("mode"),
        "threads": entry.get("threads"),
        "forks": entry.get("forks"),
        "jvm_path": entry.get("jvm"),
        "jvm_args": " ".join(jvm_arguments),
        "warmup_time": entry.get("warmupTime"),
        "warmup_batch_size": entry.get("warmupBatchSize"),
        "measurement_time": entry.get("measurementTime"),
        "measurement_batch_size": entry.get("measurementBatchSize"),
        "native_access_enabled": any("--enable-native-access" in arg for arg in jvm_arguments),
    }


def build_suite_attributes(results_path: Path, results: list[dict[str, Any]]) -> dict[str, Any]:
    first_entry = results[0]
    return {
        "results_sha256": sha256_file(results_path),
        "benchmark_count": len(results),
        "jmh_version": first_entry.get("jmhVersion"),
        "mode": first_entry.get("mode"),
        "threads": first_entry.get("threads"),
        "forks": first_entry.get("forks"),
        "warmup_iterations": first_entry.get("warmupIterations"),
        "warmup_time": first_entry.get("warmupTime"),
        "measurement_iterations": first_entry.get("measurementIterations"),
        "measurement_time": first_entry.get("measurementTime"),
    }


def benchmark_class(benchmark_name: str) -> str:
    return benchmark_name.rsplit(".", 1)[0]


def benchmark_method(benchmark_name: str) -> str:
    return benchmark_name.rsplit(".", 1)[-1]


def sorted_variants(case_entries: list[dict[str, Any]], subject_order: list[str]) -> list[dict[str, Any]]:
    ordering = {subject_key: index for index, subject_key in enumerate(subject_order)}
    return [
        case["variant"]
        for case in sorted(
            case_entries,
            key=lambda item: ordering.get(item["variant"]["subject"]["attributes"]["subject_key"], 99),
        )
    ]


def metric_unit(unit: str) -> str:
    mapping = {
        "ns/op": "ns_per_op",
        "us/op": "us_per_op",
        "ms/op": "ms_per_op",
        "ops/s": "ops_per_sec",
        "B/s": "bytes_per_sec",
    }
    return mapping.get(unit, "ns_per_op")


def estimator_for_mode(mode: str | None) -> str:
    if mode == "avgt":
        return "mean"
    if mode == "thrpt":
        return "throughput"
    return "mean"


if __name__ == "__main__":
    main()
