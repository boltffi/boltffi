

export {% if asynchronous %}async {% endif %}function {{ name }}({% for parameter in parameters %}{{ parameter.name }}: {{ parameter.ty }}{% if !loop.last %}, {% endif %}{% endfor %}{% if let Some(options) = async_options %}{% if !parameters.is_empty() %}, {% endif %}{{ options }}{% endif %}): {% if asynchronous %}Promise<{{ returns }}>{% else %}{{ returns }}{% endif %} {
{% for statement in body %}  {{ statement }}
{% endfor %}
}
