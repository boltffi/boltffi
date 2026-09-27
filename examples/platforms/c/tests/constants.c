#include <stdint.h>
#include <string.h>

#include "test.h"

bool test_constants(void) {
    CHECK(DEMO_ENABLED && DEMO_ANSWER == 42 && DEMO_LARGE == INT64_C(9007199254740993) && DEMO_HALF == 0.5 && strcmp(DEMO_LABEL, "boltffi") == 0, "inline scalar constants");
    DemoBytes bytes = demo_bytes();
    CHECK(bytes.len == 3 && memcmp(bytes.ptr, "ffi", 3) == 0, "accessor byte constant");
    demo_bytes_free(&bytes);
    CHECK(DEMO_MODE == DEMO_MODE_FAST, "inline enum constant");
    DemoState idle = DEMO_IDLE;
    CHECK(idle.tag == DEMO_STATE_IDLE, "inline data enum constant");
    CHECK_STRING(demo_alias(), "boltffi", "accessor string constant");
    CHECK(demo_computed() == 42, "computed scalar constant");
    DemoTupleU32U32 pair = demo_pair();
    CHECK(pair.field_0 == 3 && pair.field_1 == 5, "tuple constant");
    DemoState busy = demo_busy();
    CHECK(busy.tag == DEMO_STATE_BUSY && busy.data.busy.jobs == 3, "case:constants.values.should_expose_inline_and_accessor_values");
    CHECK(DEMO_MODE_PREFERRED == DEMO_MODE_SAFE && demo_mode_fallback() == DEMO_MODE_SAFE && DEMO_MODE_VARIANT_COUNT == 2, "associated enum constants");
    DemoState initial = DEMO_STATE_INITIAL;
    CHECK(initial.tag == DEMO_STATE_IDLE, "associated data enum constant");
    DemoPoint zero = demo_point_zero();
    CHECK(zero.x == 0 && zero.y == 0, "associated record constant");
    CHECK(DEMO_MATH_UTILS_DEFAULT_PRECISION == 2, "case:constants.associated.should_expose_values_on_exported_types");
    return true;
}
