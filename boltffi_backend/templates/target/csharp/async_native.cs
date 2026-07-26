        [DllImport(LibName, EntryPoint = {{ asynchronous.poll_entry }})]
        internal static extern void {{ asynchronous.poll_name }}(nint future, ulong callbackData, BoltFFIAsync.RustFutureContinuationCallback callback);

        [DllImport(LibName, EntryPoint = {{ asynchronous.complete_entry }})]
{% if asynchronous.complete_return_marshal_i1 %}        [return: MarshalAs(UnmanagedType.I1)]
{% endif %}        internal static extern {{ asynchronous.complete_return_type }} {{ asynchronous.complete_name }}(nint future, out FfiStatus status{% for parameter in asynchronous.complete_parameters %}, {% if parameter.marshal_bool_array %}[MarshalAs(UnmanagedType.LPArray, ArraySubType = UnmanagedType.U1)] {% else if parameter.marshal_i1 %}[MarshalAs(UnmanagedType.I1)] {% endif %}{% if parameter.byte_array %}[In{% if parameter.array_out %}, Out{% endif %}] {% endif %}{{ parameter.modifier }}{{ parameter.ty }} {{ parameter.name }}{% endfor %});

        [DllImport(LibName, EntryPoint = {{ asynchronous.cancel_entry }})]
        internal static extern void {{ asynchronous.cancel_name }}(nint future);

        [DllImport(LibName, EntryPoint = {{ asynchronous.free_entry }})]
        internal static extern void {{ asynchronous.free_name }}(nint future);
