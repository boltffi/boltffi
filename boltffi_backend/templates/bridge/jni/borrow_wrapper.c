JNIEXPORT jbyteArray JNICALL {{ wrapper.symbol }}(JNIEnv *env, jclass cls, jlong handle) {
    (void)cls;
    const uint8_t *ptr = NULL;
    uintptr_t len = 0;
    int32_t ok = {{ wrapper.c_function }}((const void *)(uintptr_t)handle, &ptr, &len);
    if (!ok) {
        boltffi_jni_throw_runtime(env, "native opaque borrow failed");
        return NULL;
    }
    if (ptr == NULL) {
        if (len != 0) {
            boltffi_jni_throw_runtime(env, "BoltFFI borrow pointer was null with non-zero length");
            return NULL;
        }
        return (*env)->NewByteArray(env, 0);
    }
    if (len > (uintptr_t)INT32_MAX) {
        boltffi_jni_throw_runtime(env, "BoltFFI borrow slice too large for Java byte array");
        return NULL;
    }
    jbyteArray result = (*env)->NewByteArray(env, (jsize)len);
    if (!result) { return NULL; }
    (*env)->SetByteArrayRegion(env, result, 0, (jsize)len, (const jbyte *)ptr);
    if ((*env)->ExceptionCheck(env)) {
        (*env)->DeleteLocalRef(env, result);
        return NULL;
    }
    return result;
}
