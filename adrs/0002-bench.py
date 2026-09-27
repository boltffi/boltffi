"""Compile-time and size comparison behind ADR 0002.

Usage: python3 adrs/0002-bench.py <main checkout> <lanes checkout> [runs]
"""

import json
import os
import statistics
import subprocess
import sys
import tempfile
import time

MAIN, LANES = sys.argv[1], sys.argv[2]
RUNS = int(sys.argv[3]) if len(sys.argv) > 3 else 5
TREES = {"main": MAIN, "lanes": LANES}
TOUCH = "src/primitives/mod.rs"


def run(command, cwd):
    started = time.monotonic()
    result = subprocess.run(command, shell=True, cwd=cwd, capture_output=True, text=True)
    elapsed = time.monotonic() - started
    if result.returncode:
        sys.exit(f"`{command}` failed in {cwd}:\n{result.stderr[-3000:]}")
    return elapsed


def commit(tree):
    return subprocess.run(
        ["git", "rev-parse", "--short", "HEAD"], cwd=tree, capture_output=True, text=True
    ).stdout.strip()


def report(name, times):
    main, lanes = statistics.median(times["main"]), statistics.median(times["lanes"])
    raw = json.dumps({tree: [round(value, 2) for value in values] for tree, values in times.items()})
    print(f"{name:22s} main {main:6.2f}s  lanes {lanes:6.2f}s ({(lanes / main - 1) * 100:+.0f}%)  {raw}", flush=True)


def measure(name, directories, prepare, command):
    for directory in directories.values():
        run(prepare, directory)
        run(command, directory)
    times = {tree: [] for tree in directories}
    for _ in range(RUNS):
        for tree, directory in directories.items():
            run(prepare, directory)
            times[tree].append(run(command, directory))
    report(name, times)


def synthetic(shape, count):
    lines = ["use boltffi::*;"]
    for index in range(count):
        if shape == "flat":
            fields = "pub a: i32, pub b: f64, pub c: String"
        else:
            previous = f"pub prev: R{index - 1}, " if index else ""
            fields = f"{previous}pub a: i32, pub c: String"
        lines.append(f"#[data] #[derive(Clone)] pub struct R{index} {{ {fields} }}")
        lines.append(f"#[export] pub fn f{index}(r: R{index}) -> R{index} {{ r }}")
    return "\n".join(lines)


def scale_crate(directory, tree, source, scaffolding):
    os.makedirs(os.path.join(directory, "src"), exist_ok=True)
    with open(os.path.join(directory, "Cargo.toml"), "w") as manifest:
        manifest.write(
            '[package]\nname = "scale"\nversion = "0.1.0"\nedition = "2024"\n'
            '[lib]\ncrate-type = ["staticlib", "rlib"]\n'
            f'[dependencies]\nboltffi = {{ path = "{os.path.abspath(tree)}/boltffi" }}\n[workspace]\n'
        )
    with open(os.path.join(directory, "src/lib.rs"), "w") as lib:
        lib.write(("boltffi::scaffolding!();\n" if scaffolding else "") + source)
    return directory


print(f"main {commit(MAIN)}  lanes {commit(LANES)}  runs {RUNS}", flush=True)
demos = {tree: os.path.join(path, "examples/demo") for tree, path in TREES.items()}
measure("clean debug build", demos, "cargo clean -p demo -q", "cargo build -q --lib")
measure("clean release build", demos, "cargo clean -p demo -q --release", "cargo build -q --lib --release")
measure("touch debug build", demos, f"touch {TOUCH}", "cargo build -q --lib")
sizes = {}
for tree, demo in demos.items():
    for candidate in ("target/release", "../../target/release"):
        for name in ("libdemo.dylib", "libdemo.so", "demo.dll"):
            path = os.path.join(demo, candidate, name)
            if os.path.exists(path) and tree not in sizes:
                sizes[tree] = os.path.getsize(path)
print("release library bytes", sizes, flush=True)
with tempfile.TemporaryDirectory() as root:
    for shape, count in (("flat", 400), ("chain", 200)):
        source = synthetic(shape, count)
        crates = {
            tree: scale_crate(os.path.join(root, shape, tree), path, source, tree == "lanes")
            for tree, path in TREES.items()
        }
        measure(f"{shape} n={count}", crates, "cargo clean -q -p scale", "cargo build -q --lib")
