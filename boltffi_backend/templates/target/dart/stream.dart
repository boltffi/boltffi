{%- match stream.owner() %}
{%- when Some with (owner) %}
extension {{ owner }}${{ stream.method_name() }} on {{ owner }} {
{{ stream.associated_method() }}
}
{%- when None %}
{{ stream.method() }}
{%- endmatch %}
