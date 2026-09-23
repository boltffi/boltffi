{%- for record in closure.records %}
    {{ record.array }} = boltffi_jni_record_to_byte_array(env, &{{ record.parameter }}, (uintptr_t)sizeof({{ record.parameter }}));
    if ({{ record.array }} == NULL) {
        goto __boltffi_fail;
    }
{% endfor %}
