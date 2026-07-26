from __future__ import annotations

import argparse
import bisect
import dataclasses
import difflib
import enum
import os
import pathlib
import re
import subprocess
import sys
import tempfile
from collections.abc import Sequence
from functools import reduce
from itertools import chain, takewhile


BASE_RELEASE = 8
LATEST_RELEASE = 26
TOOL_DIRECTORY = pathlib.Path(__file__).resolve().parent
BACKEND_DIRECTORY = TOOL_DIRECTORY.parents[1]
TABLE_PATH = BACKEND_DIRECTORY / "src/target/java/syntax/unicode/tables.rs"
PROBE_PATH = TOOL_DIRECTORY / "IdentifierProbe.java"


class IdentifierProperty(enum.Enum):
    START = ("start", "IDENTIFIER_START")
    PART = ("part", "IDENTIFIER_PART")
    IGNORABLE = ("ignorable", "IDENTIFIER_IGNORABLE")

    def __init__(self, spelling: str, constant: str) -> None:
        self.spelling = spelling
        self.constant = constant

    @classmethod
    def parse(cls, spelling: str) -> IdentifierProperty:
        try:
            return next(property for property in cls if property.spelling == spelling)
        except StopIteration as error:
            raise ValueError(f"unknown identifier property {spelling}") from error


@dataclasses.dataclass(frozen=True, order=True)
class CodePointRange:
    start: int
    end: int

    def __post_init__(self) -> None:
        if self.start < 0 or self.start > self.end or self.end > 0x10FFFF:
            raise ValueError(f"invalid code point range {self.start:x}..{self.end:x}")
        if self.start <= 0xDFFF and self.end >= 0xD800:
            raise ValueError(f"surrogate code point range {self.start:x}..{self.end:x}")

    def difference(self, other: CodePointRange) -> tuple[CodePointRange, ...]:
        if other.end < self.start or other.start > self.end:
            return (self,)
        candidates = (
            (
                CodePointRange(self.start, other.start - 1)
                if self.start < other.start
                else None
            ),
            CodePointRange(other.end + 1, self.end) if other.end < self.end else None,
        )
        return tuple(candidate for candidate in candidates if candidate is not None)


@dataclasses.dataclass(frozen=True)
class CharacterSet:
    ranges: tuple[CodePointRange, ...]

    def __post_init__(self) -> None:
        if any(
            previous.end >= current.start
            for previous, current in zip(self.ranges, self.ranges[1:], strict=False)
        ):
            raise ValueError("character ranges must be ordered and disjoint")

    @classmethod
    def empty(cls) -> CharacterSet:
        return cls(())

    @classmethod
    def parse(cls, source: str, name: str) -> CharacterSet:
        match = re.search(
            rf"const {re.escape(name)}: CharacterSet = CharacterSet\(&\[(.*?)\]\);",
            source,
            re.S,
        )
        if match is None:
            raise ValueError(f"missing {name}")
        ranges = tuple(
            CodePointRange(int(start, 16), int(end, 16))
            for start, end in re.findall(
                r"\(0x([0-9a-f]+), 0x([0-9a-f]+)\)", match.group(1)
            )
        )
        return cls(ranges)

    def union(self, other: CharacterSet) -> CharacterSet:
        ordered = sorted((*self.ranges, *other.ranges))
        if not ordered:
            return CharacterSet.empty()
        merged = reduce(self.merge, ordered[1:], [ordered[0]])
        return CharacterSet(tuple(merged))

    def difference(self, other: CharacterSet) -> CharacterSet:
        exclusion_ends = tuple(codepoints.end for codepoints in other.ranges)

        def subtract(source: CodePointRange) -> tuple[CodePointRange, ...]:
            first = bisect.bisect_left(exclusion_ends, source.start)
            exclusions = takewhile(
                lambda exclusion: exclusion.start <= source.end,
                other.ranges[first:],
            )
            return reduce(
                lambda remaining, exclusion: tuple(
                    chain.from_iterable(
                        codepoints.difference(exclusion) for codepoints in remaining
                    )
                ),
                exclusions,
                (source,),
            )

        return CharacterSet(tuple(chain.from_iterable(map(subtract, self.ranges))))

    def apply(self, change: CharacterChange) -> CharacterSet:
        return self.union(change.additions).difference(change.removals)

    @staticmethod
    def merge(
        merged: list[CodePointRange], current: CodePointRange
    ) -> list[CodePointRange]:
        previous = merged[-1]
        if current.start <= previous.end + 1:
            merged[-1] = CodePointRange(previous.start, max(previous.end, current.end))
        else:
            merged.append(current)
        return merged

    def render(self, name: str) -> str:
        if not self.ranges:
            return f"const {name}: CharacterSet = CharacterSet(&[]);"
        if len(self.ranges) == 1:
            codepoints = self.ranges[0]
            return (
                f"const {name}: CharacterSet = "
                f"CharacterSet(&[(0x{codepoints.start:x}, 0x{codepoints.end:x})]);"
            )
        ranges = "\n".join(
            f"    (0x{codepoints.start:x}, 0x{codepoints.end:x}),"
            for codepoints in self.ranges
        )
        return "\n".join(
            (
                f"const {name}: CharacterSet = CharacterSet(&[",
                ranges,
                "]);",
            )
        )


