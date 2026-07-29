#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdatomic.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    int32_t code;
} FfiStatus;

#define FFI_STATUS_OK ((FfiStatus){0})
#define FFI_STATUS_NULL_POINTER ((FfiStatus){1})
#define FFI_STATUS_BUFFER_TOO_SMALL ((FfiStatus){2})
#define FFI_STATUS_INVALID_ARG ((FfiStatus){3})
#define FFI_STATUS_CANCELLED ((FfiStatus){4})
#define FFI_STATUS_INTERNAL_ERROR ((FfiStatus){100})

typedef struct {
    uint8_t *ptr;
    uintptr_t len;
    uintptr_t cap;
    uintptr_t align;
} FfiBuf_u8;

typedef struct {
    uint8_t *ptr;
    uintptr_t len;
    uintptr_t cap;
} FfiString;

typedef struct {
    FfiString message;
} FfiError;

typedef struct {
    const uint8_t *ptr;
    uintptr_t len;
} FfiSpan;

typedef const void *RustFutureHandle;
typedef int8_t StreamPollResult;
typedef int32_t WaitResult;
typedef void (*RustFutureContinuationCallback)(uint64_t callback_data, int8_t poll_result);
typedef void (*StreamContinuationCallback)(uint64_t callback_data, StreamPollResult result);

static inline bool boltffi_atomic_u8_cas(uint8_t *state, uint8_t expected, uint8_t desired) {
    return atomic_compare_exchange_strong_explicit((_Atomic uint8_t *)state, &expected, desired, memory_order_acq_rel, memory_order_acquire);
}

static inline uint64_t boltffi_atomic_u64_exchange(uint64_t *slot, uint64_t value) {
    return atomic_exchange_explicit((_Atomic uint64_t *)slot, value, memory_order_acq_rel);
}

static inline bool boltffi_atomic_u64_cas(uint64_t *slot, uint64_t expected, uint64_t desired) {
    return atomic_compare_exchange_strong_explicit((_Atomic uint64_t *)slot, &expected, desired, memory_order_acq_rel, memory_order_acquire);
}

static inline uint64_t boltffi_atomic_u64_load(uint64_t *slot) {
    return atomic_load_explicit((_Atomic uint64_t *)slot, memory_order_acquire);
}

typedef struct {
    uint64_t handle;
    const void *vtable;
} BoltFFICallbackHandle;
{% for function in support_functions %}
{{ function.declaration }};
{%- endfor %}

{%- for record in direct_records %}
typedef struct {
{%- if record.fields.is_empty() %}
    uint8_t _unused;
{%- else %}
{%- for field in record.fields %}
    {{ field.declaration }};
{%- endfor %}
{%- endif %}
} {{ record.name }};

{%- endfor %}
{%- for c_enum in enums %}
typedef {{ c_enum.repr }} {{ c_enum.name }};
{%- for variant in c_enum.variants %}
#define {{ variant.name }} (({{ variant.ty }}){{ variant.value }})
{%- endfor %}

{%- endfor %}
{%- for vtable in callback_vtables %}
typedef struct {
{%- if vtable.fields.is_empty() %}
    uint8_t _unused;
{%- else %}
{%- for field in vtable.fields %}
    {{ field.declaration }};
{%- endfor %}
{%- endif %}
} {{ vtable.name }};

{%- endfor %}
{%- for function in callback_functions %}
{{ function.declaration }};
{%- endfor %}
{%- if callback_functions.len() > 0 %}

{%- endif %}
{%- for function in functions %}
{{ function.declaration }};
{%- endfor %}

#ifdef __cplusplus
}
#endif
