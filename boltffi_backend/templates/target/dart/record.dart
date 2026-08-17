{%- match record.native() %}
{%- when Some with (native) %}
final class {{ native.name() }} extends $$ffi.Struct {
{%- for field in native.fields() %}
  {{ field.annotation() }}
  external {{ field.ty() }} {{ field.name() }};
{%- if !loop.last %}

{%- endif %}
{%- endfor %}
}

extension on {{ record.name() }} {
  @pragma('vm:prefer-inline')
  {{ native.name() }} _m$toStruct() {
    final _l$result = $$ffi.Struct.create<{{ native.name() }}>();
{%- for field in record.fields() %}
    _l$result.{{ field.name() }} = {{ field.name() }};
{%- endfor %}
    return _l$result;
  }

  @pragma('vm:prefer-inline')
  void _m$writeStruct($$ffi.Pointer<{{ native.name() }}> _p$target) {
{%- for field in record.fields() %}
    _p$target.ref.{{ field.name() }} = {{ field.name() }};
{%- endfor %}
  }

  @pragma('vm:prefer-inline')
  void _m$updateFromStruct({{ native.name() }} _p$value) {
{%- for field in record.fields() %}
    {{ field.name() }} = _p$value.{{ field.name() }};
{%- endfor %}
  }
}

{%- when None %}
{%- endmatch %}
{{ record.documentation() }}final class {{ record.name() }}{{ record.exception_clause() }} {
{%- for field in record.fields() %}
{{ field.documentation() }}  {{ field.ty() }} {{ field.name() }};
{%- if !loop.last %}

{%- endif %}
{%- endfor %}

  {{ record.name() }}({
{%- for field in record.fields() %}
    {{ field.default_clause() }},{% endfor %}
  }){{ record.default_initializers() }};
{%- match record.native() %}
{%- when Some with (native) %}

  @pragma('vm:prefer-inline')
  factory {{ record.name() }}._m$fromStruct({{ native.name() }} _p$value) =>
      {{ record.name() }}(
{%- for field in record.fields() %}
        {{ field.name() }}: _p$value.{{ field.name() }},{% endfor %}
      );
{%- when None %}
{%- endmatch %}

  factory {{ record.name() }}._m$wireDecode(_$$BoltWireDecoder _p$reader) =>
      {{ record.name() }}(
{%- for field in record.fields() %}
        {{ field.name() }}: {{ field.read() }},{% endfor %}
      );

  void _m$wireEncode(_$$BoltWireEncoder _p$writer) {
{%- for field in record.fields() %}
{%- for write in field.writes() %}
    {{ write }}
{%- endfor %}
{%- endfor %}
  }

  int _m$wireEncodedSize() => {{ record.encoded_size() }};

  @override
  int get hashCode{% if record.fields().is_empty() %} => runtimeType.hashCode;{% else %} {
    var _l$result = 1;
{%- for field in record.fields() %}
    _l$result = 31 * _l$result + {{ field.hash() }};
{%- endfor %}
    return _l$result;
  }{% endif %}

  @override
  bool operator ==(Object other) {
    if (identical(this, other)) return true;
    return other is {{ record.name() }}{% for field in record.fields() %} &&
        {{ field.equality() }}{% endfor %};
  }
{%- for member in record.members() %}

{{ member }}
{%- endfor %}
}
