import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it, vi } from "vitest";
import { WASM_ABI_VERSION, instantiateBoltFFI, instantiateBoltFFISync } from "../src/module.js";

/**
 * The Rust side of the same import, end to end: `boltffi_cli`'s
 * `wasm_getrandom` fixture, compiled for wasm32 with the custom `getrandom`
 * backend, exports a `random_u64` that draws from `getrandom::u64()`.
 *
 * CI builds it and passes its path; without one the suite skips, except in CI.
 */
const FIXTURE = process.env.BOLTFFI_GETRANDOM_FIXTURE_WASM;

if (!FIXTURE && process.env.CI) {
  throw new Error("BOLTFFI_GETRANDOM_FIXTURE_WASM is not set; CI must build the fixture");
}

type RandomU64 = () => bigint;

function randomU64(exports: Record<string, WebAssembly.ExportValue>): RandomU64 {
  const name = Object.keys(exports).find((key) => key.endsWith("_random_u64"));
  expect(name, "the fixture exports random_u64").toBeDefined();
  return exports[name!] as RandomU64;
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe.skipIf(!FIXTURE)("the getrandom fixture through the runtime", () => {
  const bytes = FIXTURE ? readFileSync(FIXTURE) : new Uint8Array();

  it("draws from crypto.getRandomValues through instantiateBoltFFI", async () => {
    const spy = vi.spyOn(globalThis.crypto, "getRandomValues");
    const mod = await instantiateBoltFFI(bytes, WASM_ABI_VERSION);
    const next = randomU64(mod.exports);

    const values = [next(), next(), next()];

    // `getrandom::u64()` fills eight bytes of the module's memory per call; the
    // fixture returns 0 when getrandom fails
    expect(spy).toHaveBeenCalledTimes(3);
    for (const [view] of spy.mock.calls) {
      expect((view as Uint8Array).buffer).toBe(mod.exports.memory.buffer);
      expect((view as Uint8Array).byteLength).toBe(8);
    }
    expect(values).not.toContain(0n);
    expect(new Set(values).size).toBe(3);
  });

  it("draws through instantiateBoltFFISync", () => {
    const mod = instantiateBoltFFISync(bytes, WASM_ABI_VERSION);
    const next = randomU64(mod.exports);

    expect(next()).not.toBe(0n);
    expect(next()).not.toBe(next());
  });

  it("turns a missing Web Crypto into getrandom's error instead of a trap", async () => {
    vi.stubGlobal("crypto", undefined);
    const mod = await instantiateBoltFFI(bytes, WASM_ABI_VERSION);

    expect(randomU64(mod.exports)()).toBe(0n);
  });
});
