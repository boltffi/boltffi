{{ callback.documentation() }}public protocol {{ callback.name() }}: AnyObject {
{%- for method in callback.methods() %}
    func {{ method.name() }}({{ method.parameter_list() }}){{ method.return_signature() }}
{%- endfor %}
}

private final class {{ callback.wrapper() }} {
    let impl: {{ callback.name() }}

    init(_ impl: {{ callback.name() }}) {
        self.impl = impl
    }
}

{%- if callback.proxy_required() %}
private protocol {{ callback.bridgeable() }}: {{ callback.name() }} {
    func _boltffiRetainedCallbackHandle() -> BoltFFICallbackHandle
}

private final class {{ callback.proxy() }}: {{ callback.name() }}, {{ callback.bridgeable() }} {
    private let handle: BoltFFICallbackHandle

    init(_ handle: BoltFFICallbackHandle) {
        self.handle = handle
    }

    deinit {
        {{ callback.bridge() }}.release(handle)
    }

    func _boltffiRetainedCallbackHandle() -> BoltFFICallbackHandle {
        {{ callback.bridge() }}.clone(handle)
    }
{%- for method in callback.methods() %}

    func {{ method.name() }}({{ method.parameter_list() }}){{ method.return_signature() }} {
{{ method.proxy_body() }}
    }
{%- endfor %}
}
{%- endif %}

private var {{ callback.vtable() }}: {{ callback.vtable_type() }} = {
    {{ callback.vtable_type() }}(
        free: { handle in
            guard handle != 0 else { return }
            Unmanaged<{{ callback.wrapper() }}>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(handle))!).release()
        },
        clone: { handle in
            guard handle != 0 else { return 0 }
            let wrapper = Unmanaged<{{ callback.wrapper() }}>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(handle))!)
            _ = wrapper.retain()
            return handle
        }{% for method in callback.methods() %},
        {{ method.slot() }}: { handle{% for binding in method.return_bindings() %}, {{ binding }}{% endfor %}{% for parameter in method.parameters() %}{% for binding in parameter.bindings() %}, {{ binding }}{% endfor %}{% endfor %}{% for binding in method.completion_bindings() %}, {{ binding }}{% endfor %} in
{{ method.body() }}
        }{% endfor %}
    )
}()

private enum {{ callback.bridge() }} {
    private static let register: Void = {
        {{ callback.register() }}(&{{ callback.vtable() }})
    }()

{%- if callback.proxy_required() %}
    static func release(_ handle: BoltFFICallbackHandle) {
        guard handle.handle != 0,
              let vtable = handle.vtable?.assumingMemoryBound(to: {{ callback.vtable_type() }}.self),
              let free = vtable.pointee.free else { return }
        free(handle.handle)
    }

    static func clone(_ handle: BoltFFICallbackHandle) -> BoltFFICallbackHandle {
        guard handle.handle != 0,
              let vtable = handle.vtable?.assumingMemoryBound(to: {{ callback.vtable_type() }}.self),
              let clone = vtable.pointee.clone else {
            return BoltFFICallbackHandle(handle: 0, vtable: nil)
        }
        return BoltFFICallbackHandle(handle: clone(handle.handle), vtable: handle.vtable)
    }
{%- endif %}

    static func create(_ impl: {{ callback.name() }}) -> BoltFFICallbackHandle {
        _ = register
{%- if callback.proxy_required() %}
        if let bridgeable = impl as? {{ callback.bridgeable() }} {
            return bridgeable._boltffiRetainedCallbackHandle()
        }
{%- endif %}
        let wrapper = {{ callback.wrapper() }}(impl)
        let handle = UInt64(UInt(bitPattern: Unmanaged.passRetained(wrapper).toOpaque()))
        return {{ callback.create_handle() }}(handle)
    }

{%- if callback.proxy_required() %}
    static func wrap(_ handle: BoltFFICallbackHandle) -> any {{ callback.name() }} {
        _ = register
        return {{ callback.proxy() }}(handle)
    }
{%- endif %}
}
