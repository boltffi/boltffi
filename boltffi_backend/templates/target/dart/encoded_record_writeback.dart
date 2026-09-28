final _l$selfBuffer = _l$selfOut.ptr.ref;
final _l$selfReader = _$$BoltWireDecoder(_$$BoltBufReader.fromSpan(_l$selfBuffer.ptr, _l$selfBuffer.len));
final _l$selfUpdated = {{ name }}._m$wireDecode(_l$selfReader);
{%- for field in fields %}
this.{{ field }} = _l$selfUpdated.{{ field }};
{%- endfor %}
