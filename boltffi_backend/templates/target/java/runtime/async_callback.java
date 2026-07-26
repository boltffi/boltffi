    static void boltffiFutureContinuationCallback(long handle, byte pollResult) {
        BoltFfiAsync.resume(handle, pollResult);
    }