@dataclasses.dataclass(frozen=True)
class CharacterChange:
    additions: CharacterSet
    removals: CharacterSet

    def __post_init__(self) -> None:
        if self.additions.difference(self.removals) != self.additions:
            raise ValueError("identifier additions and removals must be disjoint")

    @classmethod
    def between(cls, previous: CharacterSet, current: CharacterSet) -> CharacterChange:
        return cls(
            additions=current.difference(previous),
            removals=previous.difference(current),
        )

    @classmethod
    def parse(
        cls, source: str, property: IdentifierProperty, index: int
    ) -> CharacterChange:
        return cls(
            additions=CharacterSet.parse(
                source, f"{property.constant}_ADDITIONS_{index}"
            ),
            removals=CharacterSet.parse(
                source, f"{property.constant}_REMOVALS_{index}"
            ),
        )

    def changed(self) -> bool:
        return bool(self.additions.ranges or self.removals.ranges)


@dataclasses.dataclass(frozen=True)
class ProbeRange:
    property: IdentifierProperty
    codepoints: CodePointRange

    @classmethod
    def parse(cls, fields: Sequence[str]) -> ProbeRange:
        if len(fields) != 3:
            raise ValueError(f"invalid probe row: {' '.join(fields)}")
        return cls(
            property=IdentifierProperty.parse(fields[0]),
            codepoints=CodePointRange(int(fields[1], 16), int(fields[2], 16)),
        )


@dataclasses.dataclass(frozen=True)
class IdentifierSnapshot:
    start: CharacterSet
    part: CharacterSet
    ignorable: CharacterSet

    def __post_init__(self) -> None:
        if self.start.difference(self.part).ranges:
            raise ValueError("identifier starts must also be identifier parts")
        if self.ignorable.difference(self.part).ranges:
            raise ValueError("ignorable characters must also be identifier parts")

    def characters(self, property: IdentifierProperty) -> CharacterSet:
        if property is IdentifierProperty.START:
            return self.start
        if property is IdentifierProperty.PART:
            return self.part
        return self.ignorable

    def apply(self, transition: ReleaseTransition) -> IdentifierSnapshot:
        return IdentifierSnapshot(
            start=self.start.apply(transition.start),
            part=self.part.apply(transition.part),
            ignorable=self.ignorable.apply(transition.ignorable),
        )


@dataclasses.dataclass(frozen=True)
class ReleaseSnapshot:
    release: int
    identifiers: IdentifierSnapshot

    def __post_init__(self) -> None:
        if self.release < BASE_RELEASE or self.release > LATEST_RELEASE:
            raise ValueError(
                f"Java release must be between {BASE_RELEASE} and {LATEST_RELEASE}"
            )

    @classmethod
    def parse(cls, output: str) -> ReleaseSnapshot:
        lines = tuple(line.split("\t") for line in output.splitlines())
        if not lines or len(lines[0]) != 2 or lines[0][0] != "release":
            raise ValueError("probe did not report its Java release")
        release = int(lines[0][1].removeprefix("1."))
        ranges = tuple(map(ProbeRange.parse, lines[1:]))
        missing = tuple(
            property
            for property in IdentifierProperty
            if not any(probe_range.property is property for probe_range in ranges)
        )
        if missing:
            spellings = ", ".join(property.spelling for property in missing)
            raise ValueError(f"probe did not report identifier properties: {spellings}")

        def characters(property: IdentifierProperty) -> CharacterSet:
            return CharacterSet(
                tuple(
                    probe_range.codepoints
                    for probe_range in ranges
                    if probe_range.property is property
                )
            )

        return cls(
            release=release,
            identifiers=IdentifierSnapshot(
                start=characters(IdentifierProperty.START),
                part=characters(IdentifierProperty.PART),
                ignorable=characters(IdentifierProperty.IGNORABLE),
            ),
        )


