({
{% for argument in arguments %}    var {{ argument.local }} = {{ argument.parameter }}{% if argument.presence == HandlePresence::Nullable %}?.__boltffiTakeHandle() ?? 0{% else %}.__boltffiTakeHandle(){% endif %}
    defer { if {{ argument.local }} != 0 { {{ argument.release }}({{ argument.local }}) } }
{% endfor %}    let __boltffiCallResult = {{ invocation }}
{% for argument in arguments %}    {{ argument.local }} = 0
{% endfor %}    return __boltffiCallResult
})()
