JNIEXPORT {{ handle.jni_return_type }} JNICALL {{ handle.call_symbol }}(JNIEnv *env, jclass cls, jlong value{% for parameter in handle.closure.handle_parameters %}, {{ parameter.declaration }}{% endfor %}) {
    (void)env;
    (void)cls;
    {{ handle.ty }} *closure = {{ handle.ref_ }}(value);
    if (closure == NULL || closure->call == NULL) {
{%- if handle.closure.returns_void %}
        return;
{%- else %}
        return {{ handle.failure_value }};
{%- endif %}
    }
{% include "bridge/jni/closure/handle_buffer_declarations.c" %}
{% include "bridge/jni/closure/handle_byte_arrays.c" %}
{% include "bridge/jni/closure/handle_vector_declarations.c" %}
{% include "bridge/jni/closure/handle_direct_vectors.c" %}
{% include "bridge/jni/closure/handle_records.c" %}
{%- if handle.closure.returns_void %}
    closure->call(closure->context{% if handle.closure.has_rust_arguments %}, {{ handle.closure.rust_arguments }}{% endif %});
{% include "bridge/jni/closure/handle_cleanup.c" %}
{%- else %}
    {{ handle.closure.c_return_type }} result = closure->call(closure->context{% if handle.closure.has_rust_arguments %}, {{ handle.closure.rust_arguments }}{% endif %});
{% include "bridge/jni/closure/handle_cleanup.c" %}
{%- if handle.closure.returns_bytes %}
    return boltffi_jni_buffer_to_byte_array(env, result);
{%- else if handle.closure.returns_record %}
    return boltffi_jni_record_to_byte_array(env, &result, (uintptr_t)sizeof(result));
{%- else if handle.closure.returns_callback_handle %}
    return boltffi_jni_callback_handle_new_owned(env, result);
{%- else %}
    return ({{ handle.jni_return_type }})result;
{%- endif %}
{%- endif %}
}
