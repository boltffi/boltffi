({
    Native.ensureInitialized()
{% for argument in owned %}    var {{ argument.local }} = 0L
{% endfor %}    var __boltffiEntered = false
    try {
{% for argument in owned %}        {{ argument.local }} = {{ argument.parameter }}{% if argument.presence == HandlePresence::Nullable %}?.__boltffiTakeHandle() ?: 0L{% else %}.__boltffiTakeHandle(){% endif %}
{% endfor %}{% for (name, value) in arguments %}        val {{ name }} = {{ value }}
{% endfor %}        try {
            val __boltffiResult = {{ invocation }}
            __boltffiEntered = true
            __boltffiResult
        } catch (__boltffiFailure: UnsatisfiedLinkError) {
            throw __boltffiFailure
        } catch (__boltffiFailure: Throwable) {
            __boltffiEntered = true
            throw __boltffiFailure
        }
    } finally {
        if (!__boltffiEntered) {
{% for argument in owned %}            if ({{ argument.local }} != 0L) Native.{{ argument.release }}({{ argument.local }})
{% endfor %}        }
    }
})
