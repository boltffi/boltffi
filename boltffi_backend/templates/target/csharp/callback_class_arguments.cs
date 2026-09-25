{% for argument in classes %}            using var {{ argument.pending }} = new {{ argument.class }}.__OwnedHandle({{ argument.carrier }});
            {{ argument.class }}{% if argument.presence == HandlePresence::Nullable %}?{% endif %} {{ argument.wrapper }} = null!;
{% endfor %}            bool __boltffiClassesDelivered = false;
            try
            {
{% for argument in classes %}                {{ argument.wrapper }} = {% if argument.presence == HandlePresence::Nullable %}{{ argument.carrier }} == 0 ? null : {% endif %}new {{ argument.class }}({{ argument.carrier }});
                {{ argument.pending }}.Commit();
{% endfor %}{{ body }}
            }
            finally
            {
                if (!__boltffiClassesDelivered)
                {
{% for argument in classes %}                    {{ argument.wrapper }}?.Dispose();
{% endfor %}                }
            }
