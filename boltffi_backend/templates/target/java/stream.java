{% if let Some(doc) = stream.doc() %}{{ doc }}
{% endif %}{% if stream.callback_delivery() %}    public StreamSubscription<{{ stream.item_type() }}> {{ stream.name() }}(java.util.function.Consumer<{{ stream.item_type() }}> callback{% if stream.failure().is_some() %}, java.util.function.Consumer<RuntimeException> onError{% endif %}) {
        long subscription = {{ stream.subscribe() }};
        return BoltFfiStream.callback(
            subscription,
            16L,
            (streamHandle, maxCount) -> {
                byte[] bytes = {{ stream.pop_batch() }};
                if (bytes == null) throw new IllegalStateException("BoltFFI stream pop_batch returned null");
                if (bytes.length == 0) return java.util.Collections.emptyList();
{% for statement in stream.item_setup() %}                {{ statement }}
{% endfor %}                return {{ stream.items() }};
            },
            (streamHandle, continuation) -> {{ stream.poll() }},
            (streamHandle) -> {{ stream.unsubscribe() }},
            (streamHandle) -> {{ stream.free() }},
            callback{% if let Some(failure) = stream.failure() %},
            (streamHandle) -> {
                byte[] bytes = {{ failure.take_error() }};
                if (bytes == null || bytes.length == 0) return null;
                WireReader {{ failure.reader() }} = new WireReader(bytes);
                return {{ failure.thrown() }};
            },
            onError{% endif %}
        );
    }
{% else %}    public StreamSubscription<{{ stream.item_type() }}> {{ stream.name() }}() {
        return StreamSubscription.batch(
            {{ stream.subscribe() }},
            (streamHandle, maxCount) -> {
                byte[] bytes = {{ stream.pop_batch() }};
                if (bytes == null) throw new IllegalStateException("BoltFFI stream pop_batch returned null");
                if (bytes.length == 0) return java.util.Collections.emptyList();
{% for statement in stream.item_setup() %}                {{ statement }}
{% endfor %}                return {{ stream.items() }};
            },
            (streamHandle, timeout) -> {{ stream.wait() }},
            (streamHandle) -> {{ stream.unsubscribe() }},
            (streamHandle) -> {{ stream.free() }}{% if let Some(failure) = stream.failure() %},
            (streamHandle) -> {
                byte[] bytes = {{ failure.take_error() }};
                if (bytes == null || bytes.length == 0) return null;
                WireReader {{ failure.reader() }} = new WireReader(bytes);
                return {{ failure.thrown() }};
            }{% endif %}
        );
    }
{% endif %}
