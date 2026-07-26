{%- for record in closure.records %}
    {{ record.array }} = boltffi_jni_record_to_byte_array(env, &{{ record.parameter }}, (uintptr_t)sizeof({{ record.parameter }}));
    if ({{ record.array }} == NULL) {
        boltffi_jni_clear_exception(env);
{% include "bridge/jni/closure/cleanup.c" %}
        boltffi_jni_exit(env, attached);
{% include "bridge/jni/closure/fail.c" %}
    }
{% endfor %}
