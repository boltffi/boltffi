

const _wasmBytes = readFileSync(_wasmPath);
const _module: BoltFFIModule = instantiateBoltFFISync(_wasmBytes, WASM_ABI_VERSION, { env: _callbackImports });
const _exports: BoltFFIExports = _module.exports;
{{ constant_initializers }}

export const initialized = Promise.resolve();
export default function init(): Promise<void> { return initialized; }

// Lower-level counterpart to `options.signal` / `options.cancelId`.
// `callId` must be unique among in-flight calls -- see AsyncFutureManager.cancelById.
export function __boltffiCancelById(callId: number): void {
  _module.asyncManager.cancelById(callId);
}
