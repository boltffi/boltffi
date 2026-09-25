#ifndef BOLTFFI_C_DEMO_TEST_H
#define BOLTFFI_C_DEMO_TEST_H

#include <stdbool.h>
#include <stdio.h>
#include <string.h>

#include "demo.h"

#define CHECK(condition, scenario)                                            \
    do {                                                                      \
        if (!(condition)) {                                                   \
            fprintf(stderr, "FAIL %s:%d: %s: %s\n", __FILE__, __LINE__,        \
                    scenario, #condition);                                    \
            return false;                                                     \
        }                                                                     \
    } while (0)

#define CHECK_STRING(expression, expected, scenario)                          \
    do {                                                                      \
        DemoString returned_text = (expression);                             \
        const char *expected_text = (expected);                              \
        const size_t expected_length = strlen(expected_text);                \
        const bool matches = returned_text.len == expected_length &&         \
            (expected_length == 0 || memcmp(returned_text.ptr,                \
                                            expected_text, expected_length) == 0); \
        demo_string_free(&returned_text);                                    \
        CHECK(matches, scenario);                                            \
    } while (0)

bool test_scalars(void);
bool test_results(void);
bool test_class_handles(void);
bool test_options(void);
bool test_strings(void);
bool test_bytes(void);
bool test_records(void);
bool test_record_vectors(void);

bool test_vectors(void);
bool test_nested_vectors(void);

bool test_nested_options(void);

bool test_builtins(void);
bool test_maps(void);

bool test_custom_types(void);

bool test_shapes(void);
bool test_messages(void);

bool test_animals(void);
bool test_filters(void);

bool test_owned_records(void);
bool test_record_collections(void);

bool test_constants(void);

bool test_callback_ownership(void);
bool test_callback_class_handles(void);

bool test_scalar_enums(void);

bool test_result_values(void);
bool test_error_values(void);

bool test_nested_records(void);
bool test_service_configs(void);
bool test_borrowed_constructors(void);
bool test_optional_record_vectors(void);
bool test_mutable_values(void);

#endif
