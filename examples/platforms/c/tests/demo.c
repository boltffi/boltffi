/*
 * C platform smoke test for the experimental C backend.
 *
 * Covers package-prefixed free functions, owned class handles, typed String
 * results, and fallible class initializers.
 */
#include <stdio.h>

#include "demo.h"

static int failures = 0;

#define CHECK(cond)                                                           \
    do {                                                                      \
        if (!(cond)) {                                                        \
            (void)fprintf(stderr, "FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond); \
            failures++;                                                       \
        }                                                                     \
    } while (0)

int main(void) {
    CHECK(DEMO_ANSWER == 42);
    CHECK(demo_add(2, 3) == 5);

    {
        DemoAccumulator accumulator = demo_accumulator_new();
        CHECK(accumulator.handle != 0);
        CHECK(demo_accumulator_add(&accumulator, 5).code == 0);
        CHECK(demo_accumulator_get(&accumulator) == 5);
        demo_accumulator_free(&accumulator);
        CHECK(accumulator.handle == 0);
    }

    {
        DemoSafeDivideResult quotient = demo_safe_divide(6, 2);
        CHECK(quotient.ok);
        CHECK(quotient.data.value == 3);

        DemoSafeDivideResult division_by_zero = demo_safe_divide(1, 0);
        CHECK(!division_by_zero.ok);
        CHECK(division_by_zero.data.error.len != 0);
        boltffi_free_string(division_by_zero.data.error);
    }

    {
        DemoInventoryTryNewResult invalid = demo_inventory_try_new(0);
        CHECK(!invalid.ok);
        CHECK(invalid.data.error.len != 0);
        boltffi_free_string(invalid.data.error);

        DemoInventoryTryNewResult valid = demo_inventory_try_new(4);
        CHECK(valid.ok);
        CHECK(valid.data.value.handle != 0);
        demo_inventory_free(&valid.data.value);
    }

    if (failures == 0) {
        (void)printf("C platform tests passed.\n");
        return 0;
    }
    (void)fprintf(stderr, "%d failures\n", failures);
    return 1;
}
