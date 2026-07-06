{{ record.documentation() }}public struct {{ record.name() }}: Hashable, Equatable, Sendable{{ record.error_conformance() }} {
{%- for field in record.fields() %}
{{ field.documentation() }}    public var {{ field.name() }}: {{ field.ty() }}
{%- endfor %}

    public init({{ record.parameter_list() }}) {
{%- for field in record.fields() %}
        {{ field.assignment() }}
{%- endfor %}
    }
{%- if record.direct() %}

    @usableFromInline init(fromC c: {{ record.c_type() }}) {
        self.init({{ record.c_initializer_arguments() }})
    }

    @usableFromInline var cValue: {{ record.c_type() }} {
        {{ record.c_type() }}({{ record.c_value_arguments() }})
    }
{%- endif %}
{%- if record.codec_payload() %}

    @inlinable static func decode(from reader: inout WireReader) -> {{ record.name() }} {
        {{ record.name() }}({{ record.decode_arguments() }})
    }

    @inlinable func encode(to writer: inout WireWriter) {
{%- for field in record.fields() %}
        {{ field.write() }}
{%- endfor %}
    }
{%- endif %}
{%- for initializer in record.initializers() %}

{{ initializer.documentation() }}{% if initializer.factory() %}    public static func {{ initializer.name() }}({{ initializer.parameter_list() }}){{ initializer.throwing_keyword() }} -> {{ initializer.factory_return() }} {
{% else %}    public init{{ initializer.failable_marker() }}({{ initializer.parameter_list() }}){{ initializer.throwing_keyword() }} {
{% endif -%}
{{ initializer.body() }}
    }
{%- endfor %}
{%- for method in record.static_methods() %}

{{ method.documentation() }}    public {{ method.static_keyword() }}{{ method.mutating_keyword() }}func {{ method.name() }}({{ method.parameter_list() }}){{ method.async_keyword() }}{{ method.returns().signature() }} {
{{ method.body() }}
    }
{%- endfor %}
{%- for method in record.instance_methods() %}

{{ method.documentation() }}    public {{ method.static_keyword() }}{{ method.mutating_keyword() }}func {{ method.name() }}({{ method.parameter_list() }}){{ method.async_keyword() }}{{ method.returns().signature() }} {
{{ method.body() }}
    }
{%- endfor %}
}
