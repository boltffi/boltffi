import { afterEach, describe, expect, it, vi } from "vitest";
import {
  RandomFillStatus,
  createRandomFillImport,
  instantiateBoltFFI,
  instantiateBoltFFISync,
} from "../src/module.js";

const PAGE = 65536;

/**
 * A module that imports `env.__boltffi_getrandom` the way `boltffi_core` does
 * and exports `fill(ptr, len)`, which calls straight through to it, beside the
 * two exports `BoltFFIModule` needs. Hand-assembled:
 *
 * ```wat
 * (module
 *   (import "env" "__boltffi_getrandom" (func $getrandom (param i32 i32) (result i32)))
 *   (memory (export "memory") 1)
 *   (func (export "boltffi_wasm_abi_version") (result i32) i32.const 7)
 *   (func (export "boltffi_wasm_return_slot_addr") (result i32) i32.const 0)
 *   (func (export "fill") (param i32 i32) (result i32)
 *     local.get 0 local.get 1 call $getrandom))
 * ```
 */
const GETRANDOM_WASM = new Uint8Array([
  0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x0b, 0x02, 0x60, 0x02, 0x7f, 0x7f, 0x01,
  0x7f, 0x60, 0x00, 0x01, 0x7f, 0x02, 0x1b, 0x01, 0x03, 0x65, 0x6e, 0x76, 0x13, 0x5f, 0x5f, 0x62,
  0x6f, 0x6c, 0x74, 0x66, 0x66, 0x69, 0x5f, 0x67, 0x65, 0x74, 0x72, 0x61, 0x6e, 0x64, 0x6f, 0x6d,
  0x00, 0x00, 0x03, 0x04, 0x03, 0x01, 0x01, 0x00, 0x05, 0x03, 0x01, 0x00, 0x01, 0x07, 0x4c, 0x04,
  0x06, 0x6d, 0x65, 0x6d, 0x6f, 0x72, 0x79, 0x02, 0x00, 0x18, 0x62, 0x6f, 0x6c, 0x74, 0x66, 0x66,
  0x69, 0x5f, 0x77, 0x61, 0x73, 0x6d, 0x5f, 0x61, 0x62, 0x69, 0x5f, 0x76, 0x65, 0x72, 0x73, 0x69,
  0x6f, 0x6e, 0x00, 0x01, 0x1d, 0x62, 0x6f, 0x6c, 0x74, 0x66, 0x66, 0x69, 0x5f, 0x77, 0x61, 0x73,
  0x6d, 0x5f, 0x72, 0x65, 0x74, 0x75, 0x72, 0x6e, 0x5f, 0x73, 0x6c, 0x6f, 0x74, 0x5f, 0x61, 0x64,
  0x64, 0x72, 0x00, 0x02, 0x04, 0x66, 0x69, 0x6c, 0x6c, 0x00, 0x03, 0x0a, 0x14, 0x03, 0x04, 0x00,
  0x41, 0x07, 0x0b, 0x04, 0x00, 0x41, 0x00, 0x0b, 0x08, 0x00, 0x20, 0x00, 0x20, 0x01, 0x10, 0x00,
  0x0b,
]);

/**
 * Like `GETRANDOM_WASM`, but the call is made from a start function, which runs
 * inside instantiation, and `start_status` hands back what it returned:
 *
 * ```wat
 * (module
 *   (import "env" "__boltffi_getrandom" (func $getrandom (param i32 i32) (result i32)))
 *   (memory (export "memory") 1)
 *   (global $status (mut i32) (i32.const -1))
 *   (func $start (global.set $status (call $getrandom (i32.const 0) (i32.const 4))))
 *   (start $start)
 *   (func (export "boltffi_wasm_abi_version") (result i32) i32.const 7)
 *   (func (export "boltffi_wasm_return_slot_addr") (result i32) i32.const 0)
 *   (func (export "start_status") (result i32) global.get $status))
 * ```
 */
