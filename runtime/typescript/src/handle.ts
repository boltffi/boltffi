/**
 * Base class for generated class bindings.
 *
 * Every generated class needs the same handle lifecycle: hold a wasm handle,
 * borrow it for a call, refuse to be used after disposal, and release exactly
 * once. Emitting that per class costs roughly 300 bytes of identical glue each
 * time, so it lives here instead. Generated classes supply only what differs:
 * their name, their finalizer, and how to release the handle.
 */
export abstract class BoltFFIHandle {
  /** Zero once disposed. */
  protected _handle: number;
  protected _disposed = false;

  protected constructor(handle: number) {
    this._handle = handle;
  }

  /** Type name used in error messages. Generated classes override it. */
  protected static _typeName = "BoltFFIHandle";

  /** Releases the wasm-side handle. Generated classes override it. */
  protected _release(_handle: number): void {}

  /** Unregisters from the finalizer, if the runtime has one. */
  protected _unregister(): void {}

  dispose(): void {
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
  protected _borrowHandle(): number {
    this._assertNotDisposed();
    return this._handle;
  }

  protected _assertNotDisposed(): void {
    if (this._disposed) {
      const name = (this.constructor as typeof BoltFFIHandle)._typeName;
      throw new Error(`${name} has been disposed`);
    }
  }

  /** Null maps to the zero handle, matching the wasm-side convention. */
  static _toHandle(value: BoltFFIHandle | null): number {
    return value === null ? 0 : value._borrowHandle();
  }
}
