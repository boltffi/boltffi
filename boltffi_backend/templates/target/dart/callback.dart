{{ callback.register_declaration() }}
{{ callback.create_declaration() }}
final class {{ callback.native_vtable().name() }} extends $$ffi.Struct {
{%- for field in callback.native_vtable().fields() %}
  external {{ field.ty() }} {{ field.name() }};
{%- if !loop.last %}

{%- endif %}
{%- endfor %}
}

{{ callback.documentation() }}abstract interface class {{ callback.name() }} {
{%- for method in callback.interface_methods() %}
{{ method }}
{%- if !loop.last %}

{%- endif %}
{%- endfor %}
}

final class {{ callback.proxy_name() }} implements {{ callback.name() }} {
  static final Finalizer<_$$BoltCallbackHandle> _finalizer =
      Finalizer<_$$BoltCallbackHandle>((handle) {
    final vtable =
        handle.vtable.cast<{{ callback.native_vtable().name() }}>().ref;
    vtable.free.asFunction<void Function(int)>()(handle.handle);
  });

  _$$BoltCallbackHandle _handle;
  final {{ callback.native_vtable().name() }} _vtable;

  {{ callback.proxy_name() }}(this._handle)
      : _vtable =
            _handle.vtable.cast<{{ callback.native_vtable().name() }}>().ref {
    _finalizer.attach(this, _handle, detach: this);
  }

  _$$BoltCallbackHandle _m$cloneHandle() {
    if (_handle.handle == 0) return _k$BoltCallbackHandleNull;
    final cloned =
        _vtable.clone.asFunction<int Function(int)>()(_handle.handle);
    return $$ffi.Struct.create<_$$BoltCallbackHandle>()
      ..handle = cloned
      ..vtable = _handle.vtable;
  }
{%- for method in callback.proxy_methods() %}

{{ method }}
{%- endfor %}
}

final class {{ callback.bridge_name() }} {
  static final _$$BoltFFIHandleMap<{{ callback.name() }}> _k$handles =
      _$$BoltFFIHandleMap<{{ callback.name() }}>();
{%- for callable in callback.callables() %}

{{ callable }}
{%- endfor %}
{%- for declaration in callback.shim_declarations() %}

{{ declaration }}
{%- endfor %}

  static final _$$BoltCallocPtr<{{ callback.native_vtable().name() }}> _k$vtable = (() {
    final vtable = _$$BoltCallocPtr<{{ callback.native_vtable().name() }}>.alloc(
      $$ffi.sizeOf<{{ callback.native_vtable().name() }}>(),
    );
    vtable.ptr.ref
{{ callback.free_vtable_initializer() }}
{{ callback.clone_vtable_initializer() }}
{%- for initializer in callback.vtable_initializers() %}
{{ initializer }}
{%- endfor %};
    return vtable;
  })();

  static final bool _k$registered = (() {
    _f${{ callback.register_name() }}(_k$vtable.ptr);
    return true;
  })();

  static _$$BoltCallbackHandle create({{ callback.name() }}? implementation) {
    _k$registered;
    if (implementation == null) return _k$BoltCallbackHandleNull;
    if (implementation is {{ callback.proxy_name() }}) {
      return implementation._m$cloneHandle();
    }
    // `_k$registered` above already guarantees `_k$vtable` is registered
    // with Rust, so there's nothing left for a native round-trip to check --
    // both fields are already known here, so build the handle directly.
    //
    // Captures the calling isolate's thread as this registration's owner.
    // Also doubles as the dispatch handle itself (not a locally-numbered
    // one from `_k$handles.insert`): boltffi_dart_runtime's hooks table is
    // one process-wide map keyed by this same handle across every callback
    // type, so a per-type counter starting back at a small number for each
    // type would collide across types.
    final instanceHandle = _f$boltffi_dart_runtime_create_instance();
    _k$handles.insertAt(instanceHandle, implementation);
    {{ callback.shim_register_call() }}
    return $$ffi.Struct.create<_$$BoltCallbackHandle>()
      ..handle = instanceHandle
      ..vtable = _k$vtable.ptr.cast();
  }

  static {{ callback.name() }} wrap(_$$BoltCallbackHandle handle) {
    if (handle.handle == 0) {
      throw StateError('{{ callback.name() }} callback handle is null');
    }
    return _k$handles.get(handle.handle) ?? {{ callback.proxy_name() }}(handle);
  }

  static void _m$free(int handle) {
    _k$handles.remove(handle);
    _f${{ callback.shim_release_symbol() }}(handle);
  }

  static int _m$clone(int originalHandle) {
    final implementation = _k$handles.get(originalHandle);
    if (implementation == null) return 0;
    // A clone is a genuinely new registration, not an alias -- it needs its
    // own hooks (shim dispatch is keyed by this handle) and its own owner-
    // thread instance, not `originalHandle`'s.
    final instanceHandle = _f$boltffi_dart_runtime_create_instance();
    _k$handles.insertAt(instanceHandle, implementation);
    {{ callback.shim_register_call() }}
    return instanceHandle;
  }
{%- for entry in callback.entries() %}

{{ entry }}
{%- endfor %}
}
