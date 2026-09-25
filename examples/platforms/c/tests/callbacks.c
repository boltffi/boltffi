#include <stdint.h>

#include "test.h"

static uint32_t callback_releases;

static void value_callback_free(uint64_t identity) {
    if (identity == 42) ++callback_releases;
}

static uint64_t value_callback_clone(uint64_t identity) {
    return identity;
}

static int32_t value_callback_invoke(uint64_t identity, int32_t value) {
    return value + (int32_t)identity;
}

bool test_callback_ownership(void) {
    static const DemoValueCallback vtable = {
        .free = value_callback_free,
        .clone = value_callback_clone,
        .on_value = value_callback_invoke
    };
    DemoValueCallbackHandle callback = demo_value_callback_create(&vtable, 42);
    DemoValueCallbackHandle copy = demo_value_callback_clone(&callback);
    CHECK(demo_invoke_value_callback(&callback, 1) == 43, "foreign callback invocation");
    CHECK(callback.raw.handle == 0 && callback.raw.vtable == NULL && callback_releases == 1, "consumed foreign callback is cleared");
    demo_value_callback_free(&callback);
    CHECK(callback_releases == 1, "cleared callback cannot be freed twice");
    CHECK(demo_invoke_boxed_value_callback(&copy, 2) == 44, "cloned callback survives the original");
    CHECK(copy.raw.handle == 0 && callback_releases == 2, "consumed clone is released once");
    CHECK(demo_invoke_optional_value_callback(NULL, 7) == 7, "absent callback uses the Rust fallback");
    callback = demo_make_incrementing_callback(5);
    copy = demo_value_callback_clone(&callback);
    CHECK(demo_invoke_boxed_value_callback(&copy, 37) == 42, "Rust callback round trip");
    CHECK(copy.raw.handle == 0, "consumed Rust callback is cleared");
    CHECK(demo_invoke_value_callback(&callback, 2) == 7, "Rust callback clone retains the original");
    demo_value_callback_free(&callback);
    callback = demo_make_incrementing_callback(6);
    demo_value_callback_free(&callback);
    CHECK(callback.raw.handle == 0 && callback.raw.vtable == NULL, "returned Rust callback is explicitly releasable");
    return true;
}

typedef struct {
    DemoOwnedMessage first;
    DemoOwnedMessage second;
    bool fail;
    bool valid_label;
    uint32_t received_callback;
    uint32_t callback_releases;
} MessageReceiverState;

static void message_receiver_free(uint64_t identity) {
    MessageReceiverState *receiver = (MessageReceiverState *)(uintptr_t)identity;
    ++receiver->callback_releases;
}

static uint64_t message_receiver_clone(uint64_t identity) {
    return identity;
}

static void message_receiver_attach(uint64_t identity, uint64_t handle, uint32_t callback) {
    MessageReceiverState *receiver = (MessageReceiverState *)(uintptr_t)identity;
    receiver->first._boltffi_handle = handle;
    receiver->received_callback = callback;
}

static void message_receiver_optional(uint64_t identity, uint64_t handle) {
    MessageReceiverState *receiver = (MessageReceiverState *)(uintptr_t)identity;
    receiver->first._boltffi_handle = handle;
}

static FfiBuf_u8 message_receiver_pair(
    uint64_t identity, int32_t *success, uint64_t first,
    const uint8_t *label, uintptr_t label_length, uint64_t second
) {
    MessageReceiverState *receiver = (MessageReceiverState *)(uintptr_t)identity;
    const uint8_t expected_label[] = {4, 0, 0, 0, 'p', 'a', 'i', 'r'};
    receiver->first._boltffi_handle = first;
    receiver->second._boltffi_handle = second;
    receiver->valid_label = label_length == sizeof(expected_label)
        && memcmp(label, expected_label, sizeof(expected_label)) == 0;
    if (receiver->fail) {
        const uint8_t error[] = {1, 0, 0, 0};
        return boltffi_buf_from_bytes(error, sizeof(error));
    }
    *success = (int32_t)demo_owned_message_length(&receiver->first);
    if (second != 0) *success += (int32_t)demo_owned_message_length(&receiver->second);
    return (FfiBuf_u8){0};
}

