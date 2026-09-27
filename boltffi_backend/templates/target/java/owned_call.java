{{ native_owner }}.ensureInitialized();
{% for argument in owned %}long {{ argument.local }} = 0;
{% endfor %}boolean __boltffiEntered = false;
try {
{% for argument in owned %}    {{ argument.local }} = {% if argument.presence == HandlePresence::Nullable %}{{ argument.parameter }} == null ? 0 : {% endif %}{{ argument.parameter }}.__boltffiTakeHandle();
{% endfor %}{% for statement in bindings %}    {{ statement }}
{% endfor %}    __boltffiEntered = true;
    try {
{% for statement in body %}        {{ statement }}
{% endfor %}    } catch (UnsatisfiedLinkError __boltffiFailure) {
        __boltffiEntered = false;
        throw __boltffiFailure;
    }
} finally {
    if (!__boltffiEntered) {
{% for argument in owned %}        if ({{ argument.local }} != 0) {{ native_owner }}.{{ argument.release }}({{ argument.local }});
{% endfor %}    }
}
