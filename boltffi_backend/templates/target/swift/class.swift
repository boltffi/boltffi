{{ class.documentation() }}public final class {{ class.name() }} {
    private var __boltffiHandle: UInt64 = 0

    @usableFromInline var handle: {{ class.handle_type() }} {
        get { withUnsafeMutablePointer(to: &__boltffiHandle) { {{ class.handle_type() }}(boltffi_atomic_u64_load($0)) } }
        set { __boltffiHandle = UInt64(newValue) }
    }

    @usableFromInline init(handle: {{ class.handle_type() }}) {
        self.handle = handle
    }

    @usableFromInline func __boltffiTakeHandle() -> {{ class.handle_type() }} {
        withUnsafeMutablePointer(to: &__boltffiHandle) { {{ class.handle_type() }}(boltffi_atomic_u64_exchange($0, 0)) }
    }

    deinit {
        let handle = __boltffiTakeHandle()
        if handle != 0 { {{ class.release() }}(handle) }
    }
{%- for constant in constants %}

{{ constant }}
{%- endfor %}
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
