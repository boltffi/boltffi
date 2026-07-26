static {{ callback.vtable_type }} {{ callback.vtable }} = {
    .free = {{ callback.free }},
    .clone = {{ callback.clone }},
{%- for method in callback.methods %}
    .{{ method.vtable_slot }} = {{ method.function }},
{%- endfor %}
};
