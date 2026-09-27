import { beforeAll, describe, expect, it } from "vitest";
import wabt from "wabt";

import { BoltFFIModule, WASM_ABI_VERSION, instantiateBoltFFI, instantiateBoltFFISync } from "../src/module.js";
import { WasmBindgenModule } from "../src/wasm-bindgen.js";

let binary: Uint8Array<ArrayBuffer>;
let unprocessed: Uint8Array<ArrayBuffer>;

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
  const raw = compiler.parseWat("unprocessed.wat", `(module
    (import "__wbindgen_placeholder__" "__wbg_getRandomValues_fixture" (func))
    (import "__wbindgen_externref_xform__" "__wbindgen_externref_table_grow" (func)))`);
  unprocessed = new Uint8Array(raw.toBinary({}).buffer);
  raw.destroy();
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

  it("allows a corrected artifact after rejecting its ABI before attachment", async () => {
    const events: string[] = [];
    const wasmBindgen = new WasmBindgenModule({ "./fixture_bg.js": { read: () => 0 } }, () => { events.push("attach"); });
    const imports = { wasmBindgen, env: { callback: () => events.push("startup") } };
    const initialize = async (version: number) => mode === "sync"
      ? instantiateBoltFFISync(binary, version, imports)
      : instantiateBoltFFI(binary, version, imports);
    await expect(initialize(WASM_ABI_VERSION + 1)).rejects.toThrow("ABI version mismatch");
    expect(events).toEqual([]);
    const module = await initialize(WASM_ABI_VERSION);
    expect(events).toEqual(["attach", "startup"]);
    expect((module.exports.starts as CallableFunction)()).toBe(1);
  });

  it("rejects unprocessed imports during instantiation", async () => {
    const initialize = async () => mode === "sync"
      ? instantiateBoltFFISync(unprocessed, WASM_ABI_VERSION)
      : instantiateBoltFFI(unprocessed, WASM_ABI_VERSION);
    await expect(initialize()).rejects.toThrow("__wbindgen_placeholder__");
  });

  it("allows missing imports to be supplied after a failed instantiation", async () => {
    let attachments = 0;
    const wasmBindgen = new WasmBindgenModule({ "./fixture_bg.js": { read: () => 0 } }, () => { attachments++; });
    const initialize = async (env: Record<string, WebAssembly.ImportValue>) => mode === "sync"
      ? instantiateBoltFFISync(binary, WASM_ABI_VERSION, { wasmBindgen, env })
      : instantiateBoltFFI(binary, WASM_ABI_VERSION, { wasmBindgen, env });
    await expect(initialize({})).rejects.toThrow("callback");
    expect(attachments).toBe(0);
    const module = await initialize({ callback: () => {} });
    expect(attachments).toBe(1);
    expect((module.exports.starts as CallableFunction)()).toBe(1);
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

it("allows another response after reading the first response fails", async () => {
  let attachments = 0;
  const wasmBindgen = new WasmBindgenModule({ "./fixture_bg.js": { read: () => 0 } }, () => { attachments++; });
  const imports = { wasmBindgen, env: { callback: () => {} } };
  const consumed = new Response(binary);
  await consumed.arrayBuffer();
  await expect(instantiateBoltFFI(consumed, WASM_ABI_VERSION, imports)).rejects.toThrow();
  expect(attachments).toBe(0);
  const module = await instantiateBoltFFI(new Response(binary), WASM_ABI_VERSION, imports);
  expect(attachments).toBe(1);
  expect((module.exports.starts as CallableFunction)()).toBe(1);
});

it("reserves glue before asynchronous instantiation can race with another loader", async () => {
  const wasmBindgen = new WasmBindgenModule({ "./fixture_bg.js": { read: () => 0 } }, () => {});
  const imports = { wasmBindgen, env: { callback: () => {} } };
  const first = instantiateBoltFFI(binary, WASM_ABI_VERSION, imports);
  await expect(instantiateBoltFFI(binary, WASM_ABI_VERSION, imports)).rejects.toThrow("state loading");
  const module = await first;
  expect((module.exports.starts as CallableFunction)()).toBe(1);
});
