{%- match method.completion %}
{%- when Some with (completion) %}
{% include "bridge/jni/callback/handle_method/completion.c" %}
{%- when None %}
{%- endmatch %}

JNIEXPORT {{ method.return_type }} JNICALL {{ method.symbol }}(JNIEnv *env, jclass cls, jlong callback{% for parameter in method.parameters %}, {{ parameter.ty }} {{ parameter.name }}{% endfor %}
{%- match method.completion %}
{%- when Some with (completion) %}, jlong {{ completion.context }}
{%- when None %}
{%- endmatch %}) {
    (void)cls;
{% include "bridge/jni/method/locals.c" %}
    BoltFFICallbackHandle *callback_handle = boltffi_jni_callback_handle_ref(callback);
    const {{ method.vtable_type }} *vtable = callback_handle == NULL ? NULL : (const {{ method.vtable_type }} *)callback_handle->vtable;
    if (callback_handle == NULL || callback_handle->handle == 0 || vtable == NULL || vtable->{{ method.slot }} == NULL) {
        boltffi_jni_throw_runtime(env, "BoltFFI callback handle was null or invalid");
        goto __boltffi_error;
    }
{% include "bridge/jni/method/borrowed_arrays.c" %}
{% include "bridge/jni/method/direct_buffers.c" %}
{% include "bridge/jni/method/records.c" %}
{%- if method.returns_closure %}
{%- match method.closure_return %}
{%- when Some with (closure_return) %}
    typedef struct {
        {{ closure_return.invoke_field }};
        void *context;
        void (*release)(void *);
    } {{ closure_return.storage }};
    {{ closure_return.storage }} {{ closure_return.local }} = {0};
    FfiStatus status = vtable->{{ method.slot }}({{ method.arguments }});
{% include "bridge/jni/method/cleanup_arrays.c" %}
    if (status.code != 0) {
        boltffi_jni_throw_status(env, status);
        return 0;
    }
{% include "bridge/jni/method/writebacks.c" %}
    return {{ method.return_value }};
{%- when None %}
{%- endmatch %}
{%- else if method.returns_void %}
    vtable->{{ method.slot }}({{ method.arguments }});
{% include "bridge/jni/method/cleanup_arrays.c" %}
{% include "bridge/jni/method/writebacks.c" %}
    return;
{%- else if method.checks_status %}
    {{ method.c_result_type }} status = vtable->{{ method.slot }}({{ method.arguments }});
{% include "bridge/jni/method/cleanup_arrays.c" %}
    if (status.code != 0) {
        boltffi_jni_throw_status(env, status);
{%- include "bridge/jni/method/error_return_nested.c" %}
    }
{% include "bridge/jni/method/writebacks.c" %}
{%- if method.success_out.is_some() %}
{%- if method.returns_bytes %}
    return boltffi_jni_buffer_to_byte_array(env, {{ method.return_value }});
{%- else if method.returns_record %}
    return boltffi_jni_record_to_byte_array(env, &{{ method.return_value }}, (uintptr_t)sizeof({{ method.return_value }}));
{%- else %}
    return {{ method.return_value }};
{%- endif %}
{%- else %}
    return;
{%- endif %}
{%- else if method.checks_error_buffer %}
    {{ method.c_result_type }} error = vtable->{{ method.slot }}({{ method.arguments }});
{% include "bridge/jni/method/cleanup_arrays.c" %}
    if (error.ptr != NULL || error.len != 0) {
        boltffi_jni_throw_error_buffer(env, error);
{%- include "bridge/jni/method/error_return_nested.c" %}
    }
{% include "bridge/jni/method/writebacks.c" %}
{%- if method.returns_bytes %}
    return boltffi_jni_buffer_to_byte_array(env, {{ method.return_value }});
{%- else if method.returns_record %}
    return boltffi_jni_record_to_byte_array(env, &{{ method.return_value }}, (uintptr_t)sizeof({{ method.return_value }}));
{%- else %}
    return {{ method.return_value }};
{%- endif %}
{%- else %}
    {{ method.c_result_type }} result = vtable->{{ method.slot }}({{ method.arguments }});
{% include "bridge/jni/method/cleanup_arrays.c" %}
{% include "bridge/jni/method/writebacks.c" %}
{%- if method.returns_bytes %}
    return boltffi_jni_buffer_to_byte_array(env, result);
{%- else if method.returns_record %}
    return boltffi_jni_record_to_byte_array(env, &result, (uintptr_t)sizeof(result));
{%- else %}
    return {{ method.return_value }};
{%- endif %}
{%- endif %}
__boltffi_error:
{%- include "bridge/jni/method/cleanup_arrays.c" %}
{%- include "bridge/jni/method/error_return.c" %}
}
