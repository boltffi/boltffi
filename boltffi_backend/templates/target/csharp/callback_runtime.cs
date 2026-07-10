    [StructLayout(LayoutKind.Sequential)]
    internal struct BoltFFICallbackHandle
    {
        internal ulong handle;
        internal nint vtable;

        internal static BoltFFICallbackHandle Null => default;
        internal bool IsNull => handle == 0 || vtable == 0;
    }
