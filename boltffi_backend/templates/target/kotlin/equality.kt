override fun equals(other: kotlin.Any?): kotlin.Boolean {
    if (this === other) return true
    if (other !is {{ equality.owner() }}) return false
    return {% for term in equality.equals() %}{{ term }}{% if !loop.last %} &&
        {% endif %}{% endfor %}
}

{% if equality.hash_codes().len() == 1 -%}
override fun hashCode(): kotlin.Int = {{ equality.hash_codes()[0] }}
{%- else -%}
override fun hashCode(): kotlin.Int {
{%- for hash_code in equality.hash_codes() %}
{%- if loop.first %}
    var result = {{ hash_code }}
{%- else %}
    result = 31 * result + {{ hash_code }}
{%- endif %}
{%- endfor %}
    return result
}
{%- endif %}
