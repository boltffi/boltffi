{% if custom_type.has_representation_alias() %}
{{ custom_type.documentation() }}public typealias {{ custom_type.name() }} = {{ custom_type.representation() }}
{% endif %}
