#include <math.h>
#include <string.h>

#include "test.h"

bool test_records(void) {
    DemoPoint point = demo_point_new(3.0, 4.0);
    CHECK(point.x == 3.0 && point.y == 4.0, "case:records.blittable.point.should_construct_with_static_new");
    DemoPoint origin = demo_point_origin();
    CHECK(origin.x == 0.0 && origin.y == 0.0, "case:records.blittable.point.should_return_origin");
    DemoPoint polar = demo_point_from_polar(2.0, acos(-1.0) / 2.0);
    CHECK(fabs(polar.x) < 1e-12 && fabs(polar.y - 2.0) < 1e-12,
          "case:records.blittable.point.should_construct_from_polar_coordinates");

    DemoPointTryUnitResult normalized = demo_point_try_unit(3.0, 4.0);
    CHECK(normalized.ok && fabs(normalized.data.value.x - 0.6) < 1e-12 && fabs(normalized.data.value.y - 0.8) < 1e-12,
          "case:records.blittable.point.should_normalize_unit_vector");
    demo_point_try_unit_result_free(&normalized);
    DemoPointTryUnitResult zero = demo_point_try_unit(0.0, 0.0);
    const char zero_message[] = "cannot normalize zero vector";
    CHECK(!zero.ok && zero.data.error.len == sizeof(zero_message) - 1 && memcmp(zero.data.error.ptr, zero_message, sizeof(zero_message) - 1) == 0,
          "case:records.blittable.point.should_reject_zero_unit_vector");
    demo_point_try_unit_result_free(&zero);

    DemoOptionPoint optional = demo_point_checked_unit(3.0, 4.0);
    CHECK(optional.has_value && fabs(optional.value.x - 0.6) < 1e-12 && fabs(optional.value.y - 0.8) < 1e-12,
          "case:records.blittable.point.should_return_some_for_checked_unit");
    CHECK(!demo_point_checked_unit(0.0, 0.0).has_value,
          "case:records.blittable.point.should_return_none_for_zero_checked_unit");
    CHECK(fabs(demo_point_distance(&point) - 5.0) < 1e-12,
          "case:records.blittable.point.should_compute_distance");
    demo_point_scale(&point, 2.0);
    CHECK(point.x == 6.0 && point.y == 8.0, "case:records.blittable.point.should_scale_coordinates");
    DemoPoint added = demo_point_add(&point, (DemoPoint){1.0, 2.0});
    CHECK(added.x == 7.0 && added.y == 10.0, "case:records.blittable.point.should_add_coordinates");
    const DemoPoint path[] = {{0.0, 0.0}, {3.0, 4.0}, {6.0, 8.0}};
    CHECK(fabs(demo_point_path_length((DemoPointSlice){path, 3}) - 10.0) < 1e-12,
          "case:records.blittable.point.should_compute_path_length");
    CHECK(demo_point_dimensions() == 2, "case:records.blittable.point.should_report_dimension_count");
    DemoPoint echoed = demo_echo_point(point);
    CHECK(echoed.x == point.x && echoed.y == point.y, "case:records.blittable.point.should_roundtrip_value");
    optional = demo_try_make_point(3.0, 4.0);
    CHECK(optional.has_value && optional.value.x == 3.0 && optional.value.y == 4.0,
          "case:records.blittable.point.should_return_some_for_nonzero_coordinates");
    CHECK(!demo_try_make_point(0.0, 0.0).has_value,
          "case:records.blittable.point.should_return_none_for_origin_coordinates");
    DemoPoint made = demo_make_point(-2.0, 7.0);
    CHECK(made.x == -2.0 && made.y == 7.0, "case:records.blittable.point.should_make_from_coordinates");
    added = demo_add_points(point, made);
    CHECK(added.x == 4.0 && added.y == 15.0, "case:records.blittable.point.should_add_values");

    const DemoColor color = {17, 128, 255, 64};
    DemoColor echoed_color = demo_echo_color(color);
    CHECK(echoed_color.r == 17 && echoed_color.g == 128 && echoed_color.b == 255 && echoed_color.a == 64,
          "case:records.blittable.color.should_roundtrip_value");
    DemoColor made_color = demo_make_color(0, 127, 254, 255);
    CHECK(made_color.r == 0 && made_color.g == 127 && made_color.b == 254 && made_color.a == 255,
          "case:records.blittable.color.should_make_from_channels");
    return true;
}

