        [DllImport(LibName, EntryPoint = {{ function.entry_point }})]
{% if function.return_marshal_i1 %}        [return: MarshalAs(UnmanagedType.I1)]
{% endif %}        internal static extern {{ function.native_return_type }} {{ function.native_name }}({% for parameter in function.parameters %}{% if parameter.marshal_i1 %}[MarshalAs(UnmanagedType.I1)] {% endif %}{{ parameter.ty }} {{ parameter.name }}{% if !loop.last %}, {% endif %}{% endfor %});
