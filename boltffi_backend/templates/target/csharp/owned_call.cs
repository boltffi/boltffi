{% if asynchronous %}{
{% endif %}{% for argument in arguments %}{% if asynchronous %}        {% endif %}using var {{ argument.local }} = new {{ argument.class }}.__OwnedHandle({{ argument.parameter }}{% if argument.presence == HandlePresence::Nullable %}?.TakeHandle() ?? 0{% else %}.TakeHandle(){% endif %});
{% endfor %}{% if asynchronous %}        {% endif %}var __boltffiCallResult = {{ invocation }};
{% for argument in arguments %}{% if asynchronous %}        {% endif %}{{ argument.local }}.Commit();
{% endfor %}{% if asynchronous %}        return __boltffiCallResult;
    }{% endif %}
