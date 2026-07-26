export interface {{ name }} {
{% for method in methods %}  {{ method.name }}({% for parameter in method.parameters %}{{ parameter.name }}: {{ parameter.public_type }}{% if !loop.last %}, {% endif %}{% endfor %}): {{ method.public_return }};
{% endfor %}{% for method in async_methods %}  {{ method.name }}({% for parameter in method.parameters %}{{ parameter.name }}: {{ parameter.public_type }}{% if !loop.last %}, {% endif %}{% endfor %}): Promise<{{ method.public_return }}>;
{% endfor %}}

const {{ registry }} = new CallbackRegistry<{{ name }}>({{ registry_name }});

export function {{ register }}(callback: {{ name }}): number {
  const handle = {{ registry }}.register(callback);
  return ((_exports.{{ create_handle }} as Function)(handle) as number) >>> 0;
}

export function {{ unregister }}(handle: number): void {
  {{ registry }}.release(handle);
}

_callbackImports[{{ free_import }}] = (handle: number): void => {
  {{ registry }}.release(handle);
};

_callbackImports[{{ clone_import }}] = (handle: number): number => {
  return {{ registry }}.retain(handle);
};

{% for method in methods %}_callbackImports[{{ method.import }}] = (handle: number{% match method.return_pointer %}{% when Some with (pointer) %}, {{ pointer }}: number{% when None %}{% endmatch %}{% for parameter in method.parameters %}{% for binding in parameter.bindings %}, {{ binding.name }}: {{ binding.carrier_type }}{% endfor %}{% endfor %}): {{ method.carrier_return }} => {
  const callback = {{ registry }}.get(handle);
{% for parameter in method.parameters %}{% for statement in parameter.setup %}  {{ statement }}
{% endfor %}{% endfor %}{% match method.fallible %}{% when Some with (fallible) %}  const result = {{ method.invocation }};
  return matchWireResult(result, (success) => {
{% for statement in fallible.success_setup %}    {{ statement }}
{% endfor %}{% if fallible.encoded_success %}    _module.writeU64(successPointer, (BigInt(resultWriter.len) << 32n) | BigInt(resultWriter.ptr >>> 0));
{% endif %}    return 0n;
  }, (error) => {
{% for statement in fallible.error_setup %}    {{ statement }}
{% endfor %}    return (BigInt(resultWriter.len) << 32n) | BigInt(resultWriter.ptr >>> 0);
  });
{% when None %}{% if method.returns_void %}  {{ method.invocation }};
{% else if method.returns_string %}  const result = {{ method.invocation }};
  const allocation = _module.allocOwnedString(result);
  return (BigInt(allocation.len) << 32n) | BigInt(allocation.ptr >>> 0);
{% else if method.returns_direct_record %}  const result = {{ method.invocation }};
{% for statement in method.encoded_setup %}  {{ statement }}
{% endfor %}
{% else if method.returns_encoded %}  const result = {{ method.invocation }};
{% for statement in method.encoded_setup %}  {{ statement }}
{% endfor %}{% match method.return_pointer %}{% when Some with (pointer) %}  _module.writeCallbackBuffer({{ pointer }}, resultWriter.ptr, resultWriter.len, resultWriter.capacity);
{% when None %}{% endmatch %}{% else if method.returns_scalar_option %}  const result = {{ method.invocation }};
  return _module.{{ method.scalar_option_pack }}(result);
{% else %}{% match method.vector_return %}{% when Some with (vector) %}  const result = {{ method.invocation }};
  const allocation = {{ vector.allocation }};
  _module.{{ vector.write_method }}(allocation, {{ vector.alignment }});
{% when None %}  return {{ method.invocation }};
{% endmatch %}
{% endif %}{% endmatch %}};

{% endfor %}
{% for method in async_methods %}_callbackImports[{{ method.import }}] = (handle: number, requestId: number{% for parameter in method.parameters %}{% for binding in parameter.bindings %}, {{ binding.name }}: {{ binding.carrier_type }}{% endfor %}{% endfor %}): void => {
  const complete = _exports.{{ method.complete }} as Function;
  let callback: {{ name }};
  try {
    callback = {{ registry }}.get(handle);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    const errorWriter = _module.allocWriter(4 + message.length * 3);
    errorWriter.writeString(message);
    complete(requestId, -2, errorWriter.ptr, errorWriter.len, errorWriter.capacity);
    return;
  }
{% for parameter in method.parameters %}{% for statement in parameter.setup %}  {{ statement }}
{% endfor %}{% endfor %}  Promise.resolve()
    .then(() => {{ method.invocation }})
    .then((result) => {
{% match method.fallible %}{% when Some with (fallible) %}      matchWireResult(result, (success) => {
{% for statement in method.success_setup %}        {{ statement }}
{% endfor %}        complete(requestId, 0, resultWriter.ptr, resultWriter.len, resultWriter.capacity);
      }, (error) => {
{% for statement in fallible.error_setup %}        {{ statement }}
{% endfor %}        complete(requestId, 1, resultWriter.ptr, resultWriter.len, resultWriter.capacity);
      });
{% when None %}{% if method.returns_void %}      complete(requestId, 0, 0, 0, 0);
{% else %}{% for statement in method.success_setup %}      {{ statement }}
{% endfor %}      complete(requestId, 0, resultWriter.ptr, resultWriter.len, resultWriter.capacity);
{% endif %}{% endmatch %}    })
    .catch((error) => {
      const message = error instanceof Error ? error.message : String(error);
      const errorWriter = _module.allocWriter(4 + message.length * 3);
      errorWriter.writeString(message);
      complete(requestId, -2, errorWriter.ptr, errorWriter.len, errorWriter.capacity);
    });
};

{% endfor %}
{% match local %}{% when Some with (local) %}
const {{ local.finalizer }}: FinalizationRegistry<number> | null =
  typeof FinalizationRegistry === "undefined"
    ? null
    : new FinalizationRegistry<number>((handle) => {
        (_exports.{{ local.free }} as Function)(handle);
      });

class {{ local.proxy }} implements {{ name }} {
  private _handle: number;
  private _disposed = false;

  constructor(handle: number) {
    if (handle === 0) {
      throw new Error("{{ name }} received a null handle");
    }
    this._handle = handle;
    {{ local.finalizer }}?.register(this, handle, this);
  }

  dispose(): void {
    if (this._disposed) {
      return;
    }
    this._disposed = true;
    {{ local.finalizer }}?.unregister(this);
    (_exports.{{ local.free }} as Function)(this._handle);
    this._handle = 0;
  }

  private _borrowHandle(): number {
    if (this._disposed) {
      throw new Error("{{ name }} has been disposed");
    }
    return this._handle;
  }
{% for method in local.methods %}
  {{ method }}
{% endfor %}}

export function {{ local.wrap }}(handle: number): {{ name }} {
  return new {{ local.proxy }}(handle);
}
{% when None %}{% endmatch %}
