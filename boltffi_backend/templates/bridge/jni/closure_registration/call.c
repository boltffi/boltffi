static {{ closure.c_return_type }} {{ closure.call }}(void *user_data{% for parameter in closure.c_parameters %}, {{ parameter.declaration }}{% endfor %}) {
    JNIEnv *env = NULL;
    int attached = 0;
    if (!boltffi_jni_enter(&env, &attached)) {
{%- if closure.returns_void %}
        return;
{%- else %}
        return {{ closure.failure_value }};
{%- endif %}
    }
    jlong handle = (jlong)(uintptr_t)user_data;
{% include "bridge/jni/closure/byte_array_declarations.c" %}
{% include "bridge/jni/closure/closure_handle_declarations.c" %}
{% include "bridge/jni/closure/byte_arrays.c" %}
{% include "bridge/jni/closure/direct_vector_declarations.c" %}
{% include "bridge/jni/closure/record_declarations.c" %}
{% include "bridge/jni/closure/direct_vectors.c" %}
{% include "bridge/jni/closure/records.c" %}
{% include "bridge/jni/closure/closure_handles.c" %}
{%- if closure.returns_void %}
    (*env)->CallStaticVoidMethod(env, {{ closure.global_class }}, {{ closure.call_method }}, handle{% if closure.has_jni_arguments %}, {{ closure.jni_arguments }}{% endif %});
    boltffi_jni_clear_exception(env);
{% include "bridge/jni/closure/cleanup.c" %}
    boltffi_jni_exit(env, attached);
{%- else %}
{%- if closure.returns_byte_array %}
    jbyteArray __boltffi_return_array = (jbyteArray)(*env)->CallStaticObjectMethod(env, {{ closure.global_class }}, {{ closure.call_method }}, handle{% if closure.has_jni_arguments %}, {{ closure.jni_arguments }}{% endif %});
{%- else if closure.returns_callback_handle %}
    jlong __boltffi_return_handle = (*env)->CallStaticLongMethod(env, {{ closure.global_class }}, {{ closure.call_method }}, handle{% if closure.has_jni_arguments %}, {{ closure.jni_arguments }}{% endif %});
{%- else %}
    {{ closure.c_return_type }} result = ({{ closure.c_return_type }})(*env)->CallStatic{{ closure.call_method_suffix }}Method(env, {{ closure.global_class }}, {{ closure.call_method }}, handle{% if closure.has_jni_arguments %}, {{ closure.jni_arguments }}{% endif %});
{%- endif %}
    if (boltffi_jni_clear_exception(env)) {
{% include "bridge/jni/closure/cleanup.c" %}
        boltffi_jni_exit(env, attached);
        return {{ closure.failure_value }};
    }
{%- if closure.returns_bytes %}
    {{ closure.c_return_type }} result = boltffi_jni_byte_array_to_buffer(env, __boltffi_return_array);
    (*env)->DeleteLocalRef(env, __boltffi_return_array);
{%- else if closure.returns_record %}
    {{ closure.c_return_type }} result = {0};
    if (!boltffi_jni_read_record(env, __boltffi_return_array, (uintptr_t)sizeof(result), &result)) {
        (*env)->DeleteLocalRef(env, __boltffi_return_array);
        boltffi_jni_clear_exception(env);
{% include "bridge/jni/closure/cleanup.c" %}
        boltffi_jni_exit(env, attached);
        return {{ closure.failure_value }};
    }
    (*env)->DeleteLocalRef(env, __boltffi_return_array);
{%- else if closure.returns_callback_handle %}
    {%- match closure.callback_handle_constructor %}
    {%- when Some with (create_handle) %}
    {{ closure.c_return_type }} result = {{ create_handle }}((uint64_t)__boltffi_return_handle);
    {%- when None %}
    {{ closure.c_return_type }} result = {{ closure.failure_value }};
    {%- endmatch %}
{%- endif %}
{% include "bridge/jni/closure/cleanup.c" %}
    boltffi_jni_exit(env, attached);
    return result;
{%- endif %}
}
