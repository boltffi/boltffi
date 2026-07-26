{{ constant.documentation() }}{% if constant.inline() %}    public static let {{ constant.name() }}: {{ constant.ty() }} = {{ constant.value() }}{% endif %}{% if constant.accessor() %}    public static var {{ constant.name() }}: {{ constant.ty() }} {
{{ constant.body() }}
    }{% endif %}
