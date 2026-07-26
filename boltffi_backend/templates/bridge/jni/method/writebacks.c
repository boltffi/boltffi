{%- for parameter in method.record_buffers %}
{%- if let Some(writeback) = parameter.writeback %}
    memcpy({{ parameter.pointer }}, &{{ writeback.local }}, sizeof({{ writeback.c_type }}));
{%- endif %}
{%- endfor %}
