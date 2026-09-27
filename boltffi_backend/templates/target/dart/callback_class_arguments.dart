{% for argument in classes %}var {{ argument.pending }} = {{ argument.carrier }};
{{ argument.class }}? {{ argument.wrapper }};
{% endfor %}var _l$classesDelivered = false;
try {
{% for argument in classes %}  {{ argument.wrapper }} = {% if argument.owned.presence == HandlePresence::Nullable %}{{ argument.pending }} == 0 ? null : {% endif %}{{ argument.class }}._({{ argument.pending }});
  {{ argument.pending }} = 0;
{% endfor %}{{ body }}
} finally {
  if (!_l$classesDelivered) {
{% for argument in classes %}    {{ argument.wrapper }}?.dispose$();
    if ({{ argument.pending }} != 0) _f${{ argument.owned.release }}({{ argument.pending }});
{% endfor %}  }
}
