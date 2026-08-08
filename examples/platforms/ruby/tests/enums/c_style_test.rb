# frozen_string_literal: true

require_relative "../support"
require "demo"

# C-style enums lower to plain Integer constants under a module (no instance
# methods, no `.new`) — the Ruby surface is intentionally simpler than Python's.
class CStyleEnumTest < DemoTestCase
  def test_status_constants
    demo_case "case:enums.c_style.status.should_expose_discriminants"
    assert_equal 0, Demo::Status::ACTIVE
    assert_equal 1, Demo::Status::INACTIVE
    assert_equal 2, Demo::Status::PENDING
  end

  def test_direction_constants
    demo_case "case:enums.c_style.direction.should_expose_discriminants"
    assert_equal 0, Demo::Direction::NORTH
    assert_equal 1, Demo::Direction::SOUTH
    assert_equal 2, Demo::Direction::EAST
    assert_equal 3, Demo::Direction::WEST
  end

  def test_priority_constants
    demo_case "case:enums.c_style.priority.should_expose_discriminants"
    assert_equal 0, Demo::Priority::LOW
    assert_equal 1, Demo::Priority::MEDIUM
    assert_equal 2, Demo::Priority::HIGH
    assert_equal 3, Demo::Priority::CRITICAL
  end

  def test_log_level_constants
    demo_case "case:enums.c_style.log_level.should_expose_discriminants"
    assert_equal 0, Demo::LogLevel::TRACE
    assert_equal 1, Demo::LogLevel::DEBUG
    assert_equal 2, Demo::LogLevel::INFO
    assert_equal 3, Demo::LogLevel::WARN
    assert_equal 4, Demo::LogLevel::ERROR
  end

  def test_http_code_explicit_discriminants
    demo_case "case:enums.c_style.http_code.should_expose_explicit_discriminants"
    assert_equal 200, Demo::HttpCode::OK
    assert_equal 404, Demo::HttpCode::NOT_FOUND
    assert_equal 500, Demo::HttpCode::SERVER_ERROR
  end

  def test_sign_negative_discriminants
    demo_case "case:enums.c_style.sign.should_expose_negative_discriminants"
    assert_equal(-1, Demo::Sign::NEGATIVE)
    assert_equal 0, Demo::Sign::ZERO
    assert_equal 1, Demo::Sign::POSITIVE
  end

  def test_echo_status
    demo_case "case:enums.c_style.status.should_roundtrip"
    assert_equal Demo::Status::ACTIVE, Demo.echo_status(Demo::Status::ACTIVE)
  end

  def test_status_to_string
    demo_case "case:enums.c_style.status.should_render_label"
    assert_equal "active", Demo.status_to_string(Demo::Status::ACTIVE)
  end

  def test_is_active
    demo_case "case:enums.c_style.status.should_classify_active"
    assert_equal true, Demo.is_active(Demo::Status::ACTIVE)
    assert_equal false, Demo.is_active(Demo::Status::PENDING)
  end

  def test_echo_direction
    demo_case "case:enums.c_style.direction.should_roundtrip"
    assert_equal Demo::Direction::EAST, Demo.echo_direction(Demo::Direction::EAST)
  end

  def test_opposite_direction
    demo_case "case:enums.c_style.direction.should_compute_opposite"
    assert_equal Demo::Direction::WEST, Demo.opposite_direction(Demo::Direction::EAST)
  end

  def test_direction_to_degrees
    demo_case "case:enums.c_style.direction.should_map_to_degrees"
    assert_equal 270, Demo.direction_to_degrees(Demo::Direction::WEST)
  end

  def test_echo_priority
    demo_case "case:enums.c_style.priority.should_roundtrip"
    assert_equal Demo::Priority::HIGH, Demo.echo_priority(Demo::Priority::HIGH)
  end

  def test_priority_label
    demo_case "case:enums.c_style.priority.should_render_label"
    assert_equal "low", Demo.priority_label(Demo::Priority::LOW)
  end

  def test_is_high_priority
    demo_case "case:enums.c_style.priority.should_classify_high"
    assert_equal true, Demo.is_high_priority(Demo::Priority::CRITICAL)
    assert_equal false, Demo.is_high_priority(Demo::Priority::LOW)
  end

  def test_echo_log_level
    demo_case "case:enums.c_style.log_level.should_roundtrip"
    assert_equal Demo::LogLevel::WARN, Demo.echo_log_level(Demo::LogLevel::WARN)
  end

  def test_should_log
    demo_case "case:enums.c_style.log_level.should_gate_on_threshold"
    assert_equal true, Demo.should_log(Demo::LogLevel::ERROR, Demo::LogLevel::WARN)
  end

  def test_echo_http_code
    demo_case "case:enums.c_style.http_code.should_roundtrip"
    assert_equal 404, Demo.echo_http_code(Demo::HttpCode::NOT_FOUND)
  end

  def test_echo_sign
    demo_case "case:enums.c_style.sign.should_roundtrip_negative"
    assert_equal(-1, Demo.echo_sign(Demo::Sign::NEGATIVE))
  end
end
