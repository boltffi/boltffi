static bool {{ callback.load }}(JNIEnv *env) {
    if (!boltffi_jni_lookup_global_class_with_diagnostic(env, {{ callback.class.lookup() }}, {{ callback.class.diagnostic() }}, &{{ callback.global_class }})) {
        return false;
    }
    if (!boltffi_jni_lookup_static_method_with_diagnostic(env, {{ callback.global_class }}, {{ callback.class.diagnostic() }}, "free", "free", "(J)V", "(J)V", &{{ callback.free_method }})) {
        goto fail;
    }
    if (!boltffi_jni_lookup_static_method_with_diagnostic(env, {{ callback.global_class }}, {{ callback.class.diagnostic() }}, "clone", "clone", "(J)J", "(J)J", &{{ callback.clone_method }})) {
        goto fail;
    }
{%- for method in callback.methods %}
    if (!boltffi_jni_lookup_static_method_with_diagnostic(env, {{ callback.global_class }}, {{ callback.class.diagnostic() }}, {{ method.method_name.lookup() }}, {{ method.method_name.diagnostic() }}, {{ method.signature.lookup() }}, {{ method.signature.diagnostic() }}, &{{ method.method_id }})) {
        goto fail;
    }
{%- endfor %}
{%- for method in callback.handle_methods %}
{%- match method.completion %}
{%- when Some with (completion) %}
    if (!boltffi_jni_lookup_static_method_with_diagnostic(env, {{ callback.global_class }}, {{ callback.class.diagnostic() }}, {{ completion.success_method.lookup() }}, {{ completion.success_method.diagnostic() }}, {{ completion.success_signature.lookup() }}, {{ completion.success_signature.diagnostic() }}, &{{ completion.success_method_id }})) {
        goto fail;
    }
    if (!boltffi_jni_lookup_static_method_with_diagnostic(env, {{ callback.global_class }}, {{ callback.class.diagnostic() }}, {{ completion.failure_method.lookup() }}, {{ completion.failure_method.diagnostic() }}, {{ completion.failure_signature.lookup() }}, {{ completion.failure_signature.diagnostic() }}, &{{ completion.failure_method_id }})) {
        goto fail;
    }
{%- when None %}
{%- endmatch %}
{%- endfor %}
    {{ callback.register }}(&{{ callback.vtable }});
    return true;
fail:
    (*env)->DeleteGlobalRef(env, {{ callback.global_class }});
    {{ callback.global_class }} = NULL;
    return false;
}
