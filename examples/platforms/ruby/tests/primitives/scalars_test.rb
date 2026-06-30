# frozen_string_literal: true

require_relative "../support"
require "demo"

class ScalarsTest < DemoTestCase
  def test_echo_bool
    demo_case "case:primitives.scalars.bool.should_roundtrip_true"
    assert_equal true, Demo.echo_bool(true)
  end

  def test_negate_bool
    demo_case "case:primitives.scalars.bool.should_negate_false_to_true"
    assert_equal true, Demo.negate_bool(false)
  end

  def test_echo_i8
    demo_case "case:primitives.scalars.i8.should_roundtrip_negative_value"
    assert_equal(-7, Demo.echo_i8(-7))
  end

  def test_echo_u8
    demo_case "case:primitives.scalars.u8.should_roundtrip_max_value"
    assert_equal 255, Demo.echo_u8(255)
  end

  def test_echo_i16
    demo_case "case:primitives.scalars.i16.should_roundtrip_negative_value"
    assert_equal(-1234, Demo.echo_i16(-1234))
  end

  def test_echo_u16
    demo_case "case:primitives.scalars.u16.should_roundtrip_large_value"
    assert_equal 55_000, Demo.echo_u16(55_000)
  end

  def test_echo_i32
    demo_case "case:primitives.scalars.i32.should_roundtrip_negative_value"
    assert_equal(-42, Demo.echo_i32(-42))
  end

  def test_add_i32
    demo_case "case:primitives.scalars.i32.should_add_two_values"
    assert_equal 30, Demo.add_i32(10, 20)
  end

  def test_echo_u32
    demo_case "case:primitives.scalars.u32.should_roundtrip_large_value"
    assert_equal 4_000_000_000, Demo.echo_u32(4_000_000_000)
  end

  def test_echo_i64
    demo_case "case:primitives.scalars.i64.should_roundtrip_large_negative_value"
    assert_equal(-9_999_999_999, Demo.echo_i64(-9_999_999_999))
  end

  def test_echo_u64
    demo_case "case:primitives.scalars.u64.should_roundtrip_large_value"
    assert_equal 9_999_999_999, Demo.echo_u64(9_999_999_999)
  end

  def test_echo_usize
    demo_case "case:primitives.scalars.usize.should_roundtrip_value"
    assert_equal 123, Demo.echo_usize(123)
  end

  def test_echo_isize
    demo_case "case:primitives.scalars.isize.should_roundtrip_negative_value"
    assert_equal(-123, Demo.echo_isize(-123))
  end

  def test_echo_f32
    demo_case "case:primitives.scalars.f32.should_roundtrip_value_with_tolerance"
    assert_in_delta 3.5, Demo.echo_f32(3.5), 1e-6
  end

  def test_add_f32
    demo_case "case:primitives.scalars.f32.should_add_two_values_with_tolerance"
    assert_in_delta 4.0, Demo.add_f32(1.5, 2.5), 1e-6
  end

  def test_echo_f64
    demo_case "case:primitives.scalars.f64.should_roundtrip_pi_with_tolerance"
    assert_in_delta 3.14159265359, Demo.echo_f64(3.14159265359), 1e-12
  end

  def test_add_f64
    demo_case "case:primitives.scalars.f64.should_add_two_values_with_tolerance"
    assert_in_delta 4.0, Demo.add_f64(1.5, 2.5), 1e-12
  end

  def test_multiply
    demo_case "case:primitives.scalars.f64.should_multiply_two_values_with_tolerance"
    assert_in_delta 12.0, Demo.multiply(3.0, 4.0), 1e-12
  end

  def test_i32_overflow_raises_range_error
    assert_raises(RangeError) do
      Demo.echo_i32(3_000_000_000) # larger than signed i32 max (2_147_483_647)
    end
    assert_raises(RangeError) do
      Demo.echo_i32(-3_000_000_000) # smaller than signed i32 min (-2_147_483_648)
    end
  end

  def test_invalid_type_raises_type_error
    assert_raises(TypeError) do
      Demo.echo_i32("string")
    end
    assert_raises(TypeError) do
      Demo.echo_bool("true")
    end
  end
end
