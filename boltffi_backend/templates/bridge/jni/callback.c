typedef struct {
    void (*free)(uint64_t handle);
    uint64_t (*clone)(uint64_t handle);
} BoltFFICallbackVTablePrefix;

static const BoltFFICallbackVTablePrefix *boltffi_jni_callback_vtable_prefix(const BoltFFICallbackHandle *callback) {
    return callback == NULL ? NULL : (const BoltFFICallbackVTablePrefix *)callback->vtable;
}

static void boltffi_jni_release_callback_value(BoltFFICallbackHandle callback) {
    const BoltFFICallbackVTablePrefix *vtable = boltffi_jni_callback_vtable_prefix(&callback);
    if (callback.handle != 0 && vtable != NULL && vtable->free != NULL) {
        vtable->free(callback.handle);
    }
}

static jlong boltffi_jni_callback_handle_new_owned(JNIEnv *env, BoltFFICallbackHandle callback) {
    if (callback.handle == 0 || callback.vtable == NULL) {
        return 0;
    }
    BoltFFICallbackHandle *stored_callback = (BoltFFICallbackHandle *)malloc(sizeof(BoltFFICallbackHandle));
    if (stored_callback == NULL) {
        boltffi_jni_release_callback_value(callback);
        boltffi_jni_throw_runtime(env, "failed to allocate BoltFFI callback handle");
        return 0;
    }
    *stored_callback = callback;
    return (jlong)(uintptr_t)stored_callback;
}

static BoltFFICallbackHandle *boltffi_jni_callback_handle_ref(jlong handle) {
    return handle == 0 ? NULL : (BoltFFICallbackHandle *)(uintptr_t)handle;
}

typedef BoltFFICallbackHandle (*BoltFFICallbackCreate)(uint64_t handle);

static BoltFFICallbackHandle boltffi_jni_callback_parameter(uint64_t handle, BoltFFICallbackCreate create) {
    if ((handle & 1u) != 0u) {
        return create(handle);
    }
    const BoltFFICallbackHandle *stored_callback = boltffi_jni_callback_handle_ref((jlong)handle);
    const BoltFFICallbackVTablePrefix *vtable = boltffi_jni_callback_vtable_prefix(stored_callback);
    if (stored_callback == NULL || stored_callback->handle == 0 || vtable == NULL || vtable->clone == NULL) {
        return (BoltFFICallbackHandle){0};
    }
    return (BoltFFICallbackHandle){
        .handle = vtable->clone(stored_callback->handle),
        .vtable = stored_callback->vtable,
    };
}

static void boltffi_jni_callback_handle_release(BoltFFICallbackHandle *callback) {
    if (callback == NULL) {
        return;
    }
    boltffi_jni_release_callback_value(*callback);
    free(callback);
}

static jlong boltffi_jni_callback_handle_clone(JNIEnv *env, const BoltFFICallbackHandle *callback) {
    const BoltFFICallbackVTablePrefix *vtable = boltffi_jni_callback_vtable_prefix(callback);
    if (callback == NULL || callback->handle == 0 || vtable == NULL || vtable->clone == NULL) {
        return 0;
    }
    BoltFFICallbackHandle cloned_callback = {
        .handle = vtable->clone(callback->handle),
        .vtable = callback->vtable,
    };
    if (cloned_callback.handle == 0) {
        return 0;
    }
    return boltffi_jni_callback_handle_new_owned(env, cloned_callback);
}

JNIEXPORT jlong JNICALL {{ callback_clone_symbol }}(JNIEnv *env, jclass cls, jlong handle) {
    (void)cls;
    return boltffi_jni_callback_handle_clone(env, boltffi_jni_callback_handle_ref(handle));
}

JNIEXPORT void JNICALL {{ callback_release_symbol }}(JNIEnv *env, jclass cls, jlong handle) {
    (void)env;
    (void)cls;
    boltffi_jni_callback_handle_release(boltffi_jni_callback_handle_ref(handle));
}
