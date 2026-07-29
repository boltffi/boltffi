import { AsyncFutureManager, BoltFFIModule } from "../../src/module.js";
import { StreamPollManager } from "../../src/stream.js";

/**
 * A `BoltFFIModule` over a plain `WebAssembly.Memory` and a bump allocator, so
 * the packed-return paths can be tested without building a wasm module.
 *
 * Frees are recorded rather than reclaimed: most of these tests are about a
 * payload having been released, and nothing still pointing at it afterwards.
 */
export interface FakeModule {
  module: BoltFFIModule;
  memory: WebAssembly.Memory;
  /** Every `(pointer, length)` released through the packed-return free. */
  freed: Array<{ pointer: number; length: number }>;
  /** Copies `bytes` into wasm memory and returns the packed pointer/length. */
  pack(bytes: Uint8Array): bigint;
  /** Builds a packed value by hand, to exercise malformed payloads. */
  packRaw(pointer: number, length: number): bigint;
  /** Overwrites a region, standing in for the allocator reusing it. */
  scribble(pointer: number, length: number): void;
}

export function createFakeModule(pages = 1): FakeModule {
  const memory = new WebAssembly.Memory({ initial: pages });
  const freed: FakeModule["freed"] = [];
  let bump = 64; // leave the return slot at 0 alone

  const exports = {
    memory,
    boltffi_wasm_return_slot_addr: () => 0,
    boltffi_wasm_alloc: (size: number) => {
      const pointer = bump;
      bump = (bump + size + 7) & ~7;
      if (bump > memory.buffer.byteLength) {
        memory.grow(Math.ceil((bump - memory.buffer.byteLength) / 65536) + 1);
      }
      return pointer;
    },
    boltffi_wasm_free: () => {},
    boltffi_wasm_free_string_return: (pointer: number, length: number) => {
      freed.push({ pointer, length });
    },
  };

  const module = new BoltFFIModule(
    { exports } as unknown as WebAssembly.Instance,
    new AsyncFutureManager(),
    new StreamPollManager()
  );

  const packRaw = (pointer: number, length: number): bigint =>
    (BigInt(length) << 32n) | BigInt(pointer);
  const pack = (bytes: Uint8Array): bigint => {
    const pointer = exports.boltffi_wasm_alloc(bytes.length);
    new Uint8Array(memory.buffer).set(bytes, pointer);
    return packRaw(pointer, bytes.length);
  };
  const scribble = (pointer: number, length: number): void => {
    new Uint8Array(memory.buffer).fill(0xff, pointer, pointer + length);
  };

  return { module, memory, freed, pack, packRaw, scribble };
}
