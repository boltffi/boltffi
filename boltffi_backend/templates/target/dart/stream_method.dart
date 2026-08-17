{{ method.documentation() }}{% if method.mode().asynchronous() %}$$async.Stream<{{ method.item_type() }}> {{ method.name() }}() {
{%- else if method.mode().callback() %}$$async.StreamSubscription<{{ method.item_type() }}> {{ method.name() }}(void Function({{ method.item_type() }}) callback) {
{%- else %}$$BoltStreamPopBatchHandle<{{ method.item_type() }}> {{ method.name() }}() {
{%- endif %}
{%- if method.context().owned() %}
  _f$throwIfDisposed();
{%- endif %}
  final _l$context = _$$BoltStreamCtx(
    subscribe: () => _f${{ method.context().subscribe() }}({{ method.context().receiver() }}),
    pollFn: _f${{ method.context().poll() }},
    waitFn: _f${{ method.context().wait() }},
    unsubscribeFn: _f${{ method.context().unsubscribe() }},
    freeFn: _f${{ method.context().free() }}{% match method.context().item_size() %}{% when Some with (item_size) %},
    itemSize: {{ item_size }}{% when None %}{% endmatch %},
  );
{%- if method.mode().asynchronous() %}
  return _l$context.stream<{{ method.item_type() }}>(
    (handle, batchSize, itemSize, controller) {
      {{ method.delivery().setup() }}
      if (!({{ method.delivery().has_items() }})) {
{%- match method.delivery().cleanup() %}
{%- when Some with (cleanup) %}
        {{ cleanup }}
{%- when None %}
{%- endmatch %}
        return false;
      }
{%- match method.delivery().prepare() %}
{%- when Some with (prepare) %}
      {{ prepare }}
{%- when None %}
{%- endmatch %}
      final _l$items = {{ method.delivery().read() }};
      _l$items.forEach(controller.add);
{%- match method.delivery().cleanup() %}
{%- when Some with (cleanup) %}
      {{ cleanup }}
{%- when None %}
{%- endmatch %}
      return _l$items.length >= batchSize;
    },
  );
{%- else if method.mode().callback() %}
  final stream = _l$context.stream<{{ method.item_type() }}>(
    (handle, batchSize, itemSize, controller) {
      {{ method.delivery().setup() }}
      if (!({{ method.delivery().has_items() }})) {
{%- match method.delivery().cleanup() %}
{%- when Some with (cleanup) %}
        {{ cleanup }}
{%- when None %}
{%- endmatch %}
        return false;
      }
{%- match method.delivery().prepare() %}
{%- when Some with (prepare) %}
      {{ prepare }}
{%- when None %}
{%- endmatch %}
      final _l$items = {{ method.delivery().read() }};
      _l$items.forEach(controller.add);
{%- match method.delivery().cleanup() %}
{%- when Some with (cleanup) %}
      {{ cleanup }}
{%- when None %}
{%- endmatch %}
      return _l$items.length >= batchSize;
    },
  );
  return stream.listen(callback);
{%- else %}
  return _l$context.batch<{{ method.item_type() }}>(
    (handle, batchSize, itemSize) {
      {{ method.delivery().setup() }}
      if (!({{ method.delivery().has_items() }})) {
{%- match method.delivery().cleanup() %}
{%- when Some with (cleanup) %}
        {{ cleanup }}
{%- when None %}
{%- endmatch %}
        return [];
      }
{%- match method.delivery().prepare() %}
{%- when Some with (prepare) %}
      {{ prepare }}
{%- when None %}
{%- endmatch %}
      final items = {{ method.delivery().read() }};
{%- match method.delivery().cleanup() %}
{%- when Some with (cleanup) %}
      {{ cleanup }}
{%- when None %}
{%- endmatch %}
      return items;
    },
  );
{%- endif %}
}
