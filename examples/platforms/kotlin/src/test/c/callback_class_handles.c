#include <jni.h>
#include <pthread.h>
#include <stdlib.h>
#include "demo.h"

typedef struct {
    BoltFFICallbackHandle callback;
    uint64_t first;
    uint64_t second;
    bool failed;
} CallbackDelivery;

static void *deliver_malformed_pair(void *argument) {
    CallbackDelivery *delivery = argument;
    const uint8_t truncated_label[] = {4, 0, 0, 0, 'p'};
    int32_t success = 0;
    const ___FallibleMessageReceiverVTable *vtable = delivery->callback.vtable;
    FfiBuf_u8 error = vtable->pair(
        delivery->callback.handle, &success, delivery->first,
        truncated_label, sizeof(truncated_label), delivery->second
    );
    vtable->free(delivery->callback.handle);
    delivery->failed = error.len != 0;
    boltffi_free_buf(error);
    return NULL;
}

JNIEXPORT jboolean JNICALL
Java_com_boltffi_demo_CallbackClassHandleFaults_deliverMalformedPair(
    JNIEnv *environment, jclass declaring_class, jlong identity, jlong first, jlong second
) {
    (void)environment;
    (void)declaring_class;
    CallbackDelivery delivery = {
        .callback = boltffi_create_callback_demo_callbacks_class_handles_fallible_message_receiver((uint64_t)identity),
        .first = (uint64_t)first,
        .second = (uint64_t)second,
        .failed = false
    };
    pthread_t worker;
    if (pthread_create(&worker, NULL, deliver_malformed_pair, &delivery) != 0) {
        const ___FallibleMessageReceiverVTable *vtable = delivery.callback.vtable;
        vtable->free(delivery.callback.handle);
        boltffi_release_class_demo_classes_ownership_owned_message(delivery.first);
        boltffi_release_class_demo_classes_ownership_owned_message(delivery.second);
        return JNI_FALSE;
    }
    if (pthread_join(worker, NULL) != 0) abort();
    return delivery.failed ? JNI_TRUE : JNI_FALSE;
}
