        [DllImport(LibName, EntryPoint = {{ class.release_entry }})]
        internal static extern void {{ class.release_name }}({{ class.carrier_type }} handle);
