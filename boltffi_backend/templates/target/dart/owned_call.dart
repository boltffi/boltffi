(() {
{% for argument in owned %}  var {{ argument.local }} = 0;
{% endfor %}  try {
{% for argument in owned %}    {{ argument.local }} = {{ argument.parameter }}{% if argument.presence == HandlePresence::Nullable %}?._f$takeHandle() ?? 0{% else %}._f$takeHandle(){% endif %};
{% endfor %}    {% if returns_value %}final _l$ownedCallResult = {% endif %}{{ invocation }};
{% for argument in owned %}    {{ argument.local }} = 0;
{% endfor %}{% if returns_value %}    return _l$ownedCallResult;
{% endif %}  } finally {
{% for argument in owned %}    if ({{ argument.local }} != 0) _f${{ argument.release }}({{ argument.local }});
{% endfor %}  }
})()
