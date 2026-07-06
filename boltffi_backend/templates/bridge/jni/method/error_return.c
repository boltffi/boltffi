{%- if method.returns_bytes || method.returns_record %}
    return NULL;
{%- else if method.returns_void || (method.checks_status && method.success_out.is_none()) || (method.checks_error_buffer && method.success_out.is_none()) %}
    return;
{%- else if method.returns_boolean %}
    return JNI_FALSE;
{%- else %}
    return 0;
{%- endif %}
