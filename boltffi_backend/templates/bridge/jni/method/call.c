{%- if method.checks_completion_status %}
{%- if method.returns_void %}
    (void)env;
    {{ method.c_function }}({{ method.arguments }});
{%- else %}
    {{ method.c_result_type }} __boltffi_result = {{ method.c_function }}({{ method.arguments }});
{%- endif %}
{% include "bridge/jni/method/cleanup_arrays.c" %}
{%- if method.checks_error_buffer %}
    if (__boltffi_result.ptr != NULL || __boltffi_result.len != 0) {
        boltffi_jni_throw_error_buffer(env, __boltffi_result);
{%- include "bridge/jni/method/error_return_nested.c" %}
    }
{%- endif %}
    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
{%- include "bridge/jni/method/error_return_nested.c" %}
    }
{% include "bridge/jni/method/writebacks.c" %}
{%- if method.returns_void %}
    return;
{%- else if method.returns_bytes %}
{%- if method.success_out.is_some() %}
    return boltffi_jni_buffer_to_byte_array(env, {{ method.return_value }});
{%- else %}
    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
{%- endif %}
{%- else if method.returns_record %}
{%- if method.success_out.is_some() %}
    return boltffi_jni_record_to_byte_array(env, &{{ method.return_value }}, (uintptr_t)sizeof({{ method.return_value }}));
{%- else %}
    return boltffi_jni_record_to_byte_array(env, &__boltffi_result, (uintptr_t)sizeof(__boltffi_result));
{%- endif %}
{%- else if method.checks_error_buffer && method.success_out.is_none() %}
    return;
{%- else %}
    return {{ method.return_value }};
{%- endif %}
{%- else if method.returns_void %}
    (void)env;
    {{ method.c_function }}({{ method.arguments }});
{% include "bridge/jni/method/cleanup_arrays.c" %}
{% include "bridge/jni/method/writebacks.c" %}
    return;
{%- else if method.checks_status %}
    {{ method.c_result_type }} __boltffi_status = {{ method.c_function }}({{ method.arguments }});
{% include "bridge/jni/method/cleanup_arrays.c" %}
    if (__boltffi_status.code != 0) {
        boltffi_jni_throw_status(env, __boltffi_status);
{%- include "bridge/jni/method/error_return_nested.c" %}
    }
{% include "bridge/jni/method/writebacks.c" %}
{%- if method.returns_bytes %}
    return boltffi_jni_buffer_to_byte_array(env, {{ method.return_value }});
{%- else if method.returns_record %}
    return boltffi_jni_record_to_byte_array(env, &{{ method.return_value }}, (uintptr_t)sizeof({{ method.return_value }}));
{%- else if method.success_out.is_some() %}
    return {{ method.return_value }};
{%- else %}
    return;
{%- endif %}
{%- else if method.checks_error_buffer %}
    {{ method.c_result_type }} error = {{ method.c_function }}({{ method.arguments }});
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
{%- else if method.success_out.is_none() %}
    return;
{%- else %}
    return {{ method.return_value }};
{%- endif %}
{%- else %}
    (void)env;
    {{ method.c_result_type }} __boltffi_result = {{ method.c_function }}({{ method.arguments }});
{% include "bridge/jni/method/cleanup_arrays.c" %}
{% include "bridge/jni/method/writebacks.c" %}
{%- if method.returns_bytes %}
    return boltffi_jni_buffer_to_byte_array(env, __boltffi_result);
{%- else if method.returns_record %}
    return boltffi_jni_record_to_byte_array(env, &__boltffi_result, (uintptr_t)sizeof(__boltffi_result));
{%- else %}
    return {{ method.return_value }};
{%- endif %}
{%- endif %}