@dataclasses.dataclass(frozen=True)
class ReleaseTransition:
    release: int
    start: CharacterChange
    part: CharacterChange
    ignorable: CharacterChange

    def __post_init__(self) -> None:
        if self.release <= BASE_RELEASE or self.release > LATEST_RELEASE:
            raise ValueError(
                f"upgrade release must be between {BASE_RELEASE + 1} and {LATEST_RELEASE}"
            )

    @classmethod
    def between(
        cls,
        release: int,
        previous: IdentifierSnapshot,
        current: IdentifierSnapshot,
    ) -> ReleaseTransition:
        return cls(
            release=release,
            start=CharacterChange.between(previous.start, current.start),
            part=CharacterChange.between(previous.part, current.part),
            ignorable=CharacterChange.between(previous.ignorable, current.ignorable),
        )

    def change(self, property: IdentifierProperty) -> CharacterChange:
        if property is IdentifierProperty.START:
            return self.start
        if property is IdentifierProperty.PART:
            return self.part
        return self.ignorable

    def changed(self) -> bool:
        return any(self.change(property).changed() for property in IdentifierProperty)


@dataclasses.dataclass(frozen=True)
class IdentifierTables:
    base: IdentifierSnapshot
    transitions: tuple[ReleaseTransition, ...]

    def __post_init__(self) -> None:
        releases = tuple(transition.release for transition in self.transitions)
        if releases != tuple(sorted(set(releases))):
            raise ValueError("upgrade releases must be unique and ordered")

    @classmethod
    def derive(cls, snapshots: Sequence[ReleaseSnapshot]) -> IdentifierTables:
        releases = tuple(snapshot.release for snapshot in snapshots)
        expected = tuple(range(BASE_RELEASE, LATEST_RELEASE + 1))
        if releases != expected:
            supplied = ", ".join(map(str, releases))
            raise ValueError(
                f"generation requires Java releases {BASE_RELEASE} through "
                f"{LATEST_RELEASE}; supplied {supplied}"
            )
        transitions = tuple(
            transition
            for previous, current in zip(snapshots, snapshots[1:], strict=False)
            if (
                transition := ReleaseTransition.between(
                    current.release,
                    previous.identifiers,
                    current.identifiers,
                )
            ).changed()
        )
        return cls(base=snapshots[0].identifiers, transitions=transitions)

    @classmethod
    def parse(cls, source: str) -> IdentifierTables:
        bounds_match = re.search(
            r"pub const MIN_RELEASE: u8 = (\d+);\s+"
            r"pub const MAX_RELEASE: u8 = (\d+);",
            source,
        )
        if bounds_match is None:
            raise ValueError("missing supported release bounds")
        bounds = tuple(map(int, bounds_match.groups()))
        if bounds != (BASE_RELEASE, LATEST_RELEASE):
            raise ValueError(
                f"table supports Java {bounds[0]} through {bounds[1]}, expected "
                f"{BASE_RELEASE} through {LATEST_RELEASE}"
            )
        releases_match = re.search(
            r"pub const UPGRADE_RELEASES: \[u8; \d+\] = \[(.*?)\];",
            source,
            re.S,
        )
        if releases_match is None:
            raise ValueError("missing UPGRADE_RELEASES")
        releases = tuple(
            int(value) for value in re.findall(r"\d+", releases_match.group(1))
        )
        base = IdentifierSnapshot(
            start=CharacterSet.parse(
                source, f"{IdentifierProperty.START.constant}_BASE"
            ),
            part=CharacterSet.parse(source, f"{IdentifierProperty.PART.constant}_BASE"),
            ignorable=CharacterSet.parse(
                source, f"{IdentifierProperty.IGNORABLE.constant}_BASE"
            ),
        )
        transitions = tuple(
            ReleaseTransition(
                release=release,
                start=CharacterChange.parse(source, IdentifierProperty.START, index),
                part=CharacterChange.parse(source, IdentifierProperty.PART, index),
                ignorable=CharacterChange.parse(
                    source, IdentifierProperty.IGNORABLE, index
                ),
            )
            for index, release in enumerate(releases, start=1)
        )
        return cls(base=base, transitions=transitions)

    def snapshot(self, release: int) -> IdentifierSnapshot:
        if release < BASE_RELEASE or release > LATEST_RELEASE:
            raise ValueError(
                f"Java release must be between {BASE_RELEASE} and {LATEST_RELEASE}"
            )
        applicable = (
            transition
            for transition in self.transitions
            if transition.release <= release
        )
        return reduce(
            lambda snapshot, transition: snapshot.apply(transition),
            applicable,
            self.base,
        )

    def validation_failures(self, release_snapshot: ReleaseSnapshot) -> tuple[str, ...]:
        expected = self.snapshot(release_snapshot.release)
        properties = differing_categories(expected, release_snapshot.identifiers)
        return tuple(
            f"Java {release_snapshot.release} "
            f"{describe_difference(property, expected.characters(property), release_snapshot.identifiers.characters(property))}"
            for property in properties
        )

    def render(self) -> str:
        releases = ", ".join(str(transition.release) for transition in self.transitions)
        sections = [
            "use super::{CharacterChanges, CharacterSet, VersionedCharacterSet};",
            f"pub const MIN_RELEASE: u8 = {BASE_RELEASE};\n"
            f"pub const MAX_RELEASE: u8 = {LATEST_RELEASE};\n"
            f"pub const UPGRADE_RELEASES: [u8; {len(self.transitions)}] = [{releases}];",
        ]
        sections.extend(
            self.render_property(property) for property in IdentifierProperty
        )
        return "\n\n".join(sections) + "\n"

    def write(self, path: pathlib.Path = TABLE_PATH) -> None:
        temporary_path: pathlib.Path | None = None
        try:
            with tempfile.NamedTemporaryFile(
                mode="w",
                encoding="utf-8",
                newline="\n",
                dir=path.parent,
                prefix=f".{path.name}.",
                delete=False,
            ) as temporary:
                temporary.write(self.render())
                temporary_path = pathlib.Path(temporary.name)
            os.replace(temporary_path, path)
            temporary_path = None
        finally:
            if temporary_path is not None:
                temporary_path.unlink(missing_ok=True)

    def render_property(self, property: IdentifierProperty) -> str:
        constant = property.constant
        sections = [self.base.characters(property).render(f"{constant}_BASE")]
        sections.extend(
            rendered
            for index, transition in enumerate(self.transitions, start=1)
            for rendered in (
                transition.change(property).additions.render(
                    f"{constant}_ADDITIONS_{index}"
                ),
                transition.change(property).removals.render(
                    f"{constant}_REMOVALS_{index}"
                ),
            )
        )
        sections.append(self.render_changes(constant))
        sections.append(
            "\n".join(
                (
                    f"pub const {constant}: VersionedCharacterSet = VersionedCharacterSet {{",
                    f"    base: {constant}_BASE,",
                    f"    changes: &{constant}_CHANGES,",
                    "};",
                )
            )
        )
        return "\n\n".join(sections)

    def render_changes(self, constant: str) -> str:
        entries = "\n".join(
            "\n".join(
                (
                    "    CharacterChanges {",
                    f"        additions: {constant}_ADDITIONS_{index},",
                    f"        removals: {constant}_REMOVALS_{index},",
                    "    },",
                )
            )
            for index in range(1, len(self.transitions) + 1)
        )
        return "\n".join(
            (
                f"const {constant}_CHANGES: [CharacterChanges; {len(self.transitions)}] = [",
                entries,
                "];",
            )
        )


