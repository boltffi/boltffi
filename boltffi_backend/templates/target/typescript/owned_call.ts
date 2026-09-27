(() => {
{% for argument in owned %}  let {{ argument.local() }} = 0;
{% endfor %}  let __boltffiEntered = false;
  try {
{% for argument in owned %}{% match argument %}{% when OwnedArgument::Class with { parameter, class, local, .. } %}    {{ local }} = {{ class }}._takeHandle({{ parameter }});
{% when OwnedArgument::Closure with { parameter, local, register, .. } %}    {{ local }} = {{ register }}({{ parameter }});
{% endmatch %}{% endfor %}{% for (name, value) in arguments %}    const {{ name }} = {{ value }};
{% endfor %}    if (typeof _exports.{{ symbol }} !== "function") throw new Error("Missing native function: {{ symbol }}");
    __boltffiEntered = true;
    return {{ invocation }};
  } finally {
    if (!__boltffiEntered) {
{% for argument in owned %}{% match argument %}{% when OwnedArgument::Class with { local, release, .. } %}      if ({{ local }} !== 0) (_exports.{{ release }} as Function)({{ local }});
{% when OwnedArgument::Closure with { local, unregister, .. } %}      if ({{ local }} !== 0) {{ unregister }}({{ local }});
{% endmatch %}{% endfor %}    }
  }
})()
