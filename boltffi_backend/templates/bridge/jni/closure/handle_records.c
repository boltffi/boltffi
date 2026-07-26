{%- for record in handle.closure.handle_records %}
    {{ record.value_declaration }}
    if (!boltffi_jni_read_record(env, {{ record.parameter }}, (uintptr_t)sizeof({{ record.value }}), &{{ record.value }})) {
{%- if handle.closure.returns_void %}
        return;
{%- else %}
        return {{ handle.failure_value }};
{%- endif %}
    }
{% endfor %}
