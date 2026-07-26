export type {{ name }} = ({% for parameter in parameters %}{{ parameter.imported.name }}: {{ parameter.imported.public_type }}{% if !loop.last %}, {% endif %}{% endfor %}) => {{ public_return }};

const {{ registry }} = new CallbackRegistry<{{ name }}>({{ registry_name }});

export function {{ register }}(callback: {{ name }}): number {
  return {{ registry }}.register(callback);
}

export function {{ unregister }}(handle: number): void {
  {{ registry }}.release(handle);
}

_callbackImports[{{ free_import }}] = (handle: number): void => {
  {{ registry }}.release(handle);
};

_callbackImports[{{ call_import }}] = (handle: number{% for parameter in parameters %}{% for binding in parameter.imported.bindings %}, {{ binding.name }}: {{ binding.carrier_type }}{% endfor %}{% endfor %}{% match return_pointer %}{% when Some with (pointer) %}, {{ pointer }}: number{% when None %}{% endmatch %}{% match fallible %}{% when Some with (fallible) %}, {{ fallible.success_pointer }}: number{% when None %}{% endmatch %}): {{ carrier_return }} => {
  const callback = {{ registry }}.get(handle);
{% for parameter in parameters %}{% for statement in parameter.imported.setup %}  {{ statement }}
{% endfor %}{% endfor %}{% match fallible %}{% when Some with (fallible) %}  const result = {{ invocation }};
  return matchWireResult(result, (success) => {
    _module.{{ fallible.success_write }}({{ fallible.success_pointer }}, success);
    return 0n;
  }, (error) => {
{% for statement in fallible.error_setup %}    {{ statement }}
{% endfor %}    return (BigInt(resultWriter.len) << 32n) | BigInt(resultWriter.ptr >>> 0);
  });
{% when None %}{% if returns_void %}  {{ invocation }};
{% else if returns_string %}  const result = {{ invocation }};
  const allocation = _module.allocOwnedWireString(result);
  return (BigInt(allocation.len) << 32n) | BigInt(allocation.ptr >>> 0);
{% else if returns_direct_record %}  const result = {{ invocation }};
{% for statement in encoded_setup %}  {{ statement }}
{% endfor %}
{% else if returns_encoded %}  const result = {{ invocation }};
{% for statement in encoded_setup %}  {{ statement }}
{% endfor %}  return (BigInt(resultWriter.len) << 32n) | BigInt(resultWriter.ptr >>> 0);
{% else %}  return {{ invocation }};
{% endif %}{% endmatch %}};
