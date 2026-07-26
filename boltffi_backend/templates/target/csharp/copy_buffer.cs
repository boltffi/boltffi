        [DllImport(LibName, EntryPoint = {{ entry_point }})]
        internal static extern FfiBuf BufFromBytes([In] byte[] bytes, nuint length);
