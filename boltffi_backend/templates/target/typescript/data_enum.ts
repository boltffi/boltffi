

export type {{ name }} =
{% for variant in variants %}  | { readonly tag: {{ variant.tag }}{% for field in variant.fields %}; readonly {{ field.key }}: {{ field.ty }}{% endfor %} }{% if loop.last %};{% endif %}
{% endfor %}
{% if !methods.is_empty() %}
export const {{ name }} = {
{% for method in methods %}  {{ method }}
{% endfor %}};
{% endif %}
{% if error %}
export class {{ name }}Exception extends Error {
  constructor(public readonly value: {{ name }}) {
    super("{{ name }}");
    this.name = "{{ name }}Exception";
  }
}
{% endif %}
const {{ codec }}: WireCodec<{{ name }}> = {
  size: (value) => {
    switch (value.tag) {
{% for variant in variants %}      case {{ variant.tag }}: return {{ variant.size }};
{% endfor %}    }
  },
  encode: (writer, value) => {
    switch (value.tag) {
{% for variant in variants %}      case {{ variant.tag }}:
        writer.writeI32({{ variant.wire_tag }});
{% for statement in variant.writes %}        {{ statement }}
{% endfor %}        break;
{% endfor %}    }
  },
  decode: (reader) => {
    const tag = reader.readI32();
    switch (tag) {
{% for variant in variants %}      case {{ variant.wire_tag }}: return { tag: {{ variant.tag }}{% for field in variant.reads %}, {{ field.key }}: {{ field.value }}{% endfor %} };
{% endfor %}      default: throw new Error(`Unknown {{ name }} wire tag: ${tag}`);
    }
  },
};
