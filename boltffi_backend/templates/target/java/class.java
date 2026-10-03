package {{ package }};

{% if let Some(doc) = class.doc() %}{{ doc }}
{% endif %}public final class {{ class.name() }} implements AutoCloseable {
    private final java.util.concurrent.atomic.AtomicLong handle;

    private {{ class.name() }}(java.util.concurrent.atomic.AtomicLong handle) {
        if (handle.get() == 0) throw new IllegalArgumentException("{{ class.name() }} handle must not be zero");
        this.handle = handle;
    }

    static {{ class.name() }} __boltffiFromHandle({{ class.handle() }} handle) {
        return new {{ class.name() }}(new java.util.concurrent.atomic.AtomicLong(handle));
    }{% for constant in class.constants() %}
{% include "target/java/constant.java" %}
{% endfor %}
{% for constructor in class.constructors() %}
{% if let Some(doc) = constructor.call().doc() %}{{ doc }}
{% endif %}    public {{ class.name() }}({% for parameter in constructor.call().parameters() %}{{ parameter.ty() }} {{ parameter.name() }}{% if !loop.last %}, {% endif %}{% endfor %}) {
        this(new java.util.concurrent.atomic.AtomicLong({{ class.name() }}.{{ constructor.call().name() }}({{ constructor.arguments() }})));
    }
{% for overload in constructor.call().overloads() %}
    public {{ class.name() }}({% for parameter in overload.parameters() %}{{ parameter.ty() }} {{ parameter.name() }}{% if !loop.last %}, {% endif %}{% endfor %}) {
        this({{ overload.arguments() }});
    }
{% endfor %}

    private static {{ constructor.call().returns() }} {{ constructor.call().name() }}({% for parameter in constructor.call().parameters() %}{{ parameter.ty() }} {{ parameter.name() }}{% if !loop.last %}, {% endif %}{% endfor %}) {
{% for statement in constructor.call().body() %}        {{ statement }}
{% endfor %}    }
{% endfor %}
    {{ class.handle() }} __boltffiTakeHandle() {
        {{ class.handle() }} value = handle.getAndSet(0);
        if (value == 0) throw new IllegalStateException("{{ class.name() }} is closed");
        return value;
    }

    {{ class.handle() }} rawHandle() {
        {{ class.handle() }} value = handle.get();
        if (value == 0) throw new IllegalStateException("{{ class.name() }} is closed");
        return value;
    }

    @Override
    public void close() {
        {{ class.handle() }} __boltffi_handle = handle.getAndSet(0);
        if (__boltffi_handle == 0) return;
        {{ class.release() }}
    }
{% for call in class.factories() %}
{% include "target/java/call/static_method.java" %}
{% endfor %}{% for call in class.static_methods() %}
{% include "target/java/call/static_method.java" %}
{% endfor %}{% for call in class.instance_methods() %}
{% include "target/java/call/instance_method.java" %}
{% endfor %}{% for stream in class.streams() %}
{% include "target/java/stream.java" %}
{% endfor %}}
