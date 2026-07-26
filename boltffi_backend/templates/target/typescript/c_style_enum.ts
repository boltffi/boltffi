

export type {{ name }} =
{% for variant in variants %}  | {{ variant.value }}{% if loop.last %};{% endif %}
{% endfor %}

export const {{ name }} = {
{% for variant in variants %}  {{ variant.name }}: {{ variant.value }},
{% endfor %}{% for method in methods %}  {{ method }}
{% endfor %}} as const;
{% if error %}
export class {{ name }}Exception extends Error {
  constructor(public readonly code: {{ name }}) {
    super("{{ name }}");
    this.name = "{{ name }}Exception";
  }
}
{% endif %}

const {{ codec }}: WireCodec<{{ name }}> = {
  size: () => {{ size }},
  encode: (writer, value) => {
    writer.{{ write }}(value);
  },
  decode: (reader) => {
    const value = reader.{{ read }}();
    switch (value) {
{% for variant in variants %}      case {{ variant.value }}: return {{ name }}.{{ variant.name }};
{% endfor %}      default: throw new Error(`Unknown {{ name }} wire tag: ${value}`);
    }
  },
};
