{%- if record.is_native_opaque() %}
{{ record.documentation() }}public struct {{ record.name() }}: Sendable {
    private final class NativeStorage: @unchecked Sendable {
        let handle: UnsafeMutableRawPointer
        init(handle: UnsafeMutableRawPointer) { self.handle = handle }
        deinit { {{ record.native_opaque_drop() }}(handle) }
    }
    private let storage: NativeStorage
    @usableFromInline init(nativeHandle: UnsafeMutableRawPointer) {
        self.storage = NativeStorage(handle: nativeHandle)
    }
{%- for field in record.opaque_fields() %}
{{ field.documentation() }}    public var {{ field.name() }}: {{ field.ty() }} {
        return withExtendedLifetime(storage) { storage in
{%- if field.optional() %}
        guard {{ field.has_fn().unwrap() }}(UnsafeRawPointer(storage.handle)) != 0 else { return nil }
{%- endif %}
{%- if field.is_primitive() %}
        return {{ field.get_fn().unwrap() }}(UnsafeRawPointer(storage.handle))
{%- else if field.is_string() %}
        var ptr: UnsafePointer<UInt8>? = nil
        var len: UInt = 0
        guard {{ field.borrow_fn().unwrap() }}(UnsafeRawPointer(storage.handle), &ptr, &len) != 0 else {
            preconditionFailure("native opaque borrow failed for {{ field.name() }}")
        }
        if len == 0 { return "" }
        guard let ptr else {
            preconditionFailure("native opaque {{ field.name() }}: nil pointer with non-zero length")
        }
        guard len <= UInt(Int.max) else {
            preconditionFailure("native opaque {{ field.name() }} length overflows Int")
        }
        guard let str = String(bytes: UnsafeBufferPointer(start: ptr, count: Int(len)), encoding: .utf8) else {
            preconditionFailure("native opaque {{ field.name() }} field is not valid UTF-8")
        }
        return str
{%- else %}
        var ptr: UnsafePointer<UInt8>? = nil
        var len: UInt = 0
        guard {{ field.borrow_fn().unwrap() }}(UnsafeRawPointer(storage.handle), &ptr, &len) != 0 else {
            preconditionFailure("native opaque borrow failed for {{ field.name() }}")
        }
        if len == 0 { return Data() }
        guard let ptr else {
            preconditionFailure("native opaque {{ field.name() }}: nil pointer with non-zero length")
        }
        guard len <= UInt(Int.max) else {
            preconditionFailure("native opaque {{ field.name() }} length overflows Int")
        }
        return Data(bytes: ptr, count: Int(len))
{%- endif %}
        }
    }
{%- endfor %}
}
{%- else -%}
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
{%- endif %}
