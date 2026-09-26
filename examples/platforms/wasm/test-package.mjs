import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const npm = process.env.npm_execpath;
assert.ok(npm, "run this test with npm run test:package");
const workspace = fileURLToPath(new URL("../../../", import.meta.url));
const consumer = await mkdtemp(join(tmpdir(), "boltffi-wasm-consumer-"));

try {
  const packages = [join(workspace, "runtime/typescript"), join(workspace, "examples/platforms/wasm/dist")].map((directory) => {
    const packed = JSON.parse(execFileSync(process.execPath, [npm, "pack", "--json", "--pack-destination", consumer], { cwd: directory, encoding: "utf8" }))[0];
    if (packed.name === "@boltffi/demo") {
      const files = new Set(packed.files.map((file) => file.path));
      ["demo_node.js", "demo_imports.js", "demo_bg.js", "demo_bg.wasm", "demo.js.map"].forEach((file) => assert.ok(files.has(file), `missing ${file}`));
      assert.ok([...files].some((file) => file.startsWith("snippets/")));
    }
    return join(consumer, packed.filename);
  });
  await writeFile(join(consumer, "package.json"), JSON.stringify({ private: true, type: "module" }));
  execFileSync(process.execPath, [npm, "install", "--ignore-scripts", "--no-audit", "--no-fund", ...packages], { cwd: consumer, stdio: "inherit" });
  const sourceMap = JSON.parse(await readFile(join(consumer, "node_modules/@boltffi/demo/demo.js.map"), "utf8"));
  assert.ok(sourceMap.sourcesContent.some((source) => source.includes("wasmUuidV4")));
  const synchronous = `
    import assert from 'node:assert/strict';
    import * as demo from '@boltffi/demo';
    import { JavaScriptState } from './node_modules/@boltffi/demo/demo_bg.js';
    await demo.initialized;
    assert.equal(JavaScriptState.Ready, 0);
    assert.equal('JavaScriptState' in demo, false);
    assert.equal(demo.wasmUuidV4().length, 36);
    assert.equal(demo.wasmStartCount(), 1);
    assert.equal(demo.wasmTrue, true);
    assert.equal(demo.wasmFalse, false);
    assert.equal(demo.wasmJsClosure(7), 21);
    assert.equal(await demo.wasmAwaitPromise('installed'), 'installed');
  `;
  const asynchronous = `
    import assert from 'node:assert/strict';
    import { readFileSync } from 'node:fs';
    import * as demo from './node_modules/@boltffi/demo/demo.js';
    const values = [];
    globalThis.boltffiStartupHook = () => values.push(demo.wasmJsClosure(7));
    await demo.default(readFileSync('./node_modules/@boltffi/demo/demo_bg.wasm'));
    assert.deepEqual(values, [21]);
    assert.equal(demo.demoComputed, 42);
    assert.deepEqual(Array.from(demo.demoBytes), [102, 102, 105]);
    assert.equal(demo.wasmTrue, true);
    assert.equal(demo.wasmFalse, false);
    assert.equal(demo.wasmUuidV4().length, 36);
    assert.equal(demo.wasmLocalSnippet(41), 42);
    assert.equal(await demo.wasmAwaitPromise('installed'), 'installed');
  `;
  [synchronous, asynchronous].forEach((source) => {
    execFileSync(process.execPath, ["--input-type=module", "--eval", source], { cwd: consumer, stdio: "inherit" });
  });
} finally {
  await rm(consumer, { recursive: true, force: true });
}
