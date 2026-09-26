

const _wasmBytes = readFileSync(_wasmPath);
let _module: BoltFFIModule;
let _exports: BoltFFIExports;
instantiateBoltFFISync(_wasmBytes, WASM_ABI_VERSION, {
  env: _callbackImports,
  wasmBindgen: _wasmBindgen,
  bind(module) {
    _module = module;
    _exports = module.exports;
  },
});
{{ constant_initializers }}

export const initialized = Promise.resolve();
export default function init(): Promise<void> { return initialized; }

// Lower-level counterpart to `options.signal` / `options.cancelId`.
// `callId` must be unique among in-flight calls -- see AsyncFutureManager.cancelById.
export function __boltffiCancelById(callId: number): void {
  _module.asyncManager.cancelById(callId);
}
