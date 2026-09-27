{%- for bytes in closure.byte_arrays %}
    {{ bytes.name }} = boltffi_jni_bytes_to_byte_array(env, {{ bytes.pointer }}, {{ bytes.length }});
    if ({{ bytes.name }} == NULL) {
        goto __boltffi_fail;
    }
{%- endfor %}
