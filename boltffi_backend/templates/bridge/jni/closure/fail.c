{%- if closure.returns_void %}
        return;
{%- else if closure.returns_error %}
        return callback_error;
{%- else %}
        return {{ closure.failure_value }};
{%- endif %}
