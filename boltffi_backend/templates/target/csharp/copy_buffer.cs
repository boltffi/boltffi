        [DllImport(LibName, EntryPoint = {{ entry_point }})]
        internal static extern FfiBuf BufFromBytes([In] byte[] bytes, nuint length);
        [DllImport(LibName, EntryPoint = "boltffi_callback_error")]
        internal static extern FfiBuf CallbackError([In] byte[] message, nuint length);