const START_FUNCTION_WASM = new Uint8Array([
  0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x0e, 0x03, 0x60, 0x02, 0x7f, 0x7f, 0x01,
  0x7f, 0x60, 0x00, 0x01, 0x7f, 0x60, 0x00, 0x00, 0x02, 0x1b, 0x01, 0x03, 0x65, 0x6e, 0x76, 0x13,
  0x5f, 0x5f, 0x62, 0x6f, 0x6c, 0x74, 0x66, 0x66, 0x69, 0x5f, 0x67, 0x65, 0x74, 0x72, 0x61, 0x6e,
  0x64, 0x6f, 0x6d, 0x00, 0x00, 0x03, 0x05, 0x04, 0x02, 0x01, 0x01, 0x01, 0x05, 0x03, 0x01, 0x00,
  0x01, 0x06, 0x06, 0x01, 0x7f, 0x01, 0x41, 0x7f, 0x0b, 0x07, 0x54, 0x04, 0x06, 0x6d, 0x65, 0x6d,
  0x6f, 0x72, 0x79, 0x02, 0x00, 0x18, 0x62, 0x6f, 0x6c, 0x74, 0x66, 0x66, 0x69, 0x5f, 0x77, 0x61,
  0x73, 0x6d, 0x5f, 0x61, 0x62, 0x69, 0x5f, 0x76, 0x65, 0x72, 0x73, 0x69, 0x6f, 0x6e, 0x00, 0x02,
  0x1d, 0x62, 0x6f, 0x6c, 0x74, 0x66, 0x66, 0x69, 0x5f, 0x77, 0x61, 0x73, 0x6d, 0x5f, 0x72, 0x65,
  0x74, 0x75, 0x72, 0x6e, 0x5f, 0x73, 0x6c, 0x6f, 0x74, 0x5f, 0x61, 0x64, 0x64, 0x72, 0x00, 0x03,
  0x0c, 0x73, 0x74, 0x61, 0x72, 0x74, 0x5f, 0x73, 0x74, 0x61, 0x74, 0x75, 0x73, 0x00, 0x04, 0x08,
  0x01, 0x01, 0x0a, 0x1b, 0x04, 0x0a, 0x00, 0x41, 0x00, 0x41, 0x04, 0x10, 0x00, 0x24, 0x00, 0x0b,
  0x04, 0x00, 0x41, 0x07, 0x0b, 0x04, 0x00, 0x41, 0x00, 0x0b, 0x04, 0x00, 0x23, 0x00, 0x0b,
]);

