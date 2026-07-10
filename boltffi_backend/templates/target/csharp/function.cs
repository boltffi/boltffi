{% if function.checks_status %}        public static {{ function.public_return_type }} {{ function.name }}({% for parameter in function.parameters %}{{ parameter.ty }} {{ parameter.name }}{% if !loop.last %}, {% endif %}{% endfor %})
        {
            FfiStatus status = {{ function.invocation }};
            if (status.code != 0)
            {
                throw new global::System.InvalidOperationException($"BoltFFI call failed with status code {status.code}");
            }
        }
{% else %}        public static {{ function.public_return_type }} {{ function.name }}({% for parameter in function.parameters %}{{ parameter.ty }} {{ parameter.name }}{% if !loop.last %}, {% endif %}{% endfor %})
            => {{ function.invocation }};
{% endif %}