bool test_callback_class_handles(void) {
    static const DemoMessageReceiver receiver_vtable = {
        .free = message_receiver_free,
        .clone = message_receiver_clone,
        .attach = message_receiver_attach,
        .optional = message_receiver_optional
    };
    static const DemoFallibleMessageReceiver fallible_vtable = {
        .free = message_receiver_free,
        .clone = message_receiver_clone,
        .pair = message_receiver_pair
    };
    MessageReceiverState state = {0};
    DemoMessageDrops drops = demo_message_drops_new();
    DemoMessageReceiverHandle receiver =
        demo_message_receiver_create(&receiver_vtable, (uint64_t)(uintptr_t)&state);
    demo_deliver_message(&receiver, &drops);
    CHECK(demo_message_drops_count(&drops) == 0 && demo_owned_message_length(&state.first) == 9,
          "case:callbacks.class_handles.should_retain_after_return");
    CHECK(state.callback_releases == 1, "message outlives the callback object");
    CHECK(state.received_callback == 42, "callback argument keeps its value");
    demo_owned_message_free(&state.first);
    demo_owned_message_free(&state.first);
    CHECK(demo_message_drops_count(&drops) == 1, "received message drops exactly once");

    DemoFallibleMessageReceiverHandle fallible =
        demo_fallible_message_receiver_create(&fallible_vtable, (uint64_t)(uintptr_t)&state);
    DemoDeliverMessagePairResult pair = demo_deliver_message_pair(&fallible, &drops, true);
    CHECK(pair.ok && pair.data.value == 11 && state.valid_label,
          "case:callbacks.class_handles.should_deliver_multiple_and_optional");
    demo_deliver_message_pair_result_free(&pair);
    CHECK(demo_message_drops_count(&drops) == 1, "retained pair stays alive");
    demo_owned_message_free(&state.first);
    CHECK(demo_message_drops_count(&drops) == 2, "first message drops independently");
    CHECK(demo_owned_message_length(&state.second) == 6, "second message still works");
    demo_owned_message_free(&state.second);

    fallible = demo_fallible_message_receiver_create(&fallible_vtable, (uint64_t)(uintptr_t)&state);
    pair = demo_deliver_message_pair(&fallible, &drops, false);
    CHECK(pair.ok && pair.data.value == 5 && state.second._boltffi_handle == 0, "optional message is absent");
    demo_deliver_message_pair_result_free(&pair);
    demo_owned_message_free(&state.first);
    CHECK(demo_message_drops_count(&drops) == 4, "optional delivery releases once");

    state.fail = true;
    fallible = demo_fallible_message_receiver_create(&fallible_vtable, (uint64_t)(uintptr_t)&state);
    pair = demo_deliver_message_pair(&fallible, &drops, true);
    CHECK(!pair.ok && pair.data.error == DEMO_MATH_ERROR_NEGATIVE_INPUT,
          "case:callbacks.class_handles.should_retain_after_error");
    demo_deliver_message_pair_result_free(&pair);
    CHECK(demo_message_drops_count(&drops) == 4, "callback error preserves delivered ownership");
    CHECK(demo_owned_message_length(&state.first) == 5 && demo_owned_message_length(&state.second) == 6,
          "messages remain usable after callback error");
    demo_owned_message_free(&state.first);
    demo_owned_message_free(&state.second);
    CHECK(demo_message_drops_count(&drops) == 6, "messages drop once after callback error");

    receiver = demo_make_message_receiver();
    const DemoMessageReceiver *rust_receiver = receiver.raw.vtable;
    DemoOwnedMessage moved = demo_owned_message_new((DemoStringView){"moved", 5}, &drops);
    uint64_t handle = moved._boltffi_handle;
    moved._boltffi_handle = 0;
    rust_receiver->attach(receiver.raw.handle, handle, 42);
    demo_owned_message_free(&moved);
    CHECK(demo_message_drops_count(&drops) == 7,
          "case:callbacks.class_handles.should_consume_in_rust_callback");
    moved = demo_owned_message_new((DemoStringView){"optional", 8}, &drops);
    handle = moved._boltffi_handle;
    moved._boltffi_handle = 0;
    rust_receiver->optional(receiver.raw.handle, handle);
    rust_receiver->optional(receiver.raw.handle, 0);
    demo_owned_message_free(&moved);
    CHECK(demo_message_drops_count(&drops) == 8, "Rust callback consumes optional message");
    demo_message_receiver_free(&receiver);
    demo_message_drops_free(&drops);
    return true;
}
