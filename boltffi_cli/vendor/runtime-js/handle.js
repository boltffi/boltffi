/**
 * Base class for generated class bindings.
 *
 * Every generated class needs the same handle lifecycle: hold a wasm handle,
 * borrow it for a call, refuse to be used after disposal, and release exactly
 * once. Emitting that per class costs roughly 300 bytes of identical glue each
 * time, so it lives here instead. Generated classes supply only what differs:
 * their name, their finalizer, and how to release the handle.
 */
export class BoltFFIHandle {
    constructor(handle) {
        this._disposed = false;
        this._handle = handle;
    }
    /** Releases the wasm-side handle. Generated classes override it. */
    _release(_handle) { }
    /** Unregisters from the finalizer, if the runtime has one. */
    _unregister() { }
    dispose() {
        if (this._disposed) {
            return;
        }
        this._disposed = true;
        this._unregister();
        this._release(this._handle);
        this._handle = 0;
    }
    /**
     * Handle to pass across the boundary, refusing a disposed instance so a
     * use-after-free surfaces as an error rather than a wasm trap.
     */
    _borrowHandle() {
        this._assertNotDisposed();
        return this._handle;
    }
    _assertNotDisposed() {
        if (this._disposed) {
            const name = this.constructor._typeName;
            throw new Error(`${name} has been disposed`);
        }
    }
    /** Null maps to the zero handle, matching the wasm-side convention. */
    static _toHandle(value) {
        return value === null ? 0 : value._borrowHandle();
    }
}
/** Type name used in error messages. Generated classes override it. */
BoltFFIHandle._typeName = "BoltFFIHandle";
//# sourceMappingURL=handle.js.map