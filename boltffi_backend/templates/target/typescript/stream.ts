const {{ factory }} = ({% match owner %}{% when Some with (_) %}ownerHandle: number{% when None %}{% endmatch %}): StreamSession<{{ item.ty }}> => {
  const handle = (_exports.{{ subscribe }} as Function)({% match owner %}{% when Some with (_) %}ownerHandle{% when None %}{% endmatch %}) as number;
  return new StreamSession<{{ item.ty }}>(
    handle,
    (subscription, maxCount) => {
{% if item.size > 0 %}      const allocation = _module.allocStreamBuffer(maxCount, {{ item.size }});
      try {
        const count = (_exports.{{ pop_batch }} as Function)(subscription, allocation.ptr, maxCount) as number;
        if (count === 0) {
          return [];
        }
{% match item.bulk %}{% when Some with (bulk) %}        return Array.from(_module.{{ bulk }}(allocation.ptr, count));
{% when None %}        const reader = _module.readerFromMemory(allocation.ptr, count * {{ item.size }});
{% match item.direct_decode %}{% when Some with (decode) %}        return Array.from({ length: count }, () => {{ decode }});
{% when None %}        return [];
{% endmatch %}{% endmatch %}      } finally {
        _module.freeAlloc(allocation);
      }
{% else %}      const packed = (_exports.{{ pop_batch }} as Function)(subscription, maxCount) as bigint;
      if (packed === 0n) {
        return [];
      }
      const reader = _module.takePackedBuffer(packed);
{% match item.encoded_array %}{% when Some with (array) %}      return Array.from(reader.{{ array }}());
{% when None %}{% match item.encoded_decode %}{% when Some with (decode) %}      return reader.readArray(() => {{ decode }});
{% when None %}      return [];
{% endmatch %}{% endmatch %}{% endif %}    },
    (subscription) => {
      (_exports.{{ poll }} as Function)(subscription);
    },
    _module.streamManager,
    (subscription) => {
      (_exports.{{ unsubscribe }} as Function)(subscription);
    },
    (subscription) => {
      (_exports.{{ free }} as Function)(subscription);
    },
  );
};

{% match owner %}{% when Some with (owner) %}export interface {{ owner }} {
{% if asynchronous %}  {{ name }}(): AsyncIterable<{{ item.ty }}>;
{% else if batch %}  {{ name }}(): StreamSession<{{ item.ty }}>;
{% else if callback %}  {{ name }}(callback: (item: {{ item.ty }}) => void): StreamCancellable<{{ item.ty }}>;
{% endif %}}

{{ owner }}.prototype.{{ name }} = function (this: {{ owner }}{% if callback %}, callback: (item: {{ item.ty }}) => void{% endif %}) {
  const stream = {{ factory }}({{ owner }}._toHandle(this));
{% if callback %}  return stream.consume(callback);
{% else %}  return stream;
{% endif %}};
{% when None %}{% if asynchronous %}export function {{ name }}(): AsyncIterable<{{ item.ty }}> {
  return {{ factory }}();
}
{% else if batch %}export function {{ name }}(): StreamSession<{{ item.ty }}> {
  return {{ factory }}();
}
{% else if callback %}export function {{ name }}(callback: (item: {{ item.ty }}) => void): StreamCancellable<{{ item.ty }}> {
  return {{ factory }}().consume(callback);
}
{% endif %}{% endmatch %}
