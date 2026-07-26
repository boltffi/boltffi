const {{ name }} = (): {{ returns }} => {
{% for statement in body %}  {{ statement }}
{% endfor %}};
