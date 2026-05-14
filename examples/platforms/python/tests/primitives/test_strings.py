import unittest

import demo


class StringsTests(unittest.TestCase):
    def test_echo_string(self) -> None:
        self.assertEqual(demo.echo_string("hello"), "hello")
        self.assertEqual(demo.echo_string(""), "", "case:primitives.strings.echo.empty")
        self.assertEqual(demo.echo_string("café"), "café")
        self.assertEqual(demo.echo_string("日本語"), "日本語")
        self.assertEqual(
            demo.echo_string("hello 🌍 world"),
            "hello 🌍 world",
            "case:primitives.strings.echo.emoji",
        )

    def test_concat_strings(self) -> None:
        self.assertEqual(demo.concat_strings("foo", "bar"), "foobar", "case:primitives.strings.concat.basic")
        self.assertEqual(demo.concat_strings("", "bar"), "bar")
        self.assertEqual(demo.concat_strings("foo", ""), "foo")
        self.assertEqual(demo.concat_strings("🎉", "🎊"), "🎉🎊")

    def test_string_length(self) -> None:
        self.assertEqual(demo.string_length("hello"), 5)
        self.assertEqual(demo.string_length(""), 0)
        self.assertEqual(demo.string_length("café"), 5, "case:primitives.strings.length.utf8_bytes")
        self.assertEqual(demo.string_length("🌍"), 4)

    def test_string_is_empty(self) -> None:
        self.assertIs(demo.string_is_empty(""), True, "case:primitives.strings.is_empty.empty")

    def test_repeat_string(self) -> None:
        self.assertEqual(demo.repeat_string("ab", 3), "ababab", "case:primitives.strings.repeat.basic")
