    static native {{ method.returns() }} {{ method.name() }}({% for parameter in method.parameters() %}{{ parameter.ty() }} {{ parameter.name() }}{% if !loop.last %}, {% endif %}{% endfor %});
