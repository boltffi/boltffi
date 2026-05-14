from __future__ import annotations

import argparse
import re
import sys
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path

from benchmark_catalog import lookup_case_spec
from dotnet_benchmark_names import method_name_to_case_id


REPO_ROOT = Path(__file__).resolve().parents[2]


@dataclass(frozen=True)
class HarnessSource:
    name: str
    path: Path | tuple[Path, ...]
    pattern: re.Pattern[str]
    strip_tool_prefix: bool = False


HARNESS_SOURCES: tuple[HarnessSource, ...] = (
    HarnessSource(
        name="swift_macos",
        path=(
            REPO_ROOT / "benchmarks/harnesses/swift-macos-bench/Sources/BoltFFI/main.swift",
            REPO_ROOT / "benchmarks/harnesses/swift-macos-bench/Sources/Uniffi/main.swift",
            REPO_ROOT / "benchmarks/harnesses/swift-macos-bench/Sources/AsyncRunner/main.swift",
        ),
        pattern=re.compile(r'benchmark\("([^"]+)"\)|name:\s*"([^"]+)"'),
        strip_tool_prefix=True,
    ),
    HarnessSource(
        name="kotlin_jmh",
        path=REPO_ROOT / "benchmarks/harnesses/kotlin-jvm-bench/src/jmh/kotlin/com/example/bench_compare/JmhBenchmarks.kt",
        pattern=re.compile(r"open fun ([A-Za-z0-9_]+)\("),
        strip_tool_prefix=True,
    ),
    HarnessSource(
        name="java_jmh",
        path=REPO_ROOT / "benchmarks/harnesses/java-jvm-bench/src/jmh/java/com/example/bench_compare/UniffiJavaBench.java",
        pattern=re.compile(r"public void ([A-Za-z0-9_]+)\("),
        strip_tool_prefix=True,
    ),
    HarnessSource(
        name="kotlin_cli",
        path=REPO_ROOT / "benchmarks/harnesses/kotlin-jvm-bench/src/main/kotlin/com/example/bench_compare/CompareMain.kt",
        pattern=re.compile(r'(?:pairedBenchmark|singleBenchmark)\(\s*"([^"]+)"'),
    ),
    HarnessSource(
        name="android_app",
        path=REPO_ROOT / "benchmarks/harnesses/android-app/app/src/main/java/com/boltffi/bench/Benchmarks.kt",
        pattern=re.compile(r'bench(?:BoltffiOnly)?\("([^"]+)"'),
    ),
    HarnessSource(
        name="ios_app",
        path=REPO_ROOT / "benchmarks/harnesses/ios-app/App/ContentView.swift",
        pattern=re.compile(r'Bench\(name: "([^"]+)"'),
    ),
    HarnessSource(
        name="wasm_benchmarkjs",
        path=REPO_ROOT / "benchmarks/harnesses/wasm-bench/bench.mjs",
        pattern=re.compile(r"(?:runSuite\(|name:\s*)'([^']+)'"),
    ),
    HarnessSource(
        name="python_pyperf",
        path=REPO_ROOT / "benchmarks/harnesses/python-bench/bench.py",
        pattern=re.compile(r'BenchmarkCase\(\s*"([^"]+)"'),
    ),
    HarnessSource(
        name="dotnet_benchmarkdotnet",
        path=(
            REPO_ROOT / "benchmarks/harnesses/dotnet-bench/WireReaderBenchmarks.cs",
            REPO_ROOT / "benchmarks/harnesses/dotnet-bench/EnumWireBenchmarks.cs",
            REPO_ROOT / "benchmarks/harnesses/dotnet-bench/SharedSurfaceBenchmarks.cs",
        ),
        pattern=re.compile(r"\[Benchmark\]\s+public\s+[^\n(]+?\s+([A-Za-z0-9_]+)\(", re.MULTILINE),
    ),
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Audit benchmark harness names against the shared catalog.")
    parser.add_argument("--fail-on-unknown", action="store_true", help="Exit non-zero if any harness emits uncataloged names.")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    unknown_count = 0

    for harness, base_names in collect_harness_case_names().items():
        unknown = sorted(name for name in set(base_names) if lookup_case_spec(name) is None)
        canonical_collisions = canonical_alias_collisions(base_names)

        print(f"[{harness.name}] {len(base_names)} emitted names, {len(set(base_names))} unique")
        if canonical_collisions:
            print(f"  alias collisions: {', '.join(canonical_collisions)}")
        if unknown:
            print(f"  unknown names: {', '.join(unknown)}")
            unknown_count += len(unknown)

        known_canonical = sorted(
            {
                case.canonical_name
                for name in base_names
                if (case := lookup_case_spec(name)) is not None
            }
        )
        print(f"  catalog coverage: {len(known_canonical)} canonical cases\n")

    if args.fail_on_unknown and unknown_count:
        return 1
    return 0


def extract_names(harness: HarnessSource) -> list[str]:
    paths = harness.path if isinstance(harness.path, tuple) else (harness.path,)
    names: list[str] = []
    for path in paths:
        content = path.read_text()
        for match in harness.pattern.findall(content):
            if isinstance(match, tuple):
                names.extend(group for group in match if group)
            else:
                names.append(match)
    if harness.name in {"kotlin_jmh", "java_jmh"}:
        names = [name for name in names if name.startswith(("boltffi_", "uniffi_", "ffm_"))]
    if harness.name == "dotnet_benchmarkdotnet":
        names = [method_name_to_case_id(name) for name in names if name != "Setup"]
    return names


def collect_harness_case_names() -> dict[HarnessSource, list[str]]:
    return {
        harness: [
            strip_tool_prefix(name) if harness.strip_tool_prefix else name
            for name in extract_names(harness)
        ]
        for harness in HARNESS_SOURCES
    }


def strip_tool_prefix(name: str) -> str:
    stripped = name
    changed = True
    while changed:
        changed = False
        for prefix in ("boltffi_", "uniffi_", "wasmbindgen_", "ffm_", "java_"):
            if stripped.startswith(prefix):
                stripped = stripped.removeprefix(prefix)
                changed = True
    return stripped


def canonical_alias_collisions(base_names: list[str]) -> list[str]:
    by_canonical: dict[str, set[str]] = defaultdict(set)
    for name in base_names:
        case = lookup_case_spec(name)
        if case is None:
            continue
        by_canonical[case.canonical_name].add(name)

    collisions = []
    for canonical_name, aliases in sorted(by_canonical.items()):
        if len(aliases) > 1:
            collisions.append(f"{canonical_name} <- {', '.join(sorted(aliases))}")
    return collisions


if __name__ == "__main__":
    sys.exit(main())
