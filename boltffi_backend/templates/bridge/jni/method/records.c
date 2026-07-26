{%- for parameter in method.record_buffers %}
    if (!boltffi_jni_direct_buffer_address(env, {{ parameter.name }}, (jlong)sizeof({{ parameter.c_type }}), &{{ parameter.pointer }})) {
        goto __boltffi_error;
    }
    memcpy(&{{ parameter.local }}, {{ parameter.pointer }}, sizeof({{ parameter.c_type }}));
{%- endfor %}
