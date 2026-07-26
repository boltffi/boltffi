{%- for bytes in closure.byte_arrays %}
    if ({{ bytes.name }} != NULL) {
        (*env)->DeleteLocalRef(env, {{ bytes.name }});
    }
{%- endfor %}
{%- for vector in closure.direct_vectors %}
    if ({{ vector.name }} != NULL) {
        (*env)->DeleteLocalRef(env, {{ vector.name }});
    }
{%- endfor %}
{%- for record in closure.records %}
    if ({{ record.array }} != NULL) {
        (*env)->DeleteLocalRef(env, {{ record.array }});
    }
{%- endfor %}
{%- for handle in closure.closure_handles %}
    {{ handle.handle_release }}({{ handle.handle }});
{%- endfor %}
