from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import asdict, dataclass
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
DEMO_SRC = REPO_ROOT / "examples/demo/src"

FUNCTION_RE = re.compile(r"^\s*pub\s+(?:async\s+)?fn\s+([A-Za-z0-9_]+)\s*\(")
METHOD_RE = re.compile(r"^\s*pub\s+(?:async\s+)?fn\s+([A-Za-z0-9_]+)\s*\(")
TRAIT_RE = re.compile(r"^\s*pub\s+trait\s+([A-Za-z0-9_]+)\b")
TRAIT_METHOD_RE = re.compile(r"^\s*(?:async\s+)?fn\s+([A-Za-z0-9_]+)\s*\(")
IMPL_RE = re.compile(r"^\s*impl(?:<[^>]+>)?\s+([A-Za-z0-9_]+)\s*\{")


@dataclass(frozen=True)
class DemoExport:
    export_id: str
    kind: str
    module: str
    owner: str | None
    name: str
    path: str
    line: int


@dataclass
class ExportBlock:
    kind: str
    owner: str
    depth: int


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Inventory callable exports in examples/demo.")
    parser.add_argument("--json", action="store_true", help="Emit the inventory as JSON.")
    return parser.parse_args()


def is_export_attr(line: str) -> bool:
    return ("#[export" in line or "uniffi::export" in line) and "callback_interface" not in line


def is_data_impl_attr(line: str) -> bool:
    return "#[data(impl)]" in line


def module_name_for(path: Path) -> str:
    relative = path.relative_to(DEMO_SRC)
    if relative.name == "lib.rs":
        return "crate"
    if relative.name == "mod.rs":
        parts = relative.parts[:-1]
    else:
        parts = relative.with_suffix("").parts
    return "::".join(parts)


def make_export_id(module: str, name: str, owner: str | None = None) -> str:
    if owner:
        return f"{module}::{owner}::{name}"
    return f"{module}::{name}"


def iter_demo_exports() -> tuple[DemoExport, ...]:
    seen: dict[str, DemoExport] = {}

    for path in sorted(DEMO_SRC.rglob("*.rs")):
        module = module_name_for(path)
        lines = path.read_text().splitlines()
        brace_depth = 0
        pending_attrs: list[str] = []
        current_block: ExportBlock | None = None

        for line_number, line in enumerate(lines, start=1):
            stripped = line.strip()

            if current_block is not None and brace_depth == current_block.depth:
                matcher = METHOD_RE if current_block.kind == "impl" else TRAIT_METHOD_RE
                if method_match := matcher.match(line):
                    name = method_match.group(1)
                    export = DemoExport(
                        export_id=make_export_id(module, name, current_block.owner),
                        kind="method" if current_block.kind == "impl" else "trait_method",
                        module=module,
                        owner=current_block.owner,
                        name=name,
                        path=str(path),
                        line=line_number,
                    )
                    seen.setdefault(export.export_id, export)

            if stripped.startswith("#["):
                pending_attrs.append(stripped)
            elif stripped == "" or stripped.startswith("//"):
                pass
            else:
                export_attrs = any(is_export_attr(attribute) for attribute in pending_attrs)
                data_impl_attrs = any(is_data_impl_attr(attribute) for attribute in pending_attrs)

                if export_attrs:
                    if function_match := FUNCTION_RE.match(line):
                        name = function_match.group(1)
                        export = DemoExport(
                            export_id=make_export_id(module, name),
                            kind="function",
                            module=module,
                            owner=None,
                            name=name,
                            path=str(path),
                            line=line_number,
                        )
                        seen.setdefault(export.export_id, export)
                    elif impl_match := IMPL_RE.match(line):
                        current_block = ExportBlock(
                            kind="impl",
                            owner=impl_match.group(1).split("::")[-1],
                            depth=brace_depth + line.count("{") - line.count("}"),
                        )
                    elif trait_match := TRAIT_RE.match(line):
                        current_block = ExportBlock(
                            kind="trait",
                            owner=trait_match.group(1),
                            depth=brace_depth + line.count("{") - line.count("}"),
                        )
                elif data_impl_attrs:
                    if impl_match := IMPL_RE.match(line):
                        current_block = ExportBlock(
                            kind="impl",
                            owner=impl_match.group(1).split("::")[-1],
                            depth=brace_depth + line.count("{") - line.count("}"),
                        )

                pending_attrs = []

            brace_depth += line.count("{") - line.count("}")
            if current_block is not None and brace_depth < current_block.depth:
                current_block = None

    return tuple(sorted(seen.values(), key=lambda export: export.export_id))


def print_text_inventory(exports: tuple[DemoExport, ...]) -> None:
    by_module: dict[str, list[DemoExport]] = {}
    for export in exports:
        by_module.setdefault(export.module, []).append(export)

    print(f"demo exports: {len(exports)} callable exports")
    print()
    for module, module_exports in sorted(by_module.items()):
        print(f"[{module}] {len(module_exports)}")
        for export in module_exports:
            owner = f"{export.owner}::" if export.owner else ""
            print(f"  {owner}{export.name}")
        print()


def main() -> int:
    args = parse_args()
    exports = iter_demo_exports()
    if args.json:
        json.dump([asdict(export) for export in exports], sys.stdout, indent=2)
        sys.stdout.write("\n")
    else:
        print_text_inventory(exports)
    return 0


if __name__ == "__main__":
    sys.exit(main())
