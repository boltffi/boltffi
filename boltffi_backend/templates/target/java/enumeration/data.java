package {{ package }};

{% if let Some(doc) = enumeration.doc() %}{{ doc }}
{% endif %}{% if enumeration.flat_error() %}public enum {{ enumeration.name() }} {
{% for variant in enumeration.variants() %}    {{ variant.name() }}{% if !loop.last %},{% else %};{% endif %}
{% endfor %}
    private static final {{ enumeration.name() }}[] VALUES = values();

    static {{ enumeration.name() }} fromReader(WireReader reader) {
        int tag = reader.readInt();
        if (tag < 0 || tag >= VALUES.length) {
            throw new IllegalArgumentException("Unknown {{ enumeration.name() }} tag: " + tag);
        }
        return VALUES[tag];
    }

    int wireSize() {
        return 4;
    }

    void writeTo(WireWriter writer) {
        writer.writeInt(ordinal());
    }

    byte[] toByteArray() {
        WireLease lease = WireWriterPool.acquire(4);
        try {
            writeTo(lease.writer());
            return lease.bytes();
        } finally {
            lease.close();
        }
    }

    public static final class Exception extends RuntimeException {
        private final {{ enumeration.name() }} error;

        Exception({{ enumeration.name() }} error) {
            super(error.name());
            this.error = error;
        }

        public {{ enumeration.name() }} getError() {
            return error;
        }
    }
{% else if enumeration.sealed() %}public sealed interface {{ enumeration.name() }} permits {% for variant in enumeration.variants() %}{{ enumeration.name() }}.{{ variant.name() }}{% if !loop.last %}, {% endif %}{% endfor %} {
{% for variant in enumeration.variants() %}
{% if let Some(doc) = variant.doc() %}{{ doc }}
{% endif %}    record {{ variant.name() }}({% for field in variant.fields() %}{{ field.ty() }} {{ field.name() }}{% if !loop.last %}, {% endif %}{% endfor %}) implements {{ enumeration.name() }} {
        public int wireSize() {
            return {{ variant.size() }};
        }

        public void writeTo(WireWriter writer) {
            {{ variant.tag_write() }}
{% for field in variant.fields() %}{% for statement in field.wire_write() %}            {{ statement }}
{% endfor %}{% endfor %}        }
    }
{% endfor %}
    int wireSize();

    void writeTo(WireWriter writer);

    default byte[] toByteArray() {
        WireLease lease = WireWriterPool.acquire(wireSize());
        try {
            writeTo(lease.writer());
            return lease.bytes();
        } finally {
            lease.close();
        }
    }

    static {{ enumeration.name() }} fromReader(WireReader reader) {
        int tag = reader.readInt();
        switch (tag) {
{% for variant in enumeration.variants() %}            case {{ variant.tag() }}: return new {{ variant.name() }}({% for field in variant.fields() %}{{ field.wire_read() }}{% if !loop.last %}, {% endif %}{% endfor %});
{% endfor %}            default: throw new IllegalArgumentException("Unknown {{ enumeration.name() }} tag: " + tag);
        }
    }
{% else %}public abstract class {{ enumeration.name() }}{% if enumeration.error() %} extends RuntimeException{% endif %} {
{% if enumeration.error() %}    protected {{ enumeration.name() }}(String message) {
        super(message);
    }
{% else %}    private {{ enumeration.name() }}() {}
{% endif %}{% for variant in enumeration.variants() %}
{% if let Some(doc) = variant.doc() %}{{ doc }}
{% endif %}    public static final class {{ variant.name() }} extends {{ enumeration.name() }} {
{% for field in variant.fields() %}        public final {{ field.ty() }} {{ field.name() }};
{% endfor %}{% if variant.unit() %}        public static final {{ variant.name() }} INSTANCE = new {{ variant.name() }}();

        private {{ variant.name() }}() {
{% if enumeration.error() %}            super("{{ enumeration.name() }}.{{ variant.name() }}");
{% endif %}        }
{% else %}        public {{ variant.name() }}({% for field in variant.fields() %}{{ field.ty() }} {{ field.name() }}{% if !loop.last %}, {% endif %}{% endfor %}) {
{% if enumeration.error() %}{% if let Some(message) = variant.message_field() %}            super({{ message }});
{% else %}            super("{{ enumeration.name() }}.{{ variant.name() }}");
{% endif %}{% endif %}{% for field in variant.fields() %}            this.{{ field.name() }} = {{ field.name() }};
{% endfor %}        }
{% endif %}
        @Override
        int wireSize() {
            return {{ variant.size() }};
        }

        @Override
        void writeTo(WireWriter writer) {
            {{ variant.tag_write() }}
{% for field in variant.fields() %}{% for statement in field.wire_write() %}            {{ statement }}
{% endfor %}{% endfor %}        }

        @Override
        public boolean equals(Object value) {
{% if variant.unit() %}            return value instanceof {{ variant.name() }};
{% else %}            if (this == value) return true;
            if (!(value instanceof {{ variant.name() }})) return false;
            {{ variant.name() }} other = ({{ variant.name() }}) value;
            return {% for field in variant.fields() %}{{ field.equals() }}{% if !loop.last %} && {% endif %}{% endfor %};
{% endif %}        }

        @Override
        public int hashCode() {
            int result = {{ variant.tag() }};
{% for field in variant.fields() %}            result = 31 * result + {{ field.hash() }};
{% endfor %}            return result;
        }
    }
{% endfor %}
    abstract int wireSize();

    abstract void writeTo(WireWriter writer);

    byte[] toByteArray() {
        WireLease lease = WireWriterPool.acquire(wireSize());
        try {
            writeTo(lease.writer());
            return lease.bytes();
        } finally {
            lease.close();
        }
    }

    static {{ enumeration.name() }} fromReader(WireReader reader) {
        int tag = reader.readInt();
        switch (tag) {
{% for variant in enumeration.variants() %}            case {{ variant.tag() }}: return {% if variant.unit() %}{{ variant.name() }}.INSTANCE{% else %}new {{ variant.name() }}({% for field in variant.fields() %}{{ field.wire_read() }}{% if !loop.last %}, {% endif %}{% endfor %}){% endif %};
{% endfor %}            default: throw new IllegalArgumentException("Unknown {{ enumeration.name() }} tag: " + tag);
        }
    }
{% endif %}
    static {{ enumeration.name() }} fromByteArray(byte[] bytes) {
        return fromReader(new WireReader(bytes));
    }
{% for call in enumeration.calls().initializers() %}
{% include "target/java/call/initializer.java" %}
{% endfor %}{% for call in enumeration.calls().static_methods() %}
{% include "target/java/call/static_method.java" %}
{% endfor %}{% for call in enumeration.calls().instance_methods() %}
{% if enumeration.sealed() %}{% include "target/java/call/interface_instance_method.java" %}{% else %}{% include "target/java/call/instance_method.java" %}{% endif %}
{% endfor %}}
