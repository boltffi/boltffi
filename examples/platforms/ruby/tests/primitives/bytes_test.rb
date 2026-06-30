# frozen_string_literal: true

require_relative "../support"
require "demo"

# Bytes cross the boundary as binary (ASCII-8BIT) Strings — built here via the
# `packed` helper (Array#pack("C*")), never as Ruby Arrays of integers.
class BytesTest < DemoTestCase
  def test_echo_bytes
    demo_case "case:bytes.bytes.should_roundtrip_values"
    assert_equal packed(1, 2, 3, 4), Demo.echo_bytes(packed(1, 2, 3, 4))
  end

  def test_echo_bytes_encoding
    demo_case "case:bytes.bytes.should_return_binary_encoding"
    assert_equal Encoding::BINARY, Demo.echo_bytes(packed(1, 2, 3)).encoding
  end

  def test_bytes_length
    demo_case "case:bytes.bytes.should_report_length"
    assert_equal 3, Demo.bytes_length(packed(10, 20, 30))
  end

  def test_bytes_sum
    demo_case "case:bytes.bytes.should_sum_values"
    assert_equal 10, Demo.bytes_sum(packed(1, 2, 3, 4))
  end

  def test_make_bytes
    demo_case "case:bytes.bytes.should_make_sequential_values"
    assert_equal packed(0, 1, 2, 3, 4), Demo.make_bytes(5)
  end

  def test_reverse_bytes
    demo_case "case:bytes.bytes.should_reverse_values"
    assert_equal packed(7, 6, 5), Demo.reverse_bytes(packed(5, 6, 7))
  end

  def test_generate_bytes
    demo_case "case:bytes.bytes.should_generate_length"
    result = Demo.generate_bytes(4)
    assert_kind_of String, result
    assert_equal 4, result.bytesize
  end
end
