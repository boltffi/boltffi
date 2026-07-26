from tests.support import DemoTestCase

import demo


class StringsTests(DemoTestCase):
    def test_echo_string(self) -> None:
        self.assertEqual(demo.echo_string("hello"), "hello")
        self.assertEqual(demo.echo_string(""), "", "case:primitives.strings.string.should_roundtrip_empty")
        self.assertEqual(demo.echo_string("café"), "café")
        self.assertEqual(demo.echo_string("日本語"), "日本語")
        self.assertEqual(
            demo.echo_string("hello 🌍 world"),
            "hello 🌍 world",
            "case:primitives.strings.string.should_roundtrip_emoji",
        )

    def test_concat_strings(self) -> None:
        self.assertEqual(demo.concat_strings("foo", "bar"), "foobar", "case:primitives.strings.string.should_concatenate_values")
        self.assertEqual(demo.concat_strings("", "bar"), "bar")
        self.assertEqual(demo.concat_strings("foo", ""), "foo")
        self.assertEqual(demo.concat_strings("🎉", "🎊"), "🎉🎊")

    def test_string_length(self) -> None:
        self.assertEqual(demo.string_length("hello"), 5)
        self.assertEqual(demo.string_length(""), 0)
        self.assertEqual(demo.string_length("café"), 5, "case:primitives.strings.string.should_report_utf8_byte_length")
        self.assertEqual(demo.string_length("🌍"), 4)

    def test_string_is_empty(self) -> None:
        self.assertIs(demo.string_is_empty(""), True, "case:primitives.strings.string.should_detect_empty")

    def test_repeat_string(self) -> None:
        self.assertEqual(demo.repeat_string("ab", 3), "ababab", "case:primitives.strings.string.should_repeat_value")

    def test_borrowed_static_string(self) -> None:
        self.assertEqual(
            demo.borrowed_static_string(),
            "borrowed static",
            "case:primitives.strings.borrowed_static_string.should_return_value",
        )
