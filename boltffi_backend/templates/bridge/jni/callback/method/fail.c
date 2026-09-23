{% for completion in method.completions %}
    {{ completion.callback }}({{ completion.failure_arguments }});
{% endfor -%}
{% if method.returns_void %}
    return;
{%- else if method.returns_error %}
    return callback_error;
{%- else %}
    return {{ method.failure_value }};
{%- endif %}
