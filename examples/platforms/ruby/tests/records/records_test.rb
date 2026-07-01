# frozen_string_literal: true

require_relative "../support"
require "demo"

class RecordsTest < DemoTestCase
  def test_make_person_returns_encoded_record
    demo_case "case:records.with_strings.person.should_make_from_fields"
    person = Demo.make_person("Ada", 37)

    assert_kind_of Demo::Person, person
    assert_equal "Ada", person.name
    assert_equal 37, person.age
  end

  def test_make_point_returns_direct_record
    demo_case "case:records.blittable.point.should_make_from_coordinates"
    point = Demo.make_point(3.0, 4.0)

    assert_kind_of Demo::Point, point
    assert_in_delta 3.0, point.x, 1e-9
    assert_in_delta 4.0, point.y, 1e-9
  end

  def test_borrowed_direct_record_parameter
    demo_case "case:records.blittable.point.should_compute_distance_from_borrowed_point"
    point = Demo.make_point(3.0, 4.0)

    assert_in_delta 5.0, Demo.distance_from_origin(point), 1e-9
  end

  def test_mutable_borrowed_direct_record_parameter
    demo_case "case:records.blittable.point.should_scale_borrowed_mutable_point"
    point = Demo.make_point(3.0, 4.0)

    Demo.scale_point(point, 2.0)

    assert_in_delta 6.0, point.x, 1e-9
    assert_in_delta 8.0, point.y, 1e-9
  end
end
