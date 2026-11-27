        [DllImport(LibName, EntryPoint = {{ entry_point }}, CallingConvention = global::System.Runtime.InteropServices.CallingConvention.Cdecl)]
        internal static extern void FreeBuf(FfiBuf buffer);
