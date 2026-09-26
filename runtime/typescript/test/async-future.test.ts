import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import wabt from "wabt";

import { AsyncFutureManager, WasmPollStatus } from "../src/module.js";

let trap: () => never;

beforeAll(async () => {
  const compiler = await wabt();
  const module = compiler.parseWat("trap.wat", '(module (func (export "trap") unreachable))');
  const instance = new WebAssembly.Instance(new WebAssembly.Module(module.toBinary({}).buffer));
  trap = instance.exports.trap as () => never;
  module.destroy();
});

afterEach(() => vi.restoreAllMocks());

describe("async Wasm traps", () => {
  it("rejects the whole wake batch and stops calling Wasm after a trap", async () => {
    const manager = new AsyncFutureManager();
    const controller = new AbortController();
    const detach = vi.spyOn(controller.signal, "removeEventListener");
    const scheduled: VoidFunction[] = [];
    vi.spyOn(globalThis, "queueMicrotask").mockImplementation((callback) => scheduled.push(callback));
    const poll = vi.fn().mockReturnValueOnce(WasmPollStatus.Pending).mockImplementation(trap);
    const otherPoll = vi.fn().mockReturnValueOnce(WasmPollStatus.Pending).mockReturnValue(WasmPollStatus.Ready);
    const panicMessage = vi.fn();
    const free = vi.fn();
    const cancel = vi.fn();
    const first = manager.pollAsync(1, poll, panicMessage, free, cancel, controller.signal, 41);
    const second = manager.pollAsync(2, otherPoll, panicMessage, free, cancel, controller.signal, 42);
    const results = Promise.allSettled([first, second]);
    manager.wake(1);
    manager.wake(2);

    expect(scheduled).toHaveLength(1);
    expect(() => scheduled.shift()!()).not.toThrow();
    const settled = await results;
    expect(settled).toEqual([
      { status: "rejected", reason: expect.any(WebAssembly.RuntimeError) },
      { status: "rejected", reason: expect.any(WebAssembly.RuntimeError) },
    ]);
    expect(otherPoll).toHaveBeenCalledTimes(1);
    expect(detach).toHaveBeenCalledTimes(2);

    manager.wake(1);
    manager.cancelById(41);
    manager.cancelById(42);
    controller.abort();
    await expect(manager.pollAsync(3, otherPoll, panicMessage, free, cancel)).rejects.toBe(
      (settled[0] as PromiseRejectedResult).reason,
    );
    expect(scheduled).toHaveLength(0);
    expect(otherPoll).toHaveBeenCalledTimes(1);
    expect(panicMessage).not.toHaveBeenCalled();
    expect(free).not.toHaveBeenCalled();
    expect(cancel).not.toHaveBeenCalled();
  });

  it("rejects existing futures when a new future traps on its first poll", async () => {
    const manager = new AsyncFutureManager();
    const failures: unknown[] = [];
    const free = vi.fn();
    manager.pollAsync(1, () => WasmPollStatus.Pending, () => 0, free, () => {})
      .catch((error) => failures.push(error));

    let trapped!: Promise<number>;
    expect(() => { trapped = manager.pollAsync(2, trap, () => 0, free, () => {}); }).not.toThrow();
    await expect(trapped).rejects.toBeInstanceOf(WebAssembly.RuntimeError);
    expect(failures).toEqual([expect.any(WebAssembly.RuntimeError)]);
    expect(free).not.toHaveBeenCalled();
  });

  it.each(["signal", "id", "already aborted"])("rejects all futures if cancellation traps through %s", async (source) => {
    const manager = new AsyncFutureManager();
    const controller = new AbortController();
    const failures: unknown[] = [];
    const free = vi.fn();
    manager.pollAsync(1, () => WasmPollStatus.Pending, () => 0, free, () => {})
      .catch((error) => failures.push(error));
    if (source === "already aborted") controller.abort();

    let cancelled!: Promise<number>;
    expect(() => {
      cancelled = manager.pollAsync(2, () => WasmPollStatus.Pending, () => 0, free, trap, controller.signal, 42);
      if (source === "signal") controller.abort();
      if (source === "id") manager.cancelById(42);
    }).not.toThrow();
    await expect(cancelled).rejects.toBeInstanceOf(WebAssembly.RuntimeError);
    expect(failures).toEqual([expect.any(WebAssembly.RuntimeError)]);
    expect(free).not.toHaveBeenCalled();
  });

  it("settles the current promise if releasing a cancelled future traps", async () => {
    const manager = new AsyncFutureManager();
    const scheduled: VoidFunction[] = [];
    vi.spyOn(globalThis, "queueMicrotask").mockImplementation((callback) => scheduled.push(callback));
    const poll = vi.fn().mockReturnValueOnce(WasmPollStatus.Pending).mockReturnValue(WasmPollStatus.Cancelled);
    const pending = manager.pollAsync(1, poll, () => 0, trap, () => {});
    const settled = Promise.allSettled([pending]);
    manager.wake(1);
    expect(() => scheduled.shift()!()).not.toThrow();
    expect(await settled).toEqual([{ status: "rejected", reason: expect.any(WebAssembly.RuntimeError) }]);
  });

  it("still completes other futures after ordinary cancellation", async () => {
    const manager = new AsyncFutureManager();
    const free = vi.fn();
    const cancelledPoll = vi.fn().mockReturnValueOnce(WasmPollStatus.Pending).mockReturnValue(WasmPollStatus.Cancelled);
    const readyPoll = vi.fn().mockReturnValueOnce(WasmPollStatus.Pending).mockReturnValue(WasmPollStatus.Ready);
    const cancelled = manager.pollAsync(1, cancelledPoll, () => 0, free, () => {});
    const ready = manager.pollAsync(2, readyPoll, () => 0, free, () => {});
    const settled = Promise.allSettled([cancelled, ready]);
    manager.wake(1);
    manager.wake(2);
    expect(await settled).toEqual([
      { status: "rejected", reason: expect.objectContaining({ name: "BoltFFICancelledError" }) },
      { status: "fulfilled", value: 2 },
    ]);
    expect(free).toHaveBeenCalledExactlyOnceWith(1);
  });
});
