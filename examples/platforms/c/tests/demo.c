#include "test.h"

int main(void) {
    bool (*const tests[])(void) = {
        test_scalars, test_results, test_class_handles, test_options,
        test_strings, test_bytes, test_records, test_record_vectors,
        test_vectors, test_nested_vectors, test_nested_options,
        test_builtins, test_maps, test_custom_types, test_shapes,
        test_messages, test_animals, test_filters, test_owned_records,
        test_record_collections, test_constants, test_callback_ownership, test_scalar_enums,
        test_result_values, test_error_values, test_nested_records,
        test_service_configs, test_borrowed_constructors, test_optional_record_vectors,
        test_mutable_values, test_callback_class_handles
    };
    bool passed = true;
    for (size_t index = 0; index < sizeof(tests) / sizeof(tests[0]); ++index) {
        passed = tests[index]() && passed;
    }
    if (passed) puts("C platform tests passed.");
    return passed ? 0 : 1;
}
