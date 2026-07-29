{%- match constant.body() %}
{%- when Body::Inline with (inline) %}
{%- match inline %}
{%- when Inline::Stored with (value) %}
val {{ value.name() }}: {{ value.ty() }} = {{ value.value() }}
{%- when Inline::Computed with (value) %}
val {{ value.name() }}: {{ value.ty() }}
    get() = {{ value.value() }}
{%- endmatch %}
{%- when Body::Accessor with (accessor) %}
val {{ accessor.name() }}{% if let Some(return_type) = accessor.returns() %}: {{ return_type }}{% endif %}
    get() {
{%- for statement in accessor.setup() %}
        {{ statement }}
{%- endfor %}
{%- if accessor.has_cleanup() %}
        try {
{%- for statement in accessor.call() %}
            {{ statement }}
{%- endfor %}
        } finally {
{%- for statement in accessor.cleanup() %}
            {{ statement }}
{%- endfor %}
        }
{%- else %}
{%- for statement in accessor.call() %}
        {{ statement }}
{%- endfor %}
{%- endif %}
    }
{%- endmatch %}
