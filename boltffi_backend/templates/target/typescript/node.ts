import { BoltFFIModule, CallbackRegistry, StreamCancellable, StreamSession, WASM_ABI_VERSION, instantiateBoltFFISync, matchWireResult, utf8ByteCount, wireArraySize, wireMapSize, wireOptionalSize, wireResultSize, wireStringSize } from {{ runtime_package }};
import type { BoltFFIExports, Duration, WireCodec, WireResult } from {{ runtime_package }};
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const _thisDir = dirname(fileURLToPath(import.meta.url));
const _wasmPath = join(_thisDir, {{ wasm_file }});
const _callbackImports: Record<string, WebAssembly.ImportValue> = {};
{% for import in imports %}_callbackImports[{{ import }}] = (..._arguments: unknown[]) => {
  throw new Error("Wasm import " + {{ import }} + " has no installed TypeScript adapter");
};
{% endfor %}
{{ closure_adapters }}
