import { BoltFFIModule, CallbackRegistry, StreamCancellable, StreamSession, WASM_ABI_VERSION, instantiateBoltFFI, matchWireResult, utf8ByteCount, wireArraySize, wireMapSize, wireOptionalSize, wireResultSize, wireStringSize } from {{ runtime_package }};
import type { BoltFFIExports, Duration, WireCodec, WireResult } from {{ runtime_package }};

let _module: BoltFFIModule;
let _exports: BoltFFIExports;
const _callbackImports: Record<string, WebAssembly.ImportValue> = {};
{% for import in imports %}_callbackImports[{{ import }}] = (..._arguments: unknown[]) => {
  throw new Error("Wasm import " + {{ import }} + " has no installed TypeScript adapter");
};
{% endfor %}
{{ closure_adapters }}

export default async function init(source: BufferSource | Response | WebAssembly.Module): Promise<void> {
  _module = await instantiateBoltFFI(source, WASM_ABI_VERSION, { env: _callbackImports });
  _exports = _module.exports;
{{ constant_initializers }}
}
