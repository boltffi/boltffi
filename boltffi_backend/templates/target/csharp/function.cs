{{ function.documentation }}{% if function.constant_property %}        {{ function.visibility }} static {{ function.public_return_type }} {{ function.name }}
        {
            get
            {
{% if let Some(body) = function.body %}{{ body }}
{% else %}                return {{ function.invocation }};
{% endif %}            }
        }
{% else if function.asynchronous.is_some() %}{% if let Some(body) = function.body %}        {{ function.visibility }} {% if function.is_static %}static {% endif %}global::System.Threading.Tasks.Task{% if !function.returns_void %}<{{ function.public_return_type }}>{% endif %} {{ function.name }}({% if let Some(owner) = function.extension_owner %}this {{ owner }} self, {% endif %}{% for parameter in function.parameters %}{{ parameter.ty }} {{ parameter.name }}, {% endfor %}global::System.Threading.CancellationToken cancellationToken = default)
        {
{{ body }}
        }
{% endif %}{% else if let Some(body) = function.body %}        {{ function.visibility }} {% if function.is_static %}static {% endif %}{{ function.public_return_type }} {{ function.name }}({% if let Some(owner) = function.extension_owner %}this {{ owner }} self{% if !function.parameters.is_empty() %}, {% endif %}{% endif %}{% for parameter in function.parameters %}{{ parameter.ty }} {{ parameter.name }}{% if !loop.last %}, {% endif %}{% endfor %})
        {
{{ body }}
        }
{% else if function.checks_status %}        {{ function.visibility }} {% if function.is_static %}static {% endif %}{{ function.public_return_type }} {{ function.name }}({% if let Some(owner) = function.extension_owner %}this {{ owner }} self{% if !function.parameters.is_empty() %}, {% endif %}{% endif %}{% for parameter in function.parameters %}{{ parameter.ty }} {{ parameter.name }}{% if !loop.last %}, {% endif %}{% endfor %})
        {
            FfiStatus status = {{ function.invocation }};
            if (status.code != 0)
            {
                throw new global::System.InvalidOperationException($"BoltFFI call failed with status code {status.code}");
            }
{% if let Some(value) = function.return_after_status %}            return {{ value }};
{% endif %}        }
{% else %}        {{ function.visibility }} {% if function.is_static %}static {% endif %}{{ function.public_return_type }} {{ function.name }}({% if let Some(owner) = function.extension_owner %}this {{ owner }} self{% if !function.parameters.is_empty() %}, {% endif %}{% endif %}{% for parameter in function.parameters %}{{ parameter.ty }} {{ parameter.name }}{% if !loop.last %}, {% endif %}{% endfor %})
            => {{ function.invocation }};
{% endif %}
