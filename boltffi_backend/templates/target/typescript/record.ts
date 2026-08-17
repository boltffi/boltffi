

export interface {{ name }} {
{% for field in fields %}  readonly {{ field.key }}{% if field.default.is_some() %}?{% endif %}: {{ field.ty }};
{% endfor %}}
{% if error %}
export class {{ name }}Exception extends Error {
  constructor(public readonly value: {{ name }}) {
    super("{{ name }}");
    this.name = "{{ name }}Exception";
  }
}
{% endif %}

const {{ codec }}: WireCodec<{{ name }}> = {
  size: (value) => {{ size }},
  encode: (writer, value) => {
{% for statement in writes %}    {{ statement }}
{% endfor %}  },
  decode: (reader) => {
{% for statement in reads %}    {{ statement }}
{% endfor %}    return {
{% for field in fields %}      {{ field.key }}: {{ field.local }},
{% endfor %}    };
  },
};
{% if !methods.is_empty() || !constants.members.is_empty() %}
export const {{ name }} = {
{% for constant in constants.members %}  {{ constant }}
{% endfor %}{% for method in methods %}  {{ method }}
{% endfor %}};
{% endif %}
{% for function in constants.functions %}
{{ function }}
{% endfor %}
