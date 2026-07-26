{%- for record in closure.records %}
    jbyteArray {{ record.array }} = NULL;
{% endfor %}
