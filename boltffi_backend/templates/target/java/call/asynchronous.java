{% if let Some(asynchronous) = call.async_call() %}        return BoltFfiAsync.call(
            () -> {
{% for statement in asynchronous.create_acquire() %}                {{ statement }}
{% endfor %}{% if asynchronous.has_create_cleanup() %}                try {
{% for statement in asynchronous.create_prepare() %}                    {{ statement }}
{% endfor %}                    return {{ asynchronous.create() }};
                } finally {
{% for statement in asynchronous.create_cleanup() %}                    {{ statement }}
{% endfor %}                }
{% else %}{% for statement in asynchronous.create_prepare() %}                {{ statement }}
{% endfor %}                return {{ asynchronous.create() }};
{% endif %}            },
            (future, continuation) -> {{ asynchronous.poll() }},
            (future) -> {
{% for statement in asynchronous.complete() %}                {{ statement }}
{% endfor %}            },
            (future) -> {{ asynchronous.cancel() }},
            (future) -> {{ asynchronous.free() }}
        );
{% endif %}
