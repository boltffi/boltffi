from __future__ import annotations

import argparse
import re
import sys
import tomllib
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[3]
DEMO_ROOT = REPO_ROOT / "examples/demo"
BENCHMARK_SCRIPTS = REPO_ROOT / "benchmarks/scripts"

# Share the existing demo export parser until the inventory/audit tooling moves
# together into a Rust CLI.
sys.path.insert(0, str(BENCHMARK_SCRIPTS))
from demo_export_inventory import iter_demo_exports  # noqa: E402


CASE_ID_RE = re.compile(r"^[a-z0-9][a-z0-9_.-]*$")
CASE_MARKER_RE = re.compile(r"\bcase:([A-Za-z0-9_.-]+)\b")


@dataclass(frozen=True)
class PlatformScan:
    test_roots: tuple[Path, ...]
    file_suffixes: tuple[str, ...]


PLATFORM_SCANS = {
    "apple": PlatformScan(
        test_roots=(REPO_ROOT / "examples/platforms/apple/Tests",),
        file_suffixes=(".swift",),
    ),
    "kotlin": PlatformScan(
        test_roots=(REPO_ROOT / "examples/platforms/kotlin/src/test",),
        file_suffixes=(".kt",),
    ),
    "java": PlatformScan(
        test_roots=(REPO_ROOT / "examples/platforms/java",),
        file_suffixes=(".java",),
    ),
    "csharp": PlatformScan(
        test_roots=(REPO_ROOT / "examples/platforms/csharp/DemoTest",),
        file_suffixes=(".cs",),
    ),
    "wasm": PlatformScan(
        test_roots=(REPO_ROOT / "examples/platforms/wasm/tests",),
        file_suffixes=(".js", ".mjs", ".ts"),
    ),
    "python": PlatformScan(
        test_roots=(REPO_ROOT / "examples/platforms/python/tests",),
        file_suffixes=(".py",),
    ),
}


@dataclass(frozen=True)
class DemoTestCase:
    case_id: str
    summary: str
    description: str
    exercises: tuple[str, ...]
    excluded_platforms: dict[str, str] = field(default_factory=dict)
    source_file: Path = Path()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Audit semantic demo test cases against platform test markers."
    )
    parser.add_argument(
        "--manifest",
        type=Path,
        default=DEMO_ROOT / "tests.toml",
        help="Path to the root demo test manifest.",
    )
    parser.add_argument(
        "--show-unexercised-exports",
        action="store_true",
        help="Print demo exports that are not referenced by any manifest case.",
    )
    return parser.parse_args()


def load_toml(path: Path) -> dict[str, Any]:
    try:
        with path.open("rb") as manifest_file:
            return tomllib.load(manifest_file)
    except tomllib.TOMLDecodeError as error:
        raise ValueError(f"{path}: invalid TOML: {error}") from error


def load_manifest(manifest_path: Path) -> tuple[list[str], dict[str, DemoTestCase]]:
    root = load_toml(manifest_path)
    schema_version = root.get("schema_version")
    if schema_version != "demo_tests_v1":
        raise ValueError(
            f"{manifest_path}: schema_version must be 'demo_tests_v1', got {schema_version!r}"
        )

    platforms = require_string_list(manifest_path, root, "platforms")
    unknown_platforms = sorted(set(platforms) - PLATFORM_SCANS.keys())
    if unknown_platforms:
        raise ValueError(
            f"{manifest_path}: platforms are not configured for scanning: {', '.join(unknown_platforms)}"
        )

    case_files = require_string_list(manifest_path, root, "case_files")
    cases: dict[str, DemoTestCase] = {}
    for case_file in case_files:
        path = resolve_manifest_relative_path(manifest_path.parent, case_file)
        category_cases = load_case_file(path, set(platforms))
        for case_id, case in category_cases.items():
            if case_id in cases:
                raise ValueError(
                    f"{path}: duplicate case id {case_id!r}; first defined in {cases[case_id].source_file}"
                )
            cases[case_id] = case

    if not cases:
        raise ValueError(f"{manifest_path}: at least one demo test case is required")

    return platforms, cases


def require_string_list(path: Path, data: dict[str, Any], key: str) -> list[str]:
    value = data.get(key)
    if not isinstance(value, list) or not value or not all(isinstance(item, str) for item in value):
        raise ValueError(f"{path}: {key} must be a non-empty list of strings")
    if len(set(value)) != len(value):
        raise ValueError(f"{path}: {key} contains duplicate entries")
    return value


def resolve_manifest_relative_path(base: Path, raw_path: str) -> Path:
    path = (base / raw_path).resolve()
    try:
        path.relative_to(DEMO_ROOT.resolve())
    except ValueError as error:
        raise ValueError(f"{raw_path!r} escapes {DEMO_ROOT}") from error
    if not path.is_file():
        raise ValueError(f"{path}: case file does not exist")
    return path


def load_case_file(path: Path, platforms: set[str]) -> dict[str, DemoTestCase]:
    payload = load_toml(path)
    raw_cases = payload.get("cases")
    if not isinstance(raw_cases, dict) or not raw_cases:
        raise ValueError(f"{path}: cases must be a non-empty table")

    cases: dict[str, DemoTestCase] = {}
    for case_id, raw_case in raw_cases.items():
        cases[case_id] = parse_case(path, case_id, raw_case, platforms)
    return cases


def parse_case(path: Path, case_id: str, raw_case: object, platforms: set[str]) -> DemoTestCase:
    if not isinstance(raw_case, dict):
        raise ValueError(f"{path}: case {case_id!r} must be a table")
    if not CASE_ID_RE.match(case_id):
        raise ValueError(f"{path}: case id {case_id!r} must use lowercase slug characters")

    summary = require_non_empty_string(path, raw_case, case_id, "summary")
    description = require_non_empty_string(path, raw_case, case_id, "description")
    exercises = tuple(require_string_list_for_case(path, raw_case, case_id, "exercises"))

    excluded_platforms = raw_case.get("excluded_platforms", {})
    if not isinstance(excluded_platforms, dict):
        raise ValueError(f"{path}: case {case_id!r} excluded_platforms must be a table")

    unknown_exclusions = sorted(set(excluded_platforms) - platforms)
    if unknown_exclusions:
        raise ValueError(
            f"{path}: case {case_id!r} excludes unknown platforms: {', '.join(unknown_exclusions)}"
        )

    normalized_exclusions: dict[str, str] = {}
    for platform, reason in excluded_platforms.items():
        if not isinstance(reason, str) or not reason.strip():
            raise ValueError(
                f"{path}: case {case_id!r} exclusion for {platform!r} must include a reason"
            )
        normalized_exclusions[platform] = reason.strip()

    return DemoTestCase(
        case_id=case_id,
        summary=summary,
        description=description,
        exercises=exercises,
        excluded_platforms=normalized_exclusions,
        source_file=path,
    )


def require_non_empty_string(path: Path, raw_case: dict[str, Any], case_id: str, key: str) -> str:
    value = raw_case.get(key)
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{path}: case {case_id!r} {key} must be a non-empty string")
    return value.strip()


def require_string_list_for_case(
    path: Path,
    raw_case: dict[str, Any],
    case_id: str,
    key: str,
) -> list[str]:
    value = raw_case.get(key)
    if not isinstance(value, list) or not value or not all(isinstance(item, str) for item in value):
        raise ValueError(f"{path}: case {case_id!r} {key} must be a non-empty list of strings")
    if any(not item.strip() for item in value):
        raise ValueError(f"{path}: case {case_id!r} {key} contains an empty string")
    return [item.strip() for item in value]


def collect_platform_markers(platforms: list[str]) -> dict[str, set[str]]:
    markers: dict[str, set[str]] = {}
    for platform in platforms:
        platform_markers: set[str] = set()
        scan = PLATFORM_SCANS[platform]
        for root in scan.test_roots:
            if not root.exists():
                continue
            for path in sorted(root.rglob("*")):
                if path.is_file() and path.suffix in scan.file_suffixes:
                    platform_markers.update(CASE_MARKER_RE.findall(path.read_text(encoding="utf-8")))
        markers[platform] = platform_markers
    return markers


def validate_exercises(cases: dict[str, DemoTestCase]) -> list[str]:
    known_exports = {export.export_id for export in iter_demo_exports()}
    errors: list[str] = []
    for case in cases.values():
        for export_id in case.exercises:
            if export_id not in known_exports:
                errors.append(
                    f"{case.source_file}: case {case.case_id!r} exercises unknown export {export_id!r}"
                )
    return errors


def validate_platform_markers(
    platforms: list[str],
    cases: dict[str, DemoTestCase],
    markers: dict[str, set[str]],
) -> list[str]:
    errors: list[str] = []
    known_case_ids = set(cases)

    for platform, platform_markers in markers.items():
        unknown_markers = sorted(platform_markers - known_case_ids)
        for marker in unknown_markers:
            errors.append(f"{platform}: marker references unknown case {marker!r}")

    for case_id, case in sorted(cases.items()):
        for platform in platforms:
            present = case_id in markers[platform]
            excluded = platform in case.excluded_platforms
            if excluded and present:
                errors.append(f"{platform}: case {case_id!r} is excluded but has a test marker")
            elif not excluded and not present:
                errors.append(f"{platform}: missing required marker for case {case_id!r}")

    return errors


def print_summary(platforms: list[str], cases: dict[str, DemoTestCase], markers: dict[str, set[str]]) -> None:
    print(f"demo test cases: {len(cases)}")
    print(f"platforms: {', '.join(platforms)}")
    print()
    for platform in platforms:
        required_cases = {
            case_id for case_id, case in cases.items() if platform not in case.excluded_platforms
        }
        covered = markers[platform] & required_cases
        print(f"{platform}: {len(covered)}/{len(required_cases)} required cases marked")


def print_unexercised_exports(cases: dict[str, DemoTestCase]) -> None:
    exercised = {export_id for case in cases.values() for export_id in case.exercises}
    unexercised = [
        export.export_id for export in iter_demo_exports() if export.export_id not in exercised
    ]
    print()
    print(f"demo exports not referenced by manifest cases: {len(unexercised)}")
    for export_id in unexercised:
        print(f"  {export_id}")


def main() -> int:
    args = parse_args()
    try:
        platforms, cases = load_manifest(args.manifest.resolve())
        markers = collect_platform_markers(platforms)
        errors = [
            *validate_exercises(cases),
            *validate_platform_markers(platforms, cases, markers),
        ]
    except ValueError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2

    print_summary(platforms, cases, markers)
    if args.show_unexercised_exports:
        print_unexercised_exports(cases)

    if errors:
        print()
        print("demo test audit failures:", file=sys.stderr)
        for error in errors:
            print(f"  {error}", file=sys.stderr)
        return 1

    print()
    print("demo test audit passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