@dataclasses.dataclass(frozen=True)
class JdkHome:
    release: int
    path: pathlib.Path

    @classmethod
    def parse(cls, value: str) -> JdkHome:
        release_text, separator, path_text = value.partition("=")
        if not separator or not release_text.isdecimal() or not path_text:
            raise argparse.ArgumentTypeError("JDK must use RELEASE=/path/to/home")
        return cls(release=int(release_text), path=pathlib.Path(path_text).expanduser())

    def probe(self) -> ReleaseSnapshot:
        javac = self.executable("javac")
        java = self.executable("java")
        with tempfile.TemporaryDirectory(
            prefix=f"boltffi-java-{self.release}-"
        ) as directory:
            subprocess.run(
                (str(javac), "-d", directory, str(PROBE_PATH)),
                check=True,
                env=stable_environment(),
            )
            process = subprocess.run(
                (str(java), "-cp", directory, "IdentifierProbe"),
                check=True,
                capture_output=True,
                text=True,
                env=stable_environment(),
            )
        snapshot = ReleaseSnapshot.parse(process.stdout)
        if snapshot.release != self.release:
            raise ValueError(
                f"{self.path} reports Java {snapshot.release}, expected {self.release}"
            )
        return snapshot

    def executable(self, name: str) -> pathlib.Path:
        candidates = tuple(
            self.path / "bin" / spelling for spelling in (name, f"{name}.exe")
        )
        executable = next((path for path in candidates if path.is_file()), None)
        if executable is None:
            expected = ", ".join(str(path) for path in candidates)
            raise ValueError(f"missing JDK executable; expected one of {expected}")
        return executable


