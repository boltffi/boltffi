import pathlib
import tempfile
import unittest

from boltffi_backend.tools.java_identifiers.generate import (
    BASE_RELEASE,
    LATEST_RELEASE,
    CharacterSet,
    CodePointRange,
    IdentifierSnapshot,
    IdentifierTables,
    JdkHome,
    ReleaseSnapshot,
    TABLE_PATH,
)


class CharacterSetTests(unittest.TestCase):
    def test_rejects_surrogate_code_points(self):
        with self.assertRaisesRegex(ValueError, "surrogate"):
            CodePointRange(0xD7FF, 0xD800)

    def test_difference_splits_and_trims_ranges(self):
        characters = CharacterSet((CodePointRange(1, 10), CodePointRange(20, 30)))
        excluded = CharacterSet(
            (
                CodePointRange(0, 2),
                CodePointRange(5, 7),
                CodePointRange(10, 25),
                CodePointRange(40, 50),
            )
        )

        self.assertEqual(
            characters.difference(excluded),
            CharacterSet(
                (
                    CodePointRange(3, 4),
                    CodePointRange(8, 9),
                    CodePointRange(26, 30),
                )
            ),
        )

    def test_union_merges_adjacent_and_overlapping_ranges(self):
        left = CharacterSet((CodePointRange(1, 3), CodePointRange(10, 12)))
        right = CharacterSet((CodePointRange(4, 6), CodePointRange(11, 20)))

        self.assertEqual(
            left.union(right),
            CharacterSet((CodePointRange(1, 6), CodePointRange(10, 20))),
        )


class IdentifierTableTests(unittest.TestCase):
    def test_rejects_identifier_starts_that_are_not_parts(self):
        with self.assertRaisesRegex(ValueError, "starts"):
            IdentifierSnapshot(
                start=CharacterSet((CodePointRange(2, 2),)),
                part=CharacterSet((CodePointRange(1, 1),)),
                ignorable=CharacterSet.empty(),
            )

    def test_derivation_keeps_only_releases_with_identifier_changes(self):
        java_8 = IdentifierSnapshot(
            start=CharacterSet((CodePointRange(1, 2),)),
            part=CharacterSet((CodePointRange(1, 3),)),
            ignorable=CharacterSet.empty(),
        )
        java_9 = IdentifierSnapshot(
            start=CharacterSet((CodePointRange(1, 3),)),
            part=CharacterSet((CodePointRange(1, 4),)),
            ignorable=CharacterSet((CodePointRange(4, 4),)),
        )
        snapshots = tuple(
            ReleaseSnapshot(
                release=release,
                identifiers=java_8 if release == 8 else java_9,
            )
            for release in range(BASE_RELEASE, LATEST_RELEASE + 1)
        )

        tables = IdentifierTables.derive(snapshots)

        self.assertEqual(
            tuple(transition.release for transition in tables.transitions), (9,)
        )
        self.assertEqual(tables.snapshot(8), java_8)
        self.assertEqual(tables.snapshot(26), java_9)

    def test_checked_in_table_round_trips_through_the_generator(self):
        source = TABLE_PATH.read_text()

        tables = IdentifierTables.parse(source)

        self.assertEqual(tables.render(), source)


class JdkHomeTests(unittest.TestCase):
    def test_resolves_windows_executable_names(self):
        with tempfile.TemporaryDirectory() as directory:
            home = pathlib.Path(directory)
            binary = home / "bin" / "java.exe"
            binary.parent.mkdir()
            binary.touch()

            self.assertEqual(JdkHome(17, home).executable("java"), binary)


if __name__ == "__main__":
    unittest.main()
