#pragma once

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
/* C11 atomics with fallback for older GCC/MinGW (e.g. GCC 4.8 on manylinux2014). */
#if defined(__STDC_VERSION__) && __STDC_VERSION__ >= 201112L && !defined(__STDC_NO_ATOMICS__)
#include <stdatomic.h>
#elif defined(__GNUC__) && (__GNUC__ > 4 || (__GNUC__ == 4 && __GNUC_MINOR__ >= 7))
#define _Atomic volatile
typedef volatile int atomic_int;
#define memory_order_acquire __ATOMIC_ACQUIRE
#define memory_order_release __ATOMIC_RELEASE
#define memory_order_acq_rel __ATOMIC_ACQ_REL
#define atomic_load_explicit(ptr, order)                __atomic_load_n(ptr, order)
#define atomic_store_explicit(ptr, val, order)           __atomic_store_n(ptr, val, order)
#define atomic_compare_exchange_strong_explicit(ptr, expected, desired, succ, fail) \
    __atomic_compare_exchange_n(ptr, expected, desired, 0, succ, fail)
#define atomic_exchange_explicit(ptr, val, order)        __atomic_exchange_n(ptr, val, order)
#define atomic_compare_exchange_strong(ptr, expected, desired) \
    __atomic_compare_exchange_n(ptr, expected, desired, 0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST)
#define atomic_exchange(ptr, val) __atomic_exchange_n(ptr, val, __ATOMIC_SEQ_CST)
#else
#error "BoltFFI requires C11 <stdatomic.h> or GCC 4.7+ __atomic builtins"
#endif

typedef struct { int32_t code; } FfiStatus;
typedef struct { uint8_t* ptr; size_t len; size_t cap; } FfiString;
static inline bool {{ prefix }}_atomic_u8_cas(uint8_t* state, uint8_t expected, uint8_t desired) {
    return atomic_compare_exchange_strong_explicit((_Atomic uint8_t*)state, &expected, desired, memory_order_acq_rel, memory_order_acquire);
}
static inline uint64_t {{ prefix }}_atomic_u64_exchange(uint64_t* slot, uint64_t value) {
    return atomic_exchange_explicit((_Atomic uint64_t*)slot, value, memory_order_acq_rel);
}
static inline bool {{ prefix }}_atomic_u64_cas(uint64_t* slot, uint64_t expected, uint64_t desired) {
    return atomic_compare_exchange_strong_explicit((_Atomic uint64_t*)slot, &expected, desired, memory_order_acq_rel, memory_order_acquire);
}
static inline uint64_t {{ prefix }}_atomic_u64_load(uint64_t* slot) {
    return atomic_load_explicit((_Atomic uint64_t*)slot, memory_order_acquire);
}
FfiStatus {{ prefix }}_last_error_message(FfiString* out);
void {{ prefix }}_clear_last_error(void);
{%- for record in records %}

typedef struct {
{%- for field in record.fields %}
    {{ field.c_type }} {{ field.name }};
{%- endfor %}
} {{ record.name }};
{%- endfor %}
{% for func in functions %}
{{ func.signature }};
{%- endfor %}

void {{ prefix }}_free_string(FfiString s);
