{{ constant.documentation() }}{% if constant.inline() %}public let {{ constant.name() }}: {{ constant.ty() }} = {{ constant.value() }}{% endif %}{% if constant.accessor() %}public var {{ constant.name() }}: {{ constant.ty() }} {
{{ constant.body() }}
}{% endif %}
