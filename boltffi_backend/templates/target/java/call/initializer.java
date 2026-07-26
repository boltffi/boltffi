{% if let Some(doc) = call.doc() %}{{ doc }}
{% endif %}    public static {{ call.returns() }} {{ call.name() }}({% for parameter in call.parameters() %}{{ parameter.ty() }} {{ parameter.name() }}{% if !loop.last %}, {% endif %}{% endfor %}) {
{% for statement in call.body() %}        {{ statement }}
{% endfor %}    }
