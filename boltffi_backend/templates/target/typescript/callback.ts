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
{% let guarded = method.transfers_classes() || method.fallible.is_some() %}{% let body_indent %}{% if guarded %}    {% else %}  {% endif %}{% endlet %}{% if method.transfers_classes() %}  let __boltffiClassesDelivered = false;
{% for parameter in method.parameters %}{% if parameter.class_release.is_some() %}  let {{ parameter.argument }}!: {{ parameter.public_type }};
{% endif %}{% endfor %}{% endif %}{% if guarded %}  try {
{% endif %}{% for parameter in method.parameters %}{% if parameter.class_release.is_some() %}{% for statement in parameter.setup %}{{ body_indent }}{{ statement }}
{% endfor %}{{ body_indent }}{{ parameter.bindings[0].name }} = 0;
{% endif %}{% endfor %}{{ body_indent }}const callback = {{ registry }}.get(handle);
{% if let Some(lookup) = method.method_lookup %}    const __boltffiInvoke = {{ lookup }};
    if (typeof __boltffiInvoke !== "function") throw new TypeError("callback method is not callable");
{% endif %}{% for parameter in method.parameters %}{% if parameter.class_release.is_none() %}{% for statement in parameter.setup %}{{ body_indent }}{{ statement }}
{% endfor %}{% endif %}{% endfor %}{% if method.transfers_classes() %}    __boltffiClassesDelivered = true;
{% endif %}{% match method.fallible %}{% when Some with (fallible) %}    const result = {{ method.invocation }};
    return matchWireResult(result, (success) => {
{% for statement in fallible.success_setup %}      {{ statement }}
{% endfor %}{% if fallible.encoded_success %}      _module.writeU64(successPointer, (BigInt(resultWriter.len) << 32n) | BigInt(resultWriter.ptr >>> 0));
{% endif %}      return 0n;
    }, (error) => {
{% for statement in fallible.error_setup %}      {{ statement }}
{% endfor %}      return (BigInt(resultWriter.len) << 32n) | BigInt(resultWriter.ptr >>> 0);
    });
{% when None %}{% if method.returns_void %}{{ body_indent }}{{ method.invocation }};
{% else if method.returns_string %}{{ body_indent }}const result = {{ method.invocation }};
{{ body_indent }}const allocation = _module.allocOwnedString(result);
{{ body_indent }}return (BigInt(allocation.len) << 32n) | BigInt(allocation.ptr >>> 0);
{% else if method.returns_direct_record %}{{ body_indent }}const result = {{ method.invocation }};
{% for statement in method.encoded_setup %}{{ body_indent }}{{ statement }}
{% endfor %}
{% else if method.returns_encoded %}{{ body_indent }}const result = {{ method.invocation }};
{% for statement in method.encoded_setup %}{{ body_indent }}{{ statement }}
{% endfor %}{% match method.return_pointer %}{% when Some with (pointer) %}{{ body_indent }}_module.writeCallbackBuffer({{ pointer }}, resultWriter.ptr, resultWriter.len, resultWriter.capacity);
{% when None %}{% endmatch %}{% else if method.returns_scalar_option %}{{ body_indent }}const result = {{ method.invocation }};
{{ body_indent }}return _module.{{ method.scalar_option_pack }}(result);
{% else %}{% match method.vector_return %}{% when Some with (vector) %}{{ body_indent }}const result = {{ method.invocation }};
{{ body_indent }}const allocation = {{ vector.allocation }};
{{ body_indent }}_module.{{ vector.write_method }}(allocation, {{ vector.alignment }});
{% when None %}{{ body_indent }}return {{ method.invocation }};
{% endmatch %}
{% endif %}{% endmatch %}{% if method.fallible.is_some() %}  } catch (error) {
    const errorWriter = writeUnexpectedCallbackError(_module, error);
    return (BigInt(errorWriter.len) << 32n) | BigInt(errorWriter.ptr >>> 0);
{% endif %}{% if method.transfers_classes() %}  } finally {
    if (!__boltffiClassesDelivered) {
{% for parameter in method.parameters %}{% if let Some(release) = parameter.class_release %}      {{ parameter.argument }}?.dispose();
      if ({{ parameter.bindings[0].name }} !== 0) (_exports.{{ release }} as Function)({{ parameter.bindings[0].name }});
{% endif %}{% endfor %}    }
{% endif %}{% if guarded %}  }
{% endif %}};

{% endfor %}
{% for method in async_methods %}_callbackImports[{{ method.import }}] = (handle: number, requestId: number{% for parameter in method.parameters %}{% for binding in parameter.bindings %}, {{ binding.name }}: {{ binding.carrier_type }}{% endfor %}{% endfor %}): void => {
  const complete = _exports.{{ method.complete }} as Function;
  let callback: {{ name }};
  try {
    callback = {{ registry }}.get(handle);
  } catch (error) {
    const errorWriter = writeUnexpectedCallbackError(_module, error);
    complete(requestId, -2, errorWriter.ptr, errorWriter.len, errorWriter.capacity);
    return;
  }
{% for parameter in method.parameters %}{% for statement in parameter.setup %}  {{ statement }}
{% endfor %}{% endfor %}  Promise.resolve()
    .then(() => {{ method.invocation }})
    .then((result) => {
{% match method.fallible %}{% when Some with (fallible) %}      matchWireResult(result, (success) => {
{% if method.returns_void %}        complete(requestId, 0, 0, 0, 0);
{% else %}{% for statement in method.success_setup %}        {{ statement }}
{% endfor %}        complete(requestId, 0, resultWriter.ptr, resultWriter.len, resultWriter.capacity);
{% endif %}      }, (error) => {
{% for statement in fallible.error_setup %}        {{ statement }}
{% endfor %}        complete(requestId, 1, resultWriter.ptr, resultWriter.len, resultWriter.capacity);
      });
{% when None %}{% if method.returns_void %}      complete(requestId, 0, 0, 0, 0);
{% else %}{% for statement in method.success_setup %}      {{ statement }}
{% endfor %}      complete(requestId, 0, resultWriter.ptr, resultWriter.len, resultWriter.capacity);
{% endif %}{% endmatch %}    })
    .catch((error) => {
      const errorWriter = writeUnexpectedCallbackError(_module, error);
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
