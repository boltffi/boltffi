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
