package {{ package }};
{%- for import in imports %}

{{ import }}
{%- endfor %}

final class {{ native_owner }} {
{{ loader }}

    private {{ native_owner }}() {}
{%- for method in native_methods %}

{{ method }}
{%- endfor %}
}
{%- for helper in helpers %}

{{ helper }}
{%- endfor %}

public final class {{ file }} {
    private {{ file }}() {}
{%- for declaration in declarations %}

{{ declaration }}
{%- endfor %}
}
