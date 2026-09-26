import { test, expect } from "@playwright/test";

const rawPage = "http://127.0.0.1:4175/examples/platforms/wasm/browser/raw.html";
const packagePath = "/examples/platforms/wasm/dist";

["web", "bundler"].forEach((entry) => {
  test(`${entry} supports dependencies and the BoltFFI API in the same instance`, async ({ page }) => {
    if (entry === "web") {
      await page.goto(rawPage);
      await page.evaluate(async (path) => {
        const demo = await import(`${path}/web.js`);
        await demo.initialized;
        globalThis.demo = demo;
      }, packagePath);
    } else {
      await page.goto("http://127.0.0.1:4176");
      await page.waitForFunction(() => globalThis.demo !== undefined);
    }
    const result = await page.evaluate(async () => {
      const demo = globalThis.demo;
      const uuids = new Set(Array.from({ length: 10_000 }, () => demo.wasmUuidV4()));
      const counter = demo.Counter.new(2);
      counter.increment();
      const count = counter.get();
      counter.dispose();
      const largeString = "🦀".repeat(2_000_000);
      const largeRoundtrip = demo.echoString(largeString) === largeString;
      return {
        unique: uuids.size,
        valid: [...uuids].every((uuid) => /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(uuid)),
        v7: demo.wasmUuidV7().at(14),
        recent: Math.abs(Number(demo.wasmCurrentTime()) - Date.now()) < 1000,
        offset: demo.wasmLocalOffset() === -new Date().getTimezoneOffset() * 60,
        json: [demo.wasmJsonAccepts('{"value":1}'), demo.wasmJsonAccepts("{broken")],
        closure: demo.wasmJsClosure(7),
        snippet: demo.wasmLocalSnippet(41),
        inline: demo.wasmInlineSnippet(),
        starts: demo.wasmStartCount(),
        constants: [demo.wasmTrue, demo.wasmFalse],
        promise: await demo.wasmAwaitPromise("promises 🦀"),
        line: demo.echoLine(demo.makeLine(0, 0, 3, 4)),
        callback: demo.applyClosure((value) => demo.wasmJsClosure(value), 5),
        asyncSum: await demo.asyncAdd(3, 7),
        count,
        largeRoundtrip,
      };
    });
    expect(result).toEqual({
      unique: 10_000, valid: true, v7: "7", recent: true, offset: true,
      json: [true, false], closure: 21, snippet: 42, inline: 73, starts: 1,
      constants: [true, false],
      promise: "promises 🦀", line: { start: { x: 0, y: 0 }, end: { x: 3, y: 4 } },
      callback: 15, asyncSum: 10, count: 3, largeRoundtrip: true,
    });
  });
});

test("dependency startup can reenter BoltFFI and cannot attach a second instance", async ({ page }) => {
  await page.goto(rawPage);
  const result = await page.evaluate(async (path) => {
    const demo = await import(`${path}/demo.js`);
    const observations = [];
    globalThis.boltffiStartupHook = () => observations.push(demo.wasmJsClosure(7));
    const bytes = await (await fetch(`${path}/demo_bg.wasm`)).arrayBuffer();
    await demo.default(bytes);
    let rejected;
    try { await demo.default(bytes); } catch (error) { rejected = error.message; }
    return {
      observations, rejected, uuid: demo.wasmUuidV4(), starts: demo.wasmStartCount(),
      constants: [demo.wasmTrue, demo.wasmFalse, demo.demoComputed, Array.from(demo.demoBytes)],
    };
  }, packagePath);
  expect(result.observations).toEqual([21]);
  expect(result.constants).toEqual([true, false, 42, [102, 102, 105]]);
  expect(result.rejected).toContain("state ready");
  expect(result.uuid).toHaveLength(36);
  expect(result.starts).toBe(1);
});

test("failed dependency startup prevents reusing the glue", async ({ page }) => {
  await page.goto(rawPage);
  const errors = await page.evaluate(async (path) => {
    const demo = await import(`${path}/demo.js`);
    globalThis.boltffiStartupHook = () => { throw new Error("dependency failed"); };
    const bytes = await (await fetch(`${path}/demo_bg.wasm`)).arrayBuffer();
    const errors = [];
    try { await demo.default(bytes); } catch (error) { errors.push(error.message); }
    try { await demo.default(bytes); } catch (error) { errors.push(error.message); }
    return errors;
  }, packagePath);
  expect(errors[0]).toBe("dependency failed");
  expect(errors[1]).toContain("state failed");
});

test("an async Rust trap rejects other pending calls", async ({ page }) => {
  await page.goto(rawPage);
  const failures = await page.evaluate(async (path) => {
    const demo = await import(`${path}/web.js`);
    await demo.initialized;
    const results = await Promise.allSettled([
      demo.wasmAsyncPanic(),
      demo.wasmAwaitPromise("pending during a trap"),
    ]);
    return results.map((result) => ({ status: result.status, error: result.reason?.name }));
  }, packagePath);
  expect(failures).toEqual([
    { status: "rejected", error: "RuntimeError" },
    { status: "rejected", error: "RuntimeError" },
  ]);
});
