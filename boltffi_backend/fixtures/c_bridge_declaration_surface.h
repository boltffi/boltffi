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

void boltffi_free_string(FfiString string);
void boltffi_free_buf(FfiBuf_u8 buf);
FfiBuf_u8 boltffi_buf_from_bytes(const uint8_t *ptr, uintptr_t len);
FfiStatus boltffi_last_error_message(FfiString *out);
void boltffi_clear_last_error(void);
typedef struct {
    double x;
    double y;
} ___Point;
typedef uint8_t ___Mode;
#define MODE_FAST ((___Mode)1)
#define MODE_SLOW ((___Mode)2)
typedef uint32_t ___Shape;
#define SHAPE_DOT ((___Shape)0)
#define SHAPE_LABEL ((___Shape)1)
typedef struct {
    void (*free)(uint64_t);
    uint64_t (*clone)(uint64_t);
    void (*notify)(uint64_t, uint32_t);
    int64_t (*on_value)(uint64_t, uint32_t);
    void (*load)(uint64_t, uint32_t, void (*)(void *, FfiStatus, FfiBuf_u8), void *);
    void (*locate)(uint64_t, ___Point, void (*)(void *, FfiStatus, ___Point), void *);
    void (*maybe_value)(uint64_t, uint32_t, void (*)(void *, FfiStatus, FfiBuf_u8), void *);
    void (*values)(uint64_t, uint32_t, void (*)(void *, FfiStatus, FfiBuf_u8), void *);
    void (*try_load)(uint64_t, uint32_t, void (*)(void *, FfiStatus, FfiBuf_u8), void *);
} ___ListenerVTable;
void boltffi_register_callback_demo_listener(const ___ListenerVTable *vtable);
BoltFFICallbackHandle boltffi_create_callback_demo_listener(uint64_t handle);
___Point boltffi_init_record_demo_point_origin(void);
double boltffi_method_record_demo_point_distance(___Point receiver, ___Point other);
FfiBuf_u8 boltffi_method_record_demo_person_rename(const uint8_t *receiver_ptr, uintptr_t receiver_len, const uint8_t *name_ptr, uintptr_t name_len);
___Mode boltffi_init_enum_demo_mode_default(void);
uint8_t boltffi_method_enum_demo_mode_code(___Mode receiver);
void boltffi_release_class_demo_engine(uint64_t handle);
uint64_t boltffi_init_class_demo_engine_new(uint64_t seed);
uint32_t boltffi_method_class_demo_engine_version(void);
uint32_t boltffi_method_class_demo_engine_score(uint64_t receiver, ___Point point);
FfiStatus boltffi_method_class_demo_engine_advance(uint64_t receiver, uint32_t delta);
int32_t boltffi_function_demo_add(int32_t left, int32_t right);
RustFutureHandle boltffi_function_demo_fetch_count(void);
void boltffi_async_function_demo_fetch_count_poll(RustFutureHandle handle, uint64_t callback_data, void (*callback)(uint64_t, int8_t));
uint32_t boltffi_async_function_demo_fetch_count_complete(RustFutureHandle handle, FfiStatus *out_status);
FfiBuf_u8 boltffi_async_function_demo_fetch_count_panic_message(RustFutureHandle handle);
void boltffi_async_function_demo_fetch_count_cancel(RustFutureHandle handle);
void boltffi_async_function_demo_fetch_count_free(RustFutureHandle handle);
RustFutureHandle boltffi_function_demo_refresh(void);
void boltffi_async_function_demo_refresh_poll(RustFutureHandle handle, uint64_t callback_data, void (*callback)(uint64_t, int8_t));
void boltffi_async_function_demo_refresh_complete(RustFutureHandle handle, FfiStatus *out_status);
FfiBuf_u8 boltffi_async_function_demo_refresh_panic_message(RustFutureHandle handle);
void boltffi_async_function_demo_refresh_cancel(RustFutureHandle handle);
void boltffi_async_function_demo_refresh_free(RustFutureHandle handle);
FfiBuf_u8 boltffi_function_demo_greet(const uint8_t *name_ptr, uintptr_t name_len);
FfiBuf_u8 boltffi_function_demo_keep_shape(const uint8_t *shape_ptr, uintptr_t shape_len);
FfiBuf_u8 boltffi_function_demo_remember(const uint8_t *time_ptr, uintptr_t time_len);
intptr_t boltffi_function_demo_shift(intptr_t offset);
FfiStatus boltffi_function_demo_install(BoltFFICallbackHandle listener, uint32_t (*callback_call)(void *, uint32_t), void *callback_context, void (*callback_release)(void *));
FfiStatus boltffi_function_demo_install_void(void (*callback_call)(void *, uint32_t), void *callback_context, void (*callback_release)(void *));
uint64_t boltffi_stream_demo_engine_points_subscribe(uint64_t receiver);
uintptr_t boltffi_stream_demo_engine_points_pop_batch(uint64_t subscription, ___Point *output_ptr, uintptr_t output_capacity);
WaitResult boltffi_stream_demo_engine_points_wait(uint64_t subscription, uint32_t timeout_milliseconds);
void boltffi_stream_demo_engine_points_poll(uint64_t subscription, uint64_t callback_data, void (*callback)(uint64_t, StreamPollResult));
void boltffi_stream_demo_engine_points_unsubscribe(uint64_t subscription);
void boltffi_stream_demo_engine_points_free(uint64_t subscription);
uint64_t boltffi_stream_demo_engine_names_subscribe(uint64_t receiver);
FfiBuf_u8 boltffi_stream_demo_engine_names_pop_batch(uint64_t subscription, uintptr_t max_count);
WaitResult boltffi_stream_demo_engine_names_wait(uint64_t subscription, uint32_t timeout_milliseconds);
void boltffi_stream_demo_engine_names_poll(uint64_t subscription, uint64_t callback_data, void (*callback)(uint64_t, StreamPollResult));
void boltffi_stream_demo_engine_names_unsubscribe(uint64_t subscription);
void boltffi_stream_demo_engine_names_free(uint64_t subscription);
FfiBuf_u8 boltffi_const_demo_magic(void);

#ifdef __cplusplus
}
#endif
