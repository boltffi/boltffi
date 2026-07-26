{% if let Some(doc) = constant.doc() %}{{ doc }}
{% endif %}    public static final {{ constant.ty() }} {{ constant.name() }} = {% if let Some(value) = constant.value() %}{{ value }}{% else if let Some(accessor) = constant.accessor() %}{{ accessor.name() }}(){% endif %};
{% if let Some(accessor) = constant.accessor() %}
    private static {{ accessor.returns() }} {{ accessor.name() }}() {
{% for statement in accessor.body() %}        {{ statement }}
{% endfor %}    }
{% endif %}