/** Whether any byte in `bytes` is nonzero; 32 zero bytes from a CSPRNG is a 2^-256 event. */
function touched(bytes: Uint8Array): boolean {
  return bytes.some((byte) => byte !== 0);
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("createRandomFillImport", () => {
  it("fills exactly the requested range", () => {
    const memory = new WebAssembly.Memory({ initial: 1 });
    const fill = createRandomFillImport(() => memory);

    expect(fill(16, 32)).toBe(RandomFillStatus.Filled);

    const bytes = new Uint8Array(memory.buffer);
    expect(touched(bytes.subarray(16, 48))).toBe(true);
    expect(touched(bytes.subarray(0, 16))).toBe(false);
    expect(touched(bytes.subarray(48))).toBe(false);
  });

  it("splits a fill into the 64 KiB calls getRandomValues accepts", () => {
    const memory = new WebAssembly.Memory({ initial: 3 });
    const spy = vi.spyOn(globalThis.crypto, "getRandomValues");
    const fill = createRandomFillImport(() => memory);

    expect(fill(1, 2 * PAGE + 7)).toBe(RandomFillStatus.Filled);

    expect(spy.mock.calls.map(([view]) => (view as Uint8Array).byteLength)).toEqual([
      PAGE,
      PAGE,
      7,
    ]);
    expect(spy.mock.calls.map(([view]) => (view as Uint8Array).byteOffset)).toEqual([
      1,
      1 + PAGE,
      1 + 2 * PAGE,
    ]);
  });

  it("reads the memory per call, so a grown memory is still reachable", () => {
    const memory = new WebAssembly.Memory({ initial: 1, maximum: 2 });
    const fill = createRandomFillImport(() => memory);
    memory.grow(1);

    expect(fill(PAGE, 32)).toBe(RandomFillStatus.Filled);
    expect(touched(new Uint8Array(memory.buffer, PAGE, 32))).toBe(true);
  });

  it("fills shared memory through a scratch buffer, as browsers refuse shared views", () => {
    const memory = new WebAssembly.Memory({ initial: 1, maximum: 1, shared: true });
    const real = globalThis.crypto.getRandomValues.bind(globalThis.crypto);
    vi.spyOn(globalThis.crypto, "getRandomValues").mockImplementation((view) => {
      if ((view as Uint8Array).buffer instanceof SharedArrayBuffer) {
        throw new TypeError("The provided ArrayBufferView value must not be shared.");
      }
      return real(view as Uint8Array<ArrayBuffer>);
    });
    const fill = createRandomFillImport(() => memory);

    expect(fill(8, 32)).toBe(RandomFillStatus.Filled);
    expect(touched(new Uint8Array(memory.buffer, 8, 32))).toBe(true);
  });

  it("reports unavailable without Web Crypto", () => {
    vi.stubGlobal("crypto", undefined);
    const memory = new WebAssembly.Memory({ initial: 1 });

    expect(createRandomFillImport(() => memory)(0, 32)).toBe(RandomFillStatus.Unavailable);
  });

  it("reports unavailable before the instance's memory exists", () => {
    expect(createRandomFillImport(() => undefined)(0, 32)).toBe(RandomFillStatus.Unavailable);
  });

  it("reports a failure rather than throwing into wasm", () => {
    const memory = new WebAssembly.Memory({ initial: 1 });
    const fill = createRandomFillImport(() => memory);

    expect(fill(PAGE - 8, 16)).toBe(RandomFillStatus.Failed);

    vi.spyOn(globalThis.crypto, "getRandomValues").mockImplementation(() => {
      throw new Error("entropy source gone");
    });
    expect(fill(0, 16)).toBe(RandomFillStatus.Failed);
  });
});

describe("the __boltffi_getrandom import", () => {
  type FillExport = (ptr: number, len: number) => number;

  it("is wired into instantiateBoltFFI", async () => {
    const mod = await instantiateBoltFFI(GETRANDOM_WASM.buffer, 7);

    expect((mod.exports.fill as FillExport)(64, 32)).toBe(RandomFillStatus.Filled);
    expect(touched(new Uint8Array(mod.exports.memory.buffer, 64, 32))).toBe(true);
  });

  it("is wired into instantiateBoltFFISync", () => {
    const mod = instantiateBoltFFISync(GETRANDOM_WASM, 7);

    expect((mod.exports.fill as FillExport)(64, 32)).toBe(RandomFillStatus.Filled);
    expect(touched(new Uint8Array(mod.exports.memory.buffer, 64, 32))).toBe(true);
  });

  it("reports unavailable to a start function, before the memory is reachable", async () => {
    type StatusExport = () => number;
    const loaded = await instantiateBoltFFI(START_FUNCTION_WASM.buffer, 7);
    const loadedSync = instantiateBoltFFISync(START_FUNCTION_WASM, 7);

    expect((loaded.exports.start_status as StatusExport)()).toBe(RandomFillStatus.Unavailable);
    expect((loadedSync.exports.start_status as StatusExport)()).toBe(RandomFillStatus.Unavailable);
  });

  it("can be replaced through the caller's env imports", async () => {
    const replacement = vi.fn(() => RandomFillStatus.Unavailable);
    const mod = await instantiateBoltFFI(GETRANDOM_WASM.buffer, 7, {
      env: { __boltffi_getrandom: replacement },
    });

    expect((mod.exports.fill as FillExport)(0, 8)).toBe(RandomFillStatus.Unavailable);
    expect(replacement).toHaveBeenCalledWith(0, 8);
  });
});
