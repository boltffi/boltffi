{%- for parameter in method.borrowed_arrays %}
    {{ parameter.element_type }} *{{ parameter.pointer }} = NULL;
    jsize {{ parameter.length }} = 0;
{%- if let Some(stack) = parameter.stack_copy %}
    {{ parameter.element_type }} {{ stack.storage }}[{{ stack.max_len }}];
    bool {{ stack.needs_release }} = false;
{%- endif %}
{%- endfor %}
{%- for parameter in method.direct_buffers %}
    void *{{ parameter.pointer }} = NULL;
{%- if let Some(writeback) = parameter.writeback %}
    FfiBuf_u8 {{ writeback.local }} = {0};
{%- endif %}
{%- endfor %}
{%- for parameter in method.record_buffers %}
    void *{{ parameter.pointer }} = NULL;
    {{ parameter.c_type }} {{ parameter.local }};
{%- if let Some(writeback) = parameter.writeback %}
    {{ writeback.c_type }} {{ writeback.local }};
{%- endif %}
{%- endfor %}
{%- if let Some(success_out) = method.success_out %}
    {{ success_out.c_type() }} {{ success_out.local() }} = ({{ success_out.c_type() }}){0};
{%- endif %}
{%- if method.checks_completion_status %}
    FfiStatus __boltffi_status = (FfiStatus){0};
{%- endif %}
