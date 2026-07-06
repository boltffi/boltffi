{{ class.documentation() }}public final class {{ class.name() }} {
    @usableFromInline let handle: {{ class.handle_type() }}

    @usableFromInline init(handle: {{ class.handle_type() }}) {
        self.handle = handle
    }

    deinit {
        {{ class.release() }}(handle)
    }
{%- for initializer in class.initializers() %}

{{ initializer.documentation() }}{% if initializer.factory() %}    public static func {{ initializer.name() }}({{ initializer.parameter_list() }}){{ initializer.throwing_keyword() }} -> {{ initializer.factory_return() }} {
{% else %}    public init({{ initializer.parameter_list() }}){{ initializer.throwing_keyword() }} {
{% endif -%}
{{ initializer.body() }}
    }
{%- endfor %}
{%- for method in class.static_methods() %}

{{ method.documentation() }}    public static func {{ method.name() }}({{ method.parameter_list() }}){{ method.async_keyword() }}{{ method.returns().signature() }} {
{{ method.body() }}
    }
{%- endfor %}
{%- for method in class.instance_methods() %}

{{ method.documentation() }}    public func {{ method.name() }}({{ method.parameter_list() }}){{ method.async_keyword() }}{{ method.returns().signature() }} {
{{ method.body() }}
    }
{%- endfor %}
}
