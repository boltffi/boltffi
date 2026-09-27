#include <string.h>

#include "test.h"

bool test_class_handles(void) {
    DemoAccumulator accumulator = demo_accumulator_new();
    CHECK(demo_accumulator_get(&accumulator) == 0, "new accumulator starts at zero");
    demo_accumulator_add(&accumulator, 5);
    CHECK(demo_accumulator_get(&accumulator) == 5, "accumulator preserves method updates");
    demo_accumulator_free(&accumulator);
    demo_accumulator_free(&accumulator);

    DemoInventoryTryNewResult invalid = demo_inventory_try_new(0);
    CHECK(!invalid.ok, "case:classes.constructors.inventory.try_new.should_reject_zero_capacity");
    const char expected_error[] = "capacity must be greater than zero";
    CHECK(invalid.data.error.len == sizeof(expected_error) - 1,
          "case:classes.constructors.inventory.try_new.should_reject_zero_capacity");
    CHECK(memcmp(invalid.data.error.ptr, expected_error, sizeof(expected_error) - 1) == 0,
          "case:classes.constructors.inventory.try_new.should_reject_zero_capacity");
    demo_inventory_try_new_result_free(&invalid);

    DemoInventoryTryNewResult valid = demo_inventory_try_new(4);
    CHECK(valid.ok, "case:classes.constructors.inventory.try_new.should_return_inventory_for_positive_capacity");
    CHECK(demo_inventory_capacity(&valid.data.value) == 4,
          "case:classes.constructors.inventory.try_new.should_return_inventory_for_positive_capacity");
    CHECK(demo_inventory_count(&valid.data.value) == 0,
          "case:classes.constructors.inventory.try_new.should_return_inventory_for_positive_capacity");
    demo_inventory_try_new_result_free(&valid);

    DemoCounter counter = demo_counter_new(5);
    DemoCounterTryResetIfPositiveResult reset = demo_counter_try_reset_if_positive(&counter);
    CHECK(reset.ok && demo_counter_get(&counter) == 0, "a result with unit success resets the counter");
    demo_counter_try_reset_if_positive_result_free(&reset);
    reset = demo_counter_try_reset_if_positive(&counter);
    const char reset_error[] = "count is not positive";
    CHECK(!reset.ok && reset.data.error.len == sizeof(reset_error) - 1 && memcmp(reset.data.error.ptr, reset_error, sizeof(reset_error) - 1) == 0,
          "a result with unit success preserves its string error");
    demo_counter_try_reset_if_positive_result_free(&reset);
    demo_counter_add(&counter, 7);
    DemoPoint counter_point = demo_counter_as_point(&counter);
    CHECK(counter_point.x == 7.0 && counter_point.y == 0.0, "class methods return direct records by value");
    demo_counter_free(&counter);
    return true;
}

bool test_borrowed_constructors(void) {
    const DemoPoint points[] = {{1, 2}, {3, 4}};
    DemoConstructorCoverageMatrix matrix = demo_constructor_coverage_matrix_with_borrowed_points((DemoStringView){"points", 6}, (DemoPointSlice){points, 2});
    CHECK(demo_constructor_coverage_matrix_vector_count(&matrix) == 2, "borrowed point count");
    CHECK_STRING(demo_constructor_coverage_matrix_summary(&matrix), "label=points;points=2;first=1.0:2.0", "case:classes.constructor_matrix.with_borrowed_points.should_accept_borrowed_blittable_slice");
    demo_constructor_coverage_matrix_free(&matrix);
    const DemoPersonView people[] = {{{"Ali", 3}, 32}, {{"Sam", 3}, 28}};
    matrix = demo_constructor_coverage_matrix_with_borrowed_people((DemoPersonSlice){people, 2});
    CHECK(demo_constructor_coverage_matrix_vector_count(&matrix) == 62, "borrowed people count and age total");
    CHECK_STRING(demo_constructor_coverage_matrix_summary(&matrix), "people=2;age_total=60;names=Ali|Sam", "case:classes.constructor_matrix.with_borrowed_people.should_accept_borrowed_encoded_record_slice");
    demo_constructor_coverage_matrix_free(&matrix);
    DemoMapView map = demo_map_view_new();
    DemoMarker marker = demo_map_view_add_marker(&map, (DemoMarkerOptionsView){42, {"marker", 6}});
    CHECK(demo_marker_id(&marker) == 42 && demo_map_view_marker_count(&map) == 1, "single-threaded marker identity");
    CHECK_STRING(demo_marker_title(&marker), "marker", "case:classes.unsafe_single_threaded.map_view.add_marker.should_return_single_threaded_marker_handle");
    demo_marker_free(&marker);
    demo_map_view_free(&map);
    return true;
}
