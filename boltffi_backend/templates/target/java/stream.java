{% if let Some(doc) = stream.doc() %}{{ doc }}
{% endif %}{% if stream.callback_delivery() %}    public StreamSubscription<{{ stream.item_type() }}> {{ stream.name() }}(java.util.function.Consumer<{{ stream.item_type() }}> callback) {
{% for statement in stream.subscribe_statements() %}        {{ statement }}
{% endfor %}        return BoltFfiStream.callback(
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
            callback
        );
    }
{% else %}    public StreamSubscription<{{ stream.item_type() }}> {{ stream.name() }}() {
{% for statement in stream.subscribe_statements() %}        {{ statement }}
{% endfor %}        return StreamSubscription.batch(
            subscription,
            (streamHandle, maxCount) -> {
                byte[] bytes = {{ stream.pop_batch() }};
                if (bytes == null) throw new IllegalStateException("BoltFFI stream pop_batch returned null");
                if (bytes.length == 0) return java.util.Collections.emptyList();
{% for statement in stream.item_setup() %}                {{ statement }}
{% endfor %}                return {{ stream.items() }};
            },
            (streamHandle, timeout) -> {{ stream.wait() }},
            (streamHandle) -> {{ stream.unsubscribe() }},
            (streamHandle) -> {{ stream.free() }}
        );
    }
{% endif %}
