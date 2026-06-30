# frozen_string_literal: true

require_relative "../support"
require "demo"

# Opaque classes are Rust heap objects wrapped in Ruby TypedData. We exercise
# constructors (default + named/singleton), instance methods, static methods,
# and handle-returning chains. Fallible-error and composite-value paths are a
# tracked follow-up and intentionally excluded here.
class OpaqueClassTest < DemoTestCase
  def test_counter_chain
    demo_case "case:classes.counter.should_increment_and_add"
    counter = Demo::Counter.new(10)
    counter.increment
    counter.add(5)
    assert_equal 16, counter.get
  end

  def test_counter_reset
    demo_case "case:classes.counter.should_reset_to_zero"
    counter = Demo::Counter.new(5)
    counter.reset
    assert_equal 0, counter.get
  end

  def test_math_utils_static_add
    demo_case "case:classes.math_utils.should_add_via_static_method"
    assert_equal 5, Demo::MathUtils.add(2, 3)
  end

  def test_math_utils_static_clamp
    demo_case "case:classes.math_utils.should_clamp_via_static_method"
    assert_in_delta 3.0, Demo::MathUtils.clamp(5.0, 0.0, 3.0), 1e-9
  end

  def test_math_utils_instance_round
    demo_case "case:classes.math_utils.should_round_via_instance_method"
    assert_in_delta 3.14, Demo::MathUtils.new(2).round(3.14159), 1e-9
  end

  def test_accumulator
    demo_case "case:classes.accumulator.should_accumulate"
    acc = Demo::Accumulator.new
    acc.add(5)
    acc.add(7)
    assert_equal 12, acc.get
  end

  def test_shared_counter_increment_returns_value
    demo_case "case:classes.shared_counter.should_return_incremented_value"
    assert_equal 1, Demo::SharedCounter.new(0).increment
  end

  def test_shared_counter_add_returns_value
    demo_case "case:classes.shared_counter.should_return_added_value"
    assert_equal 15, Demo::SharedCounter.new(10).add(5)
  end

  def test_shared_counter_set_get
    demo_case "case:classes.shared_counter.should_set_then_get"
    shared = Demo::SharedCounter.new(0)
    shared.set(42)
    assert_equal 42, shared.get
  end

  def test_inventory_add_string
    demo_case "case:classes.inventory.should_add_item"
    assert_equal true, Demo::Inventory.new.add("widget")
  end

  def test_inventory_count
    demo_case "case:classes.inventory.should_count_items"
    inventory = Demo::Inventory.new
    inventory.add("a")
    inventory.add("b")
    assert_equal 2, inventory.count
  end

  def test_inventory_default_capacity
    demo_case "case:classes.inventory.should_report_default_capacity"
    assert_equal 100, Demo::Inventory.new.capacity
  end

  def test_inventory_with_capacity
    demo_case "case:classes.inventory.should_construct_with_named_capacity"
    assert_equal 5, Demo::Inventory.with_capacity(5).capacity
  end

  def test_inventory_try_new_valid
    demo_case "case:classes.inventory.should_construct_via_fallible_with_valid_capacity"
    assert_equal 5, Demo::Inventory.try_new(5).capacity
  end

  def test_data_store_with_sample_data
    demo_case "case:classes.data_store.should_seed_sample_data"
    assert_equal 3, Demo::DataStore.with_sample_data.len
  end

  def test_data_store_add_parts_and_sum
    demo_case "case:classes.data_store.should_add_parts_and_sum"
    store = Demo::DataStore.new
    store.add_parts(1.0, 2.0, 100)
    assert_in_delta 3.0, store.sum, 1e-9
  end

  def test_data_store_is_empty
    demo_case "case:classes.data_store.should_report_empty"
    assert_equal true, Demo::DataStore.new.is_empty
  end

  def test_async_worker_get_prefix
    demo_case "case:classes.async_worker.should_return_prefix"
    assert_equal "job", Demo::AsyncWorker.new("job").get_prefix
  end

  def test_constructor_coverage_matrix_default
    demo_case "case:classes.constructor_coverage_matrix.should_construct_default"
    assert_equal "new", Demo::ConstructorCoverageMatrix.new.constructor_variant
  end

  def test_constructor_coverage_matrix_scalar_mix
    demo_case "case:classes.constructor_coverage_matrix.should_construct_with_scalar_mix"
    matrix = Demo::ConstructorCoverageMatrix.with_scalar_mix(3, true, Demo::Priority::HIGH)
    assert_equal "with_scalar_mix", matrix.constructor_variant
  end

  def test_constructor_coverage_matrix_string_and_bytes
    demo_case "case:classes.constructor_coverage_matrix.should_construct_with_string_and_bytes"
    matrix = Demo::ConstructorCoverageMatrix.with_string_and_bytes("label", packed(1, 2, 3))
    assert_kind_of Integer, matrix.payload_checksum
  end
end
