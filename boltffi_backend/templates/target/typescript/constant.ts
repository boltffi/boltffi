{% match inline %}{% when Some with (inline) %}export const {{ inline.name }}: {{ inline.ty }} = {{ inline.value }};
{% when None %}{% endmatch %}{% match accessor %}{% when Some with (accessor) %}export let {{ accessor.name }}: {{ accessor.ty }};
{{ accessor.function }}
{% when None %}{% endmatch %}
