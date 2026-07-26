static bool {{ closure.load }}(JNIEnv *env) {
    if (!boltffi_jni_lookup_global_class_with_diagnostic(env, {{ closure.class.lookup() }}, {{ closure.class.diagnostic() }}, &{{ closure.global_class }})) {
        return false;
    }
    if (!boltffi_jni_lookup_static_method_with_diagnostic(env, {{ closure.global_class }}, {{ closure.class.diagnostic() }}, "call", "call", {{ closure.method_signature.lookup() }}, {{ closure.method_signature.diagnostic() }}, &{{ closure.call_method }})) {
        goto fail;
    }
    if (!boltffi_jni_lookup_static_method_with_diagnostic(env, {{ closure.global_class }}, {{ closure.class.diagnostic() }}, "free", "free", "(J)V", "(J)V", &{{ closure.free_method }})) {
        goto fail;
    }
    return true;
fail:
    (*env)->DeleteGlobalRef(env, {{ closure.global_class }});
    {{ closure.global_class }} = NULL;
    return false;
}
