{% if let Some(doc) = call.doc() %}{{ doc }}
{% endif %}    public {{ call.returns() }} {{ call.name() }}({% for parameter in call.parameters() %}{{ parameter.ty() }} {{ parameter.name() }}{% if !loop.last %}, {% endif %}{% endfor %}) {
{% if call.async_call().is_some() %}{% include "target/java/call/asynchronous.java" %}{% else %}{% for statement in call.body() %}        {{ statement }}
{% endfor %}{% endif %}    }