def stable_environment() -> dict[str, str]:
    return {**os.environ, "LANG": "C", "LC_ALL": "C"}


def unique_jdks(values: Sequence[JdkHome]) -> tuple[JdkHome, ...]:
    ordered = tuple(sorted(values, key=lambda jdk: jdk.release))
    releases = tuple(jdk.release for jdk in ordered)
    if len(releases) != len(set(releases)):
        raise ValueError("each Java release must have exactly one JDK home")
    return ordered


def probe_jdks(jdks: Sequence[JdkHome]) -> tuple[ReleaseSnapshot, ...]:
    return tuple(jdk.probe() for jdk in unique_jdks(jdks))


def differing_categories(
    expected: IdentifierSnapshot, actual: IdentifierSnapshot
) -> tuple[IdentifierProperty, ...]:
    return tuple(
        property
        for property in IdentifierProperty
        if expected.characters(property) != actual.characters(property)
    )


def describe_difference(
    property: IdentifierProperty, expected: CharacterSet, actual: CharacterSet
) -> str:
    missing = expected.difference(actual).ranges
    unexpected = actual.difference(expected).ranges
    missing_text = ", ".join(format_range(value) for value in missing[:5]) or "none"
    unexpected_text = (
        ", ".join(format_range(value) for value in unexpected[:5]) or "none"
    )
    return f"{property.spelling}: missing {missing_text}; unexpected {unexpected_text}"


def format_range(codepoints: CodePointRange) -> str:
    return f"U+{codepoints.start:04X}..U+{codepoints.end:04X}"


def validate(jdks: Sequence[JdkHome]) -> int:
    source = TABLE_PATH.read_text()
    tables = IdentifierTables.parse(source)
    snapshots = probe_jdks(jdks)
    failures = tuple(
        chain.from_iterable(
            tables.validation_failures(release_snapshot)
            for release_snapshot in snapshots
        )
    )
    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    releases = ", ".join(str(snapshot.release) for snapshot in snapshots)
    print(f"validated Java identifier tables against releases {releases}")
    return 0


def generate(jdks: Sequence[JdkHome], check: bool, write: bool) -> int:
    snapshots = probe_jdks(jdks)
    tables = IdentifierTables.derive(snapshots)
    generated = tables.render()
    if write:
        tables.write()
        print(f"updated {TABLE_PATH}")
        return 0
    if check:
        current = TABLE_PATH.read_text()
        if current == generated:
            print(f"reproduced {TABLE_PATH}")
            return 0
        difference = difflib.unified_diff(
            current.splitlines(),
            generated.splitlines(),
            fromfile=str(TABLE_PATH),
            tofile="generated",
            lineterm="",
        )
        print("\n".join(difference), file=sys.stderr)
        return 1
    print(generated, end="")
    return 0


def parser() -> argparse.ArgumentParser:
    command = argparse.ArgumentParser(
        description="Generate and validate Java identifier Unicode tables"
    )
    subcommands = command.add_subparsers(dest="command", required=True)
    check = subcommands.add_parser(
        "check", description="Exhaustively validate selected releases"
    )
    add_jdks(check)
    generation = subcommands.add_parser(
        "generate", description="Derive transitions from Java 8 through Java 26"
    )
    add_jdks(generation)
    destination = generation.add_mutually_exclusive_group()
    destination.add_argument(
        "--check", action="store_true", help="compare generated output with tables.rs"
    )
    destination.add_argument(
        "--write", action="store_true", help="replace generated tables.rs"
    )
    return command


def add_jdks(command: argparse.ArgumentParser) -> None:
    command.add_argument(
        "--jdk",
        action="append",
        required=True,
        type=JdkHome.parse,
        metavar="RELEASE=HOME",
        help="JDK release and JAVA_HOME; repeat for every release being queried",
    )


def main(arguments: Sequence[str] | None = None) -> int:
    options = parser().parse_args(arguments)
    try:
        if options.command == "check":
            return validate(options.jdk)
        return generate(options.jdk, options.check, options.write)
    except (OSError, subprocess.CalledProcessError, ValueError) as error:
        print(error, file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
