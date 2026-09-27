#include <stdio.h>

#include "demo.h"

int main(void) {
    DemoString message = demo_echo_string(demo_string_view("Hello from C", 12));
    fwrite(message.ptr, 1, message.len, stdout);
    putchar('\n');
    demo_string_free(&message);

    DemoPoint point = {3.0, 4.0};
    printf("distance: %g\n", demo_point_distance(&point));

    DemoCounter counter = demo_counter_new(0);
    demo_counter_increment(&counter);
    printf("count: %d\n", demo_counter_get(&counter));
    demo_counter_free(&counter);

    DemoSafeDivideResult quotient = demo_safe_divide(12, 3);
    if (quotient.ok) {
        printf("quotient: %d\n", quotient.data.value);
    } else {
        fwrite(quotient.data.error.ptr, 1, quotient.data.error.len, stderr);
        fputc('\n', stderr);
    }
    int status = quotient.ok ? 0 : 1;
    demo_safe_divide_result_free(&quotient);
    return status;
}
