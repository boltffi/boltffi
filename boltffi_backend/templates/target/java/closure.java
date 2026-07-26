package {{ package }};

import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicLong;

@FunctionalInterface
public interface {{ closure.name() }} {
    {{ closure.method().public_return() }} invoke({% for parameter in closure.method().public_parameters() %}{{ parameter.ty() }} {{ parameter.name() }}{% if !loop.last %}, {% endif %}{% endfor %});
}

final class {{ closure.map_name() }} {
    private static final ConcurrentHashMap<Long, {{ closure.name() }}> VALUES = new ConcurrentHashMap<>();
    private static final AtomicLong NEXT = new AtomicLong(1L);

    private {{ closure.map_name() }}() {}

    static long insert({{ closure.name() }} value) {
        long handle = NEXT.getAndAdd(2L);
        VALUES.put(handle, value);
        return handle;
    }

    static {{ closure.name() }} get(long handle) {
        return VALUES.get(handle);
    }

    static void remove(long handle) {
        VALUES.remove(handle);
    }
}

final class {{ closure.callbacks_name() }} {
    private {{ closure.callbacks_name() }}() {}

    static long insert({{ closure.name() }} value) {
        return {{ closure.map_name() }}.insert(value);
    }

    static void free(long handle) {
        {{ closure.map_name() }}.remove(handle);
    }

    static {{ closure.method().jvm_return() }} call(long handle{% for parameter in closure.method().jvm_parameters() %}, {{ parameter.ty() }} {{ parameter.name() }}{% endfor %}) {
        {{ closure.name() }} implementation = {{ closure.map_name() }}.get(handle);
        if (implementation == null) throw new IllegalStateException("invalid closure handle");
{% for statement in closure.method().setup() %}        {{ statement }}
{% endfor %}{% for statement in closure.method().body() %}        {{ statement }}
{% endfor %}    }
}
