package {{ package }};

import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;

{% if let Some(doc) = class.doc() %}{{ doc }}
{% endif %}public final class {{ class.name() }} implements AutoCloseable {
    private final {{ class.handle() }} handle;
    private final AtomicLong calls = new AtomicLong(1L);
    private final AtomicBoolean closed = new AtomicBoolean(false);

    {{ class.name() }}({{ class.handle() }} handle) {
        this.handle = handle;
    }{% for constant in class.constants() %}
{% include "target/java/constant.java" %}
{% endfor %}
{% for constructor in class.constructors() %}
{% if let Some(doc) = constructor.call().doc() %}{{ doc }}
{% endif %}    public {{ class.name() }}({% for parameter in constructor.call().parameters() %}{{ parameter.ty() }} {{ parameter.name() }}{% if !loop.last %}, {% endif %}{% endfor %}) {
        this({{ class.name() }}.{{ constructor.call().name() }}({{ constructor.arguments() }}));
    }

    private static {{ constructor.call().returns() }} {{ constructor.call().name() }}({% for parameter in constructor.call().parameters() %}{{ parameter.ty() }} {{ parameter.name() }}{% if !loop.last %}, {% endif %}{% endfor %}) {
{% for statement in constructor.call().body() %}        {{ statement }}
{% endfor %}    }
{% endfor %}
    {{ class.handle() }} rawHandle() {
        if (closed.get()) throw new IllegalStateException("{{ class.name() }} is closed");
        return handle;
    }

    {{ class.handle() }} boltffiRetain() {
        while (true) {
            if (closed.get()) throw new IllegalStateException("{{ class.name() }} is closed");
            long calls = this.calls.get();
            if (calls == 0L) throw new IllegalStateException("{{ class.name() }} is closed");
            if (this.calls.compareAndSet(calls, calls + 1L)) return handle;
        }
    }

    void boltffiRelease() {
        if (calls.decrementAndGet() == 0L) {
            {{ class.release() }}
        }
    }

    @Override
    public void close() {
        if (!closed.compareAndSet(false, true)) return;
        boltffiRelease();
    }
{% for call in class.factories() %}
{% include "target/java/call/initializer.java" %}
{% endfor %}{% for call in class.static_methods() %}
{% include "target/java/call/static_method.java" %}
{% endfor %}{% for call in class.instance_methods() %}
{% include "target/java/call/instance_method.java" %}
{% endfor %}{% for stream in class.streams() %}
{% include "target/java/stream.java" %}
{% endfor %}}
