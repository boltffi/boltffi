    internal ref struct BoltFFIOwnedClosure
    {
        private global::System.Runtime.InteropServices.GCHandle handle;

        internal BoltFFIOwnedClosure(global::System.Delegate callback)
        {
            handle = global::System.Runtime.InteropServices.GCHandle.Alloc(callback);
        }

        internal nint Handle => global::System.Runtime.InteropServices.GCHandle.ToIntPtr(handle);

        internal void Commit() => handle = default;

        public void Dispose()
        {
            if (handle.IsAllocated) handle.Free();
        }
    }
