{% if asynchronous %}async {% endif %}{{ name }}({% for parameter in parameters %}{{ parameter.name }}: {{ parameter.ty }}{% if !loop.last %}, {% endif %}{% endfor %}): {% if asynchronous %}Promise<{{ returns }}>{% else %}{{ returns }}{% endif %} {
{% for statement in body %}  {{ statement }}
{% endfor %}},
