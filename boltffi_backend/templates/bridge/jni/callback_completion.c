JNIEXPORT void JNICALL {{ invoker.success }}(JNIEnv *env, jclass cls, jlong callback, jlong context{% if invoker.has_payload %}, {{ invoker.payload_jni_type }} result{% endif %}) {
    (void)cls;
    void (*complete)(void *, FfiStatus{% if invoker.has_payload %}, {{ invoker.payload_c_type }}{% endif %}) = (void (*)(void *, FfiStatus{% if invoker.has_payload %}, {{ invoker.payload_c_type }}{% endif %}))callback;
{%- if invoker.payload_bytes %}
    FfiBuf_u8 payload = boltffi_jni_byte_array_to_buffer(env, result);
    if ((*env)->ExceptionCheck(env)) {
        {{ free_buffer }}(payload);
        complete((void *)context, (FfiStatus){.code = 1}, boltffi_jni_callback_error(env));
        return;
    }
    complete((void *)context, (FfiStatus){.code = 0}, payload);
{%- else if invoker.payload_record %}
    {{ invoker.payload_c_type }} payload = {0};
    if (!boltffi_jni_read_record(env, result, (uintptr_t)sizeof(payload), &payload)) {
        boltffi_jni_clear_exception(env);
        complete((void *)context, (FfiStatus){.code = 1}, ({{ invoker.payload_c_type }}){0});
        return;
    }
    complete((void *)context, (FfiStatus){.code = 0}, payload);
{%- else if invoker.payload_callback_handle %}
    {%- match invoker.payload_create_handle %}
    {%- when Some with (create_handle) %}
    {{ invoker.payload_c_type }} payload = {{ create_handle }}((uint64_t)result);
    complete((void *)context, (FfiStatus){.code = 0}, payload);
    {%- when None %}
    complete((void *)context, (FfiStatus){.code = 1}, ({{ invoker.payload_c_type }}){0});
    {%- endmatch %}
{%- else if invoker.has_payload %}
    (void)env;
    complete((void *)context, (FfiStatus){.code = 0}, ({{ invoker.payload_c_type }})result);
{%- else %}
    (void)env;
    complete((void *)context, (FfiStatus){.code = 0});
{%- endif %}
}

JNIEXPORT void JNICALL {{ invoker.failure }}(JNIEnv *env, jclass cls, jlong callback, jlong context, jthrowable exception) {
    (void)cls;
    void (*complete)(void *, FfiStatus{% if invoker.has_payload %}, {{ invoker.payload_c_type }}{% endif %}) = (void (*)(void *, FfiStatus{% if invoker.has_payload %}, {{ invoker.payload_c_type }}{% endif %}))callback;
{%- if invoker.payload_bytes %}
    complete((void *)context, (FfiStatus){.code = 1}, boltffi_jni_encode_callback_error(env, exception));
{%- else if invoker.has_payload %}
    (void)env;
    (void)exception;
    complete((void *)context, (FfiStatus){.code = 1}, ({{ invoker.payload_c_type }}){0});
{%- else %}
    (void)env;
    (void)exception;
    complete((void *)context, (FfiStatus){.code = 1});
{%- endif %}
}
{%- if invoker.has_payload %}
{%- match invoker.error %}
{%- when Some with (error) %}

JNIEXPORT void JNICALL {{ error }}(JNIEnv *env, jclass cls, jlong callback, jlong context, {{ invoker.payload_jni_type }} result) {
    (void)cls;
    void (*complete)(void *, FfiStatus, {{ invoker.payload_c_type }}) = (void (*)(void *, FfiStatus, {{ invoker.payload_c_type }}))callback;
{%- if invoker.payload_bytes %}
    FfiBuf_u8 payload = boltffi_jni_byte_array_to_buffer(env, result);
    if ((*env)->ExceptionCheck(env)) {
        {{ free_buffer }}(payload);
        complete((void *)context, (FfiStatus){.code = 1}, boltffi_jni_callback_error(env));
        return;
    }
    complete((void *)context, (FfiStatus){.code = 1}, payload);
{%- else if invoker.payload_record %}
    {{ invoker.payload_c_type }} payload = {0};
    if (!boltffi_jni_read_record(env, result, (uintptr_t)sizeof(payload), &payload)) {
        boltffi_jni_clear_exception(env);
        complete((void *)context, (FfiStatus){.code = 1}, ({{ invoker.payload_c_type }}){0});
        return;
    }
    complete((void *)context, (FfiStatus){.code = 1}, payload);
{%- else if invoker.payload_callback_handle %}
    {%- match invoker.payload_create_handle %}
    {%- when Some with (create_handle) %}
    {{ invoker.payload_c_type }} payload = {{ create_handle }}((uint64_t)result);
    complete((void *)context, (FfiStatus){.code = 1}, payload);
    {%- when None %}
    complete((void *)context, (FfiStatus){.code = 1}, ({{ invoker.payload_c_type }}){0});
    {%- endmatch %}
{%- else %}
    (void)env;
    complete((void *)context, (FfiStatus){.code = 1}, ({{ invoker.payload_c_type }})result);
{%- endif %}
}
{%- when None %}
{%- endmatch %}
{%- endif %}
