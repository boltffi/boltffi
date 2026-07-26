package {{ package }};

{% if let Some(doc) = enumeration.doc() %}{{ doc }}
{% endif %}public enum {{ enumeration.name() }} {
{% for variant in enumeration.variants() %}    {{ variant.name() }}({{ variant.value() }}){% if !loop.last %},{% else %};{% endif %}
{% endfor %}
    public final {{ enumeration.value_type() }} value;

    {{ enumeration.name() }}({{ enumeration.value_type() }} value) {
        this.value = value;
    }

    public static {{ enumeration.name() }} fromValue({{ enumeration.value_type() }} value) {
{% if enumeration.long_value() %}{% for variant in enumeration.variants() %}        if (value == {{ variant.value() }}) return {{ variant.name() }};
{% endfor %}{% else %}        switch (value) {
{% for variant in enumeration.variants() %}            case {{ variant.value() }}: return {{ variant.name() }};
{% endfor %}            default: break;
        }
{% endif %}        throw new IllegalArgumentException("Unknown {{ enumeration.name() }} value: " + value);
    }

    {{ enumeration.value_type() }} nativeValue() {
        return value;
    }
{% if enumeration.error() %}
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
{% endif %}{% for call in enumeration.calls().initializers() %}
{% include "target/java/call/initializer.java" %}
{% endfor %}{% for call in enumeration.calls().static_methods() %}
{% include "target/java/call/static_method.java" %}
{% endfor %}{% for call in enumeration.calls().instance_methods() %}
{% include "target/java/call/instance_method.java" %}
{% endfor %}}
