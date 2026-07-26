JNIEXPORT jint JNICALL JNI_OnLoad(JavaVM *vm, void *reserved) {
    (void)reserved;
    JNIEnv *env = NULL;
    jint env_result = (*vm)->GetEnv(vm, (void **)&env, JNI_VERSION_1_6);
    if (env_result != JNI_OK) {
        fprintf(stderr, "BoltFFI JNI_OnLoad failed: GetEnv(JNI_VERSION_1_6) returned %d\n", (int)env_result);
        return JNI_ERR;
    }
    if (!boltffi_jni_lookup_global_class_with_diagnostic(env, {{ class_name.lookup() }}, {{ class_name.diagnostic() }}, &boltffi_jni_native_class)) {
        return JNI_ERR;
    }
{%- if uses_continuations %}
    if (!boltffi_jni_continuation_load(env)) {
        (*env)->DeleteGlobalRef(env, boltffi_jni_native_class);
        boltffi_jni_native_class = NULL;
        return JNI_ERR;
    }
{%- endif %}
{%- for callback in callbacks %}
    if (!{{ callback.load }}(env)) {
{%- for cleanup in callbacks %}
        {{ cleanup.unload }}(env);
{%- endfor %}
{%- for cleanup in closures %}
        {{ cleanup.unload }}(env);
{%- endfor %}
{%- if uses_continuations %}
        boltffi_jni_continuation_unload(env);
{%- endif %}
        (*env)->DeleteGlobalRef(env, boltffi_jni_native_class);
        boltffi_jni_native_class = NULL;
        return JNI_ERR;
    }
{%- endfor %}
{%- for closure in closures %}
    if (!{{ closure.load }}(env)) {
{%- for cleanup in callbacks %}
        {{ cleanup.unload }}(env);
{%- endfor %}
{%- for cleanup in closures %}
        {{ cleanup.unload }}(env);
{%- endfor %}
{%- if uses_continuations %}
        boltffi_jni_continuation_unload(env);
{%- endif %}
        (*env)->DeleteGlobalRef(env, boltffi_jni_native_class);
        boltffi_jni_native_class = NULL;
        return JNI_ERR;
    }
{%- endfor %}
    boltffi_jni_vm = vm;
    return JNI_VERSION_1_6;
}