bool test_record_vectors(void) {
    DemoLocationSequence locations = demo_generate_locations(3);
    CHECK(locations.len == 3 && locations.ptr[2].id == 2 && locations.ptr[2].review_count == 20 && locations.ptr[2].is_open,
          "case:records.blittable.locations.should_generate_sample_vector");
    CHECK(demo_process_locations((DemoLocationSlice){locations.ptr, locations.len}) == 3,
          "case:records.blittable.locations.should_count_vector_items");
    CHECK(demo_process_locations((DemoLocationSlice){NULL, 0}) == 0,
          "case:records.blittable.locations.should_count_empty_vector");
    const DemoLocation host_locations[] = {
        {7, 1.0, 2.0, 4.0, 10, true},
        {8, 3.0, 4.0, 4.5, 20, false}
    };
    CHECK(demo_process_locations((DemoLocationSlice){host_locations, 2}) == 2,
          "case:records.blittable.locations.should_count_host_constructed_vector");
    CHECK(fabs(demo_sum_ratings((DemoLocationSlice){locations.ptr, locations.len}) - 9.3) < 1e-12,
          "case:records.blittable.locations.should_sum_generated_ratings");
    CHECK(fabs(demo_sum_ratings((DemoLocationSlice){host_locations, 2}) - 8.5) < 1e-12,
          "case:records.blittable.locations.should_sum_host_constructed_ratings");

    DemoTradeSequence trades = demo_generate_trades(3);
    CHECK(trades.len == 3 && trades.ptr[2].quantity == 3 && trades.ptr[2].volume == 2000 && trades.ptr[2].is_buy,
          "case:records.blittable.trades.should_generate_sample_vector");
    CHECK(demo_sum_trade_volumes((DemoTradeSlice){trades.ptr, trades.len}) == 3000,
          "case:records.blittable.trades.should_sum_volumes");
    CHECK(demo_aggregate_location_trade_stats((DemoLocationSlice){locations.ptr, locations.len}, (DemoTradeSlice){trades.ptr, trades.len}) == 3002,
          "case:records.blittable.trades.should_aggregate_with_locations");
    demo_trade_sequence_free(&trades);
    demo_location_sequence_free(&locations);

    DemoParticleSequence particles = demo_generate_particles(3);
    CHECK(particles.len == 3 && particles.ptr[2].id == 2 && fabs(particles.ptr[2].mass - 1.002) < 1e-12 && particles.ptr[2].active,
          "case:records.blittable.particles.should_generate_sample_vector");
    CHECK(fabs(demo_sum_particle_masses((DemoParticleSlice){particles.ptr, particles.len}) - 3.003) < 1e-12,
          "case:records.blittable.particles.should_sum_masses");
    demo_particle_sequence_free(&particles);

    DemoSensorReadingSequence readings = demo_generate_sensor_readings(3);
    CHECK(readings.len == 3 && readings.ptr[2].sensor_id == 2 && readings.ptr[2].temperature == 22.0 && readings.ptr[2].is_valid,
          "case:records.blittable.sensor_readings.should_generate_sample_vector");
    CHECK(fabs(demo_avg_sensor_temperature((DemoSensorReadingSlice){readings.ptr, readings.len}) - 21.0) < 1e-12,
          "case:records.blittable.sensor_readings.should_average_generated_temperatures");
    CHECK(demo_avg_sensor_temperature((DemoSensorReadingSlice){NULL, 0}) == 0.0,
          "case:records.blittable.sensor_readings.should_average_empty_vector_as_zero");
    demo_sensor_reading_sequence_free(&readings);

    DemoOptionLocation found = demo_find_location(7);
    CHECK(found.has_value && found.value.id == 7 && found.value.rating == 4.5 && found.value.is_open,
          "case:records.blittable.locations.find_location.should_return_some_for_positive_id");
    CHECK(!demo_find_location(0).has_value && !demo_find_location(-1).has_value,
          "case:records.blittable.locations.find_location.should_return_none_for_non_positive_id");
    return true;
}

bool test_optional_record_vectors(void) {
    DemoOptionOfLocationSequence locations = demo_find_locations(2);
    CHECK(locations.has_value && locations.value.len == 2 && locations.value.ptr[0].id == 0 && locations.value.ptr[1].id == 1, "case:records.blittable.locations.find_locations.should_return_some_vector_for_positive_count");
    demo_option_of_location_sequence_free(&locations);
    locations = demo_find_locations(0);
    CHECK(!locations.has_value, "case:records.blittable.locations.find_locations.should_return_none_for_non_positive_count");
    demo_option_of_location_sequence_free(&locations);
    return true;
}
