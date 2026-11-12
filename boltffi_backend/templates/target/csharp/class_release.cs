        [DllImport(LibName, EntryPoint = {{ class.release_entry }}, CallingConvention = global::System.Runtime.InteropServices.CallingConvention.Cdecl)]
        internal static extern void {{ class.release_name }}({{ class.carrier_type }} handle);
