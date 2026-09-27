static inline bool boltffi_jni_clear_exception(JNIEnv *env) {
    if (!(*env)->ExceptionCheck(env)) {
        return false;
    }
    (*env)->ExceptionClear(env);
    return true;
}

static void boltffi_jni_describe_load_exception(JNIEnv *env) {
    if ((*env)->ExceptionCheck(env)) {
        (*env)->ExceptionDescribe(env);
        (*env)->ExceptionClear(env);
    }
}

static bool boltffi_jni_report_class_load_failure(JNIEnv *env, const char *message, const char *diagnostic_class_name) {
    fprintf(stderr, "BoltFFI JNI_OnLoad failed: %s '%s'\n", message, diagnostic_class_name);
    boltffi_jni_describe_load_exception(env);
    return false;
}

static bool boltffi_jni_report_static_method_load_failure(JNIEnv *env, const char *diagnostic_class_name, const char *diagnostic_method_name, const char *diagnostic_signature) {
    fprintf(stderr, "BoltFFI JNI_OnLoad failed: could not resolve static method %s.%s%s\n", diagnostic_class_name, diagnostic_method_name, diagnostic_signature);
    boltffi_jni_describe_load_exception(env);
    return false;
}

static bool boltffi_jni_lookup_global_class_with_diagnostic(JNIEnv *env, const char *lookup_class_name, const char *diagnostic_class_name, jclass *out_class) {
    *out_class = NULL;
    jclass local_class = (*env)->FindClass(env, lookup_class_name);
    if (local_class == NULL) {
        return boltffi_jni_report_class_load_failure(env, "could not find JVM class", diagnostic_class_name);
    }
    jclass global_class = (*env)->NewGlobalRef(env, local_class);
    (*env)->DeleteLocalRef(env, local_class);
    if (global_class == NULL) {
        return boltffi_jni_report_class_load_failure(env, "could not create global reference for JVM class", diagnostic_class_name);
    }
    *out_class = global_class;
    return true;
}

static bool boltffi_jni_lookup_static_method_with_diagnostic(JNIEnv *env, jclass cls, const char *diagnostic_class_name, const char *lookup_method_name, const char *diagnostic_method_name, const char *lookup_signature, const char *diagnostic_signature, jmethodID *out_method) {
    *out_method = (*env)->GetStaticMethodID(env, cls, lookup_method_name, lookup_signature);
    if (*out_method == NULL) {
        return boltffi_jni_report_static_method_load_failure(env, diagnostic_class_name, diagnostic_method_name, diagnostic_signature);
    }
    return true;
}

static FfiBuf_u8 boltffi_jni_encode_callback_error(JNIEnv *env, jthrowable exception) {
    if (exception == NULL) {
        return {{ callback_error }}(NULL, 0);
    }
    if ((*env)->PushLocalFrame(env, 8) != JNI_OK) {
        (*env)->ExceptionClear(env);
        return {{ callback_error }}(NULL, 0);
    }
    FfiBuf_u8 error = {0};
    jclass exception_class = (*env)->GetObjectClass(env, exception);
    if (exception_class == NULL) goto done;
    jmethodID to_string = (*env)->GetMethodID(env, exception_class, "toString", "()Ljava/lang/String;");
    if (to_string == NULL) goto done;
    jstring message = (jstring)(*env)->CallObjectMethod(env, exception, to_string);
    if ((*env)->ExceptionCheck(env) || message == NULL) goto done;
    jclass string_class = (*env)->GetObjectClass(env, message);
    if (string_class == NULL) goto done;
    jmethodID get_bytes = (*env)->GetMethodID(env, string_class, "getBytes", "(Ljava/lang/String;)[B");
    if (get_bytes == NULL) goto done;
    jstring charset = (*env)->NewStringUTF(env, "UTF-8");
    if (charset == NULL) goto done;
    jbyteArray message_bytes = (jbyteArray)(*env)->CallObjectMethod(env, message, get_bytes, charset);
    if ((*env)->ExceptionCheck(env) || message_bytes == NULL) goto done;
    jsize length = (*env)->GetArrayLength(env, message_bytes);
    jbyte *bytes = (*env)->GetByteArrayElements(env, message_bytes, NULL);
    if (bytes == NULL) goto done;
    error = {{ callback_error }}((const uint8_t *)bytes, (uintptr_t)length);
    (*env)->ReleaseByteArrayElements(env, message_bytes, bytes, JNI_ABORT);
done:
    boltffi_jni_clear_exception(env);
    (*env)->PopLocalFrame(env, NULL);
    return error.ptr != NULL ? error : {{ callback_error }}(NULL, 0);
}

static FfiBuf_u8 boltffi_jni_callback_error(JNIEnv *env) {
    jthrowable exception = (*env)->ExceptionOccurred(env);
    (*env)->ExceptionClear(env);
    FfiBuf_u8 error = boltffi_jni_encode_callback_error(env, exception);
    (*env)->DeleteLocalRef(env, exception);
    return error;
}
