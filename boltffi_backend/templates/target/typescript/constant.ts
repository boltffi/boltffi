{% match self.body() %}{% when Body::Inline with (inline) %}export const {{ self.name }}: {{ inline.ty }} = {{ inline.value }};
{% when Body::Accessor with (accessor) %}export let {{ self.name }}: {{ accessor.ty }};
{{ accessor.function }}
{% endmatch %}
