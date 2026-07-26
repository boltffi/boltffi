package {{ package }};

import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;

{% if let Some(doc) = callback.doc() %}{{ doc }}
{% endif %}public interface {{ callback.name() }} {
{% for method in callback.methods() %}{% if let Some(doc) = method.doc() %}{{ doc }}
{% endif %}    {{ method.public_return() }} {{ method.name() }}({% for parameter in method.public_parameters() %}{{ parameter.ty() }} {{ parameter.name() }}{% if !loop.last %}, {% endif %}{% endfor %});
{% endfor %}}

final class {{ callback.callbacks_name() }} {
    private static final ConcurrentHashMap<Long, {{ callback.name() }}> VALUES = new ConcurrentHashMap<>();
    private static final AtomicLong NEXT = new AtomicLong(1L);

    private {{ callback.callbacks_name() }}() {}

    static long insert({{ callback.name() }} value) {
        long handle = NEXT.getAndAdd(2L);
        VALUES.put(handle, value);
        return handle;
    }

    static {{ callback.name() }} get(long handle) {
        return VALUES.get(handle);
    }

    static void free(long handle) {
        VALUES.remove(handle);
    }

    static long clone(long handle) {
        {{ callback.name() }} value = VALUES.get(handle);
        return value == null ? 0L : insert(value);
    }
{% for method in callback.methods() %}
    static {{ method.jvm_return() }} {{ method.jvm_name() }}(long handle{% for parameter in method.jvm_parameters() %}, {{ parameter.ty() }} {{ parameter.name() }}{% endfor %}) {
        {{ callback.name() }} implementation = VALUES.get(handle);
        if (implementation == null) throw new IllegalStateException("invalid callback handle");
{% for statement in method.setup() %}        {{ statement }}
{% endfor %}{% if let Some(asynchronous) = method.asynchronous() %}        {{ method.public_return() }} __boltffi_future;
        try {
            __boltffi_future = {{ asynchronous.call() }};
        } catch (Throwable __boltffi_failure) {
            __boltffi_future = BoltFfiCallbackFailure.failed(__boltffi_failure);
        }
        if (__boltffi_future == null) {
            {{ asynchronous.failure() }}
            return;
        }
        __boltffi_future.whenComplete((__boltffi_result, __boltffi_failure) -> {
            if (__boltffi_failure != null) {
                Throwable __boltffi_cause = BoltFfiCallbackFailure.unwrap(__boltffi_failure);
{% if let Some(error_type) = asynchronous.error_type() %}                if (__boltffi_cause instanceof {{ error_type }}) {
                    {{ error_type }} __boltffi_error = ({{ error_type }}) __boltffi_cause;
                    try {
{% for statement in asynchronous.error() %}                        {{ statement }}
{% endfor %}                    } catch (Throwable __boltffi_completion_failure) {
                        {{ asynchronous.failure() }}
                    }
                    return;
                }
{% endif %}                {{ asynchronous.failure() }}
                return;
            }
            try {
{% for statement in asynchronous.success() %}                {{ statement }}
{% endfor %}            } catch (Throwable __boltffi_completion_failure) {
                {{ asynchronous.failure() }}
            }
        });
{% else %}{% for statement in method.body() %}        {{ statement }}
{% endfor %}{% endif %}    }
{% endfor %}{% for method in callback.handle_methods() %}{% if let Some(asynchronous) = method.asynchronous() %}
    static void {{ asynchronous.success() }}(long callbackData{% if let Some(result) = asynchronous.result() %}, {{ result.ty() }} {{ result.name() }}{% endif %}) {
{% for statement in asynchronous.completion() %}        {{ statement }}
{% endfor %}    }

    static void {{ asynchronous.failure() }}(long callbackData) {
        BoltFfiCallbackFutures.failure(callbackData, new RuntimeException("callback failed"));
    }
{% endif %}{% endfor %}}
{% if !callback.handle_methods().is_empty() %}
final class {{ callback.handle_name() }} implements {{ callback.name() }}, AutoCloseable {
    private final long handle;
    private final AtomicBoolean closed = new AtomicBoolean(false);

    {{ callback.handle_name() }}(long handle) {
        this.handle = handle;
    }

    long rawHandle() {
        if (closed.get()) throw new IllegalStateException("callback handle is closed");
        return handle;
    }

    @Override
    public void close() {
        if (!closed.compareAndSet(false, true)) return;
{% if let Some(release) = callback.handle_release() %}        Native.{{ release }}(handle);
{% endif %}    }
{% for method in callback.handle_methods() %}
    @Override
    public {{ method.returns() }} {{ method.name() }}({% for parameter in method.parameters() %}{{ parameter.ty() }} {{ parameter.name() }}{% if !loop.last %}, {% endif %}{% endfor %}) {
{% for statement in method.body() %}        {{ statement }}
{% endfor %}    }
{% endfor %}}
{% endif %}

final class {{ callback.bridge_name() }} {
    private {{ callback.bridge_name() }}() {}

    static long create({{ callback.name() }} value) {
{% if !callback.handle_methods().is_empty() %}        if (value instanceof {{ callback.handle_name() }}) {
            return (({{ callback.handle_name() }}) value).rawHandle();
        }
{% endif %}
        return {{ callback.callbacks_name() }}.insert(value);
    }

    static {{ callback.name() }} wrap(long handle) {
        {{ callback.name() }} existing = {{ callback.callbacks_name() }}.get(handle);
        if (existing != null) return existing;
{% if !callback.handle_methods().is_empty() %}        return new {{ callback.handle_name() }}(handle);
{% else %}        throw new IllegalStateException("callback handle has no native methods");
{% endif %}
    }
}
