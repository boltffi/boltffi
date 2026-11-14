        [DllImport(LibName, EntryPoint = {{ entry_point }}, CallingConvention = global::System.Runtime.InteropServices.CallingConvention.Cdecl)]
        internal static extern FfiBuf BufFromBytes([In] byte[] bytes, nuint length);
        [DllImport(LibName, EntryPoint = "boltffi_callback_error", CallingConvention = global::System.Runtime.InteropServices.CallingConvention.Cdecl)]
        internal static extern FfiBuf CallbackError([In] byte[] message, nuint length);
