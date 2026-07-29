const {{ finalizer }}: FinalizationRegistry<number> | null =
  typeof FinalizationRegistry === "undefined"
    ? null
    : new FinalizationRegistry<number>((handle) => {
        (_exports.{{ release }} as Function)(handle);
      });

export class {{ name }} extends BoltFFIHandle {
  protected static override _typeName = "{{ name }}";

  private constructor(handle: number) {
    super(handle);
    {{ finalizer }}?.register(this, handle, this);
  }

  protected override _release(handle: number): void {
    (_exports.{{ release }} as Function)(handle);
  }

  protected override _unregister(): void {
    {{ finalizer }}?.unregister(this);
  }

  static _fromHandle(handle: number): {{ name }} {
    if (handle === 0) {
      throw new Error("{{ name }} received a null handle");
    }
    return new {{ name }}(handle);
  }
{% for constant in constants.members %}
  {{ constant }}
{% endfor %}
{% for method in methods %}
  {{ method }}
{% endfor %}}
{% for function in constants.functions %}
{{ function }}
{% endfor %}
