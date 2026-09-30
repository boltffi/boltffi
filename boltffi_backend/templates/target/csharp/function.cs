{{ function.documentation }}{% if function.constant_property %}        {{ function.visibility }} static {{ function.public_return_type }} {{ function.name }}
        {
            get
            {
{% if let Some(body) = function.body %}{{ body }}
{% else %}                return {{ function.invocation }};
{% endif %}            }
        }
{% else if function.asynchronous.is_some() %}{% if let Some(body) = function.body %}        {{ function.visibility }} {% if function.is_static %}static {% endif %}global::System.Threading.Tasks.Task{% if !function.returns_void %}<{{ function.public_return_type }}>{% endif %} {{ function.name }}({% if let Some(owner) = function.extension_owner %}this {{ owner }} self, {% endif %}{% for parameter in function.parameter_declarations() %}{{ parameter }}, {% endfor %}global::System.Threading.CancellationToken {{ function.names.cancellation_token }} = default)
        {
{{ body }}
        }
{% endif %}{% else if let Some(body) = function.body %}        {{ function.visibility }} {% if function.is_static %}static {% endif %}{{ function.public_return_type }} {{ function.name }}({% if let Some(owner) = function.extension_owner %}this {{ owner }} self{% if !function.parameters.is_empty() %}, {% endif %}{% endif %}{% for parameter in function.parameter_declarations() %}{{ parameter }}{% if !loop.last %}, {% endif %}{% endfor %})
        {
{{ body }}
        }
{% else if function.checks_status %}        {{ function.visibility }} {% if function.is_static %}static {% endif %}{{ function.public_return_type }} {{ function.name }}({% if let Some(owner) = function.extension_owner %}this {{ owner }} self{% if !function.parameters.is_empty() %}, {% endif %}{% endif %}{% for parameter in function.parameter_declarations() %}{{ parameter }}{% if !loop.last %}, {% endif %}{% endfor %})
        {
            FfiStatus {{ function.names.status }} = {{ function.invocation }};
            if ({{ function.names.status }}.code != 0)
            {
                throw new global::System.InvalidOperationException($"BoltFFI call failed with status code {{ "{" }}{{ function.names.status }}.code}");
            }
{% if let Some(value) = function.return_after_status %}            return {{ value }};
{% endif %}        }
{% else %}        {{ function.visibility }} {% if function.is_static %}static {% endif %}{{ function.public_return_type }} {{ function.name }}({% if let Some(owner) = function.extension_owner %}this {{ owner }} self{% if !function.parameters.is_empty() %}, {% endif %}{% endif %}{% for parameter in function.parameter_declarations() %}{{ parameter }}{% if !loop.last %}, {% endif %}{% endfor %})
            => {{ function.invocation }};
{% endif %}
{% if function.visibility == "public" %}{% for overload in function.overloads %}
        [global::System.Runtime.CompilerServices.OverloadResolutionPriority(-1)]
        public {% if function.is_static %}static {% endif %}{% if function.asynchronous.is_some() %}global::System.Threading.Tasks.Task{% if !function.returns_void %}<{{ function.public_return_type }}>{% endif %}{% else %}{{ function.public_return_type }}{% endif %} {{ function.name }}({% if let Some(owner) = function.extension_owner %}this {{ owner }} self{% if !overload.parameters.is_empty() || function.asynchronous.is_some() %}, {% endif %}{% endif %}{% for parameter in overload.parameter_declarations() %}{{ parameter }}{% if !loop.last || function.asynchronous.is_some() %}, {% endif %}{% endfor %}{% if function.asynchronous.is_some() %}global::System.Threading.CancellationToken {{ function.names.cancellation_token }} = default{% endif %})
            => {{ function.name }}({% if function.extension_owner.is_some() %}self{% if !overload.arguments.is_empty() || function.asynchronous.is_some() %}, {% endif %}{% endif %}{% for argument in overload.arguments %}{{ argument }}{% if !loop.last || function.asynchronous.is_some() %}, {% endif %}{% endfor %}{% if function.asynchronous.is_some() %}{{ function.names.cancellation_token }}{% endif %});
{% endfor %}{% endif %}
