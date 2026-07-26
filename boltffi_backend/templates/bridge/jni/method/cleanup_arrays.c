{%- for cleanup in method.borrowed_arrays %}
    if ({{ cleanup.pointer }} != NULL) {
{%- if let Some(stack) = cleanup.stack_copy %}
        if ({{ stack.needs_release }}) {
            (*env)->{{ cleanup.releaser }}(env, {{ cleanup.name }}, {{ cleanup.pointer }}, {{ cleanup.release_mode }});
        }
{%- else %}
        (*env)->{{ cleanup.releaser }}(env, {{ cleanup.name }}, {{ cleanup.pointer }}, {{ cleanup.release_mode }});
{%- endif %}
        {{ cleanup.pointer }} = NULL;
    }
{%- endfor %}
