{% if let Some(asynchronous) = call.async_call() %}        return BoltFfiAsync.call(
            () -> {
{% for statement in asynchronous.create_body() %}                {{ statement }}
{% endfor %}            },
            (future, continuation) -> {{ asynchronous.poll() }},
            (future) -> {
{% for statement in asynchronous.complete() %}                {{ statement }}
{% endfor %}            },
            (future) -> {{ asynchronous.cancel() }},
            (future) -> {{ asynchronous.free() }}
        );
{% endif %}