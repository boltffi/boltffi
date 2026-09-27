{%- for handle in closure.closure_handles %}
    {{ handle.handle }} = {{ handle.handle_new }}(env, {{ handle.call }}, (void *){{ handle.context }}, {{ handle.release }});
    if ((*env)->ExceptionCheck(env)) {
        goto __boltffi_fail;
    }
{%- endfor %}
