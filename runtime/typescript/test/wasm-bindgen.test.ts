import { beforeAll, describe, expect, it } from "vitest";
import wabt from "wabt";

import { BoltFFIModule, WASM_ABI_VERSION, instantiateBoltFFI, instantiateBoltFFISync } from "../src/module.js";
import { WasmBindgenModule } from "../src/wasm-bindgen.js";

let binary: Uint8Array<ArrayBuffer>;

beforeAll(async () => {
  const compiler = await wabt();
  const module = compiler.parseWat("startup.wat", `(module
    (import "./fixture_bg.js" "read" (func $read (result i32)))
    (import "env" "callback" (func $callback (param i32)))
    (memory (export "memory") 1)
    (global $starts (mut i32) (i32.const 0))
    (func (export "boltffi_wasm_abi_version") (result i32) i32.const ${WASM_ABI_VERSION})
    (func (export "boltffi_wasm_return_slot_addr") (result i32) i32.const 0)
    (func (export "starts") (result i32) global.get $starts)
    (func (export "__wbindgen_start")
      global.get $starts i32.const 1 i32.add global.set $starts
      i32.const 32 i32.const 73 i32.store
      call $read call $callback))`);
  binary = new Uint8Array(module.toBinary({}).buffer);
  module.destroy();
});

describe.each(["sync", "async"] as const)("%s wasm-bindgen initialization", (mode) => {
  it("binds the same memory and makes BoltFFI available before dependency startup", async () => {
    let attached: WebAssembly.Exports | undefined;
    let bound: BoltFFIModule | undefined;
    const observations: number[] = [];
    const wasmBindgen = new WasmBindgenModule({
      "./fixture_bg.js": {
        read: () => new DataView((attached!.memory as WebAssembly.Memory).buffer).getInt32(32, true),
      },
    }, (exports) => { attached = exports; });
    const imports = {
      wasmBindgen,
      bind: (module: BoltFFIModule) => { bound = module; },
      env: { callback: (value: number) => {
        expect(bound!.exports).toBe(attached);
        expect((bound!.exports.starts as CallableFunction)()).toBe(1);
        observations.push(value);
      } },
    };
    const module = mode === "sync"
      ? instantiateBoltFFISync(binary, WASM_ABI_VERSION, imports)
      : await instantiateBoltFFI(binary, WASM_ABI_VERSION, imports);
    expect(module.exports).toBe(attached);
    expect(observations).toEqual([73]);
    expect(() => instantiateBoltFFISync(binary, WASM_ABI_VERSION, imports)).toThrow("state ready");
    expect(observations).toEqual([73]);
  });

  it("checks the ABI before attaching glue or running startup", async () => {
    const events: string[] = [];
    const wasmBindgen = new WasmBindgenModule({ "./fixture_bg.js": { read: () => 0 } }, () => { events.push("attach"); });
    const imports = { wasmBindgen, env: { callback: () => events.push("startup") } };
    const initialize = async () => mode === "sync"
      ? instantiateBoltFFISync(binary, WASM_ABI_VERSION + 1, imports)
      : instantiateBoltFFI(binary, WASM_ABI_VERSION + 1, imports);
    await expect(initialize()).rejects.toThrow("ABI version mismatch");
    expect(events).toEqual([]);
    await expect(initialize()).rejects.toThrow("state failed");
  });

  it("does not reuse glue after a dependency fails during startup", async () => {
    const wasmBindgen = new WasmBindgenModule({ "./fixture_bg.js": { read: () => 0 } }, () => {});
    const imports = { wasmBindgen, env: { callback: () => { throw new Error("startup failed"); } } };
    const initialize = async () => mode === "sync"
      ? instantiateBoltFFISync(binary, WASM_ABI_VERSION, imports)
      : instantiateBoltFFI(binary, WASM_ABI_VERSION, imports);
    await expect(initialize()).rejects.toThrow("startup failed");
    await expect(initialize()).rejects.toThrow("state failed");
  });
});

it("reserves glue before asynchronous instantiation can race with another loader", async () => {
  const wasmBindgen = new WasmBindgenModule({ "./fixture_bg.js": { read: () => 0 } }, () => {});
  const imports = { wasmBindgen, env: { callback: () => {} } };
  const first = instantiateBoltFFI(binary, WASM_ABI_VERSION, imports);
  await expect(instantiateBoltFFI(binary, WASM_ABI_VERSION, imports)).rejects.toThrow("state loading");
  const module = await first;
  expect((module.exports.starts as CallableFunction)()).toBe(1);
});
