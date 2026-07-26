package {{ package }};

{% if let Some(doc) = record.doc() %}{{ doc }}
{% endif %}{% if record.native_record() %}public record {{ record.name() }}({% for field in record.fields() %}{{ field.ty() }} {{ field.name() }}{% if !loop.last %}, {% endif %}{% endfor %}) {
{% for constructor in record.default_constructors() %}
    public {{ record.name() }}({% for parameter in constructor.parameters() %}{{ parameter.ty() }} {{ parameter.name() }}{% if !loop.last %}, {% endif %}{% endfor %}) {
        this({{ constructor.arguments() }});
    }
{% endfor %}
{% else %}public final class {{ record.name() }}{% if record.error() %} extends RuntimeException{% endif %} {
{% for field in record.fields() %}    public final {{ field.ty() }} {{ field.name() }};
{% endfor %}
    public {{ record.name() }}({% for field in record.fields() %}{{ field.ty() }} {{ field.name() }}{% if !loop.last %}, {% endif %}{% endfor %}) {
{% if record.error() %}{% if let Some(message) = record.error_message() %}        super({{ message }});
{% else %}        super();
{% endif %}{% endif %}{% for field in record.fields() %}        this.{{ field.name() }} = {{ field.name() }};
{% endfor %}    }
{% for constructor in record.default_constructors() %}
    public {{ record.name() }}({% for parameter in constructor.parameters() %}{{ parameter.ty() }} {{ parameter.name() }}{% if !loop.last %}, {% endif %}{% endfor %}) {
        this({{ constructor.arguments() }});
    }
{% endfor %}
{% for field in record.fields() %}
{% if let Some(doc) = field.doc() %}{{ doc }}
{% endif %}    public {{ field.ty() }} {{ field.name() }}() {
        return {{ field.name() }};
    }
{% endfor %}
{% if record.error() %}
    public {{ record.name() }} getError() {
        return this;
    }
{% endif %}
    @Override
    public boolean equals(Object value) {
        if (this == value) return true;
        if (value == null || getClass() != value.getClass()) return false;
        {{ record.name() }} other = ({{ record.name() }}) value;
{% if record.empty() %}        return true;
{% else %}        return {% for field in record.fields() %}{{ field.equals() }}{% if !loop.last %} && {% endif %}{% endfor %};
{% endif %}    }

    @Override
    public int hashCode() {
        int result = 1;
{% for field in record.fields() %}        result = 31 * result + {{ field.hash() }};
{% endfor %}        return result;
    }

    @Override
    public String toString() {
{% if record.empty() %}        return "{{ record.name() }}{}";
{% else %}        return "{{ record.name() }}{" +
{% for field in record.fields() %}            "{% if !loop.first %}, {% endif %}{{ field.name() }}=" + {{ field.name() }}{% if !loop.last %} +{% endif %}
{% endfor %}            + '}';
{% endif %}    }
{% endif %}
{% for call in record.initializers() %}
{% include "target/java/call/initializer.java" %}
{% endfor %}{% for call in record.static_methods() %}
{% include "target/java/call/static_method.java" %}
{% endfor %}{% for call in record.instance_methods() %}
{% include "target/java/call/instance_method.java" %}
{% endfor %}{% if record.codec_payload() %}
    int wireSize() {
        return {{ record.wire_size() }};
    }

    void writeTo(WireWriter writer) {
{% for field in record.fields() %}{% for statement in field.wire_write() %}        {{ statement }}
{% endfor %}{% endfor %}    }

    byte[] toByteArray() {
        WireLease lease = WireWriterPool.acquire(wireSize());
        try {
            writeTo(lease.writer());
            return lease.bytes();
        } finally {
            lease.close();
        }
    }

    static {{ record.name() }} fromReader(WireReader reader) {
        return new {{ record.name() }}({% for field in record.fields() %}{{ field.wire_read() }}{% if !loop.last %}, {% endif %}{% endfor %});
    }
{% endif %}{% if record.direct() %}
    static final int STRUCT_SIZE = {{ record.size() }};

{% if !record.codec_payload() %}    byte[] toByteArray() {
        byte[] bytes = new byte[STRUCT_SIZE];
        writeToDirectBuffer(
            java.nio.ByteBuffer.wrap(bytes).order(java.nio.ByteOrder.nativeOrder()),
            0
        );
        return bytes;
    }

{% endif %}
    java.nio.ByteBuffer toDirectBuffer() {
        java.nio.ByteBuffer buffer = java.nio.ByteBuffer
            .allocateDirect(STRUCT_SIZE)
            .order(java.nio.ByteOrder.nativeOrder());
        writeToDirectBuffer(buffer, 0);
        return buffer;
    }

    void writeToDirectBuffer(java.nio.ByteBuffer buffer, int offset) {
{% for field in record.fields() %}{% if let Some(write) = field.direct_write() %}        {{ write }};
{% endif %}{% endfor %}    }

    static {{ record.name() }} fromDirectBuffer(java.nio.ByteBuffer buffer, int offset) {
        return new {{ record.name() }}({% for field in record.fields() %}{% if let Some(read) = field.direct_read() %}{{ read }}{% endif %}{% if !loop.last %}, {% endif %}{% endfor %});
    }

    static {{ record.name() }} fromByteArray(byte[] bytes) {
        if (bytes.length != STRUCT_SIZE) {
            throw new IllegalArgumentException("invalid {{ record.name() }} byte size");
        }
        java.nio.ByteBuffer buffer = java.nio.ByteBuffer
            .wrap(bytes)
            .order(java.nio.ByteOrder.nativeOrder());
        return fromDirectBuffer(buffer);
    }

    static {{ record.name() }} fromDirectBuffer(java.nio.ByteBuffer buffer) {
        return fromDirectBuffer(buffer, 0);
    }
{% else %}
    static {{ record.name() }} fromByteArray(byte[] bytes) {
        return fromReader(new WireReader(bytes));
    }
{% endif %}}
