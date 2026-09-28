# @boltffi/runtime

WASM runtime for [BoltFFI](https://boltffi.dev) generated TypeScript bindings.

## Installation

```bash
npm install @boltffi/runtime
```

## Usage

This package is used internally by BoltFFI-generated WASM bindings. You don't need to install it directly - it's bundled with your generated package when you run `boltffi pack wasm`.

### Example

When you run `boltffi pack wasm`, the generated TypeScript uses this runtime:

```typescript
import { distance } from 'your-package';

const d = distance({ x: 0, y: 0 }, { x: 3, y: 4 });
console.log(d); // 5.0
```

## Rust dependencies that call JavaScript

An exported Rust function can use a dependency such as `uuid` with its `v4`
and `js` features. The function still uses `#[boltffi::export]`. No
`#[wasm_bindgen]` attribute is needed on the BoltFFI API.

`boltffi pack wasm` detects wasm-bindgen metadata in the compiled artifact and
runs the matching wasm-bindgen CLI before stripping or optimizing it. The
generated package contains both the BoltFFI bindings and the dependency's
JavaScript bindings, connected to the same Wasm instance. Cargo's original
artifact is preserved, including when packing again with `--no-build`.

Install the exact CLI version named in the pack error with
`cargo install wasm-bindgen-cli --version =VERSION --locked`. An existing
executable can be selected with `BOLTFFI_WASM_BINDGEN` or
`targets.wasm.wasm_bindgen_cli` in `boltffi.toml`. Modules without wasm-bindgen
metadata do not need this tool. TypeScript compilation requires this runtime
and, for the Node entry, `@types/node` to be installed where the output's
parent directories can resolve them.

The generated loaders support Node synchronous initialization, asynchronous
initialization, browser fetches, and bundlers. A JavaScript glue module owns
one Wasm instance. Initializing it again, concurrently, or after failed
dependency startup is rejected. Local and inline JavaScript snippets are
included; npm imports and dynamically computed module paths are rejected
during packaging.

Functions are available during dependency startup so JavaScript can call back
into BoltFFI. Computed constants are read after startup and are available when
initialization completes. Their Rust conversions can then use JavaScript too.

This integration targets `wasm32-unknown-unknown` with non-shared memory and
`panic=abort`. Threads, JSPI, and panic unwinding are outside this contract.
The host must provide the JavaScript APIs a dependency calls; browser DOM APIs,
for example, are unavailable in ordinary Node programs. Uncaught JavaScript
exceptions and Rust traps retain their WebAssembly semantics. They do not
become BoltFFI `Result` values or make the instance safe to reuse.
If polling or cancelling a future throws, all pending BoltFFI futures reject
and that instance's async manager stops polling. Discard the failed instance.

BoltFFI async exports still require `Send` futures. A wasm-bindgen `JsFuture`
does not satisfy that bound. A JavaScript task can complete a channel awaited
by a BoltFFI future, as the Wasm interop demo demonstrates.

## Documentation

- [BoltFFI Docs](https://boltffi.dev/docs)
- [WASM Guide](https://boltffi.dev/docs/getting-started#use-in-typescript)

## License

MIT
