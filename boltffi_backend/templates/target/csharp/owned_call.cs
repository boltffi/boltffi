{% if asynchronous %}{
{% endif %}{% for argument in arguments %}{% if asynchronous %}        {% endif %}{% match argument %}{% when OwnedArgument::Class with { parameter, class, local, presence } %}using var {{ local }} = new {{ class }}.__OwnedHandle({{ parameter }}{% if *presence == HandlePresence::Nullable %}?.TakeHandle() ?? 0{% else %}.TakeHandle(){% endif %});{% when OwnedArgument::Closure with { parameter, local } %}using var {{ local }} = new BoltFFIOwnedClosure({{ parameter }});{% endmatch %}
{% endfor %}{% if asynchronous %}        {% endif %}{% if returns_value %}var __boltffiCallResult = {% endif %}{{ invocation }};
{% for argument in arguments %}{% if asynchronous %}        {% endif %}{{ argument.local() }}.Commit();
{% endfor %}{% if asynchronous %}        return __boltffiCallResult;
    }{% endif %}
