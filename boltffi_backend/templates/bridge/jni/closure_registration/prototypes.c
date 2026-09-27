{%- for closure in closures %}
static {{ closure.c_return_type }} {{ closure.call }}(void *user_data{% for parameter in closure.c_parameters %}, {{ parameter.declaration() }}{% endfor %});
static void {{ closure.release }}(void *user_data);
{%- endfor %}
