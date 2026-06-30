# frozen_string_literal: true

require_relative "../support"
require "demo"

class StringsTest < DemoTestCase
  def test_echo_string
    demo_case "case:primitives.strings.string.should_roundtrip_utf8"
    assert_equal "café", Demo.echo_string("café")
  end

  def test_echo_string_encoding
    demo_case "case:primitives.strings.string.should_return_utf8_encoding"
    assert_equal Encoding::UTF_8, Demo.echo_string("café").encoding
  end

  def test_string_length_counts_bytes
    # Rust `str::len` is a byte count, so "café" (é is 2 bytes in UTF-8) is 5.
    demo_case "case:primitives.strings.string.should_report_byte_length"
    assert_equal 5, Demo.string_length("café")
  end

  def test_concat_strings
    demo_case "case:primitives.strings.string.should_concatenate"
    assert_equal "foobar", Demo.concat_strings("foo", "bar")
  end

  def test_repeat_string
    demo_case "case:primitives.strings.string.should_repeat"
    assert_equal "ababab", Demo.repeat_string("ab", 3)
  end

  def test_string_is_empty
    demo_case "case:primitives.strings.string.should_detect_empty"
    assert_equal true, Demo.string_is_empty("")
    assert_equal false, Demo.string_is_empty("x")
  end

  def test_generate_string
    demo_case "case:primitives.strings.string.should_generate_string"
    result = Demo.generate_string(3)
    assert_kind_of String, result
    assert_equal Encoding::UTF_8, result.encoding
  end
end
