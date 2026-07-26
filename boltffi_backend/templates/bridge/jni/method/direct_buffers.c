{%- for parameter in method.direct_buffers %}
    if (!boltffi_jni_direct_buffer_address(env, {{ parameter.name }}, (jlong){{ parameter.length }}, &{{ parameter.pointer }})) {
        goto __boltffi_error;
    }
{%- endfor %}
