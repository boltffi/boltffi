// Reports artifact sizes with their composition, so a size comparison can be
// audited rather than taken at face value.
//
// Usage: node report-size.mjs [--json]
import { existsSync, readFileSync, statSync } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { dirname, join } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));

// Every package bench.mjs loads. BoltFFI is packed twice from the same crate —
// once through the benchmark overlay, once plain — and the harness imports
// both, so both count towards what the measured setup loads.
const BACKENDS = [
  {
    label: 'boltffi',
    packages: [
      {
        name: 'overlay',
        wasm: 'build/generated/boltffi/demo_bg.wasm',
        glue: 'build/generated/boltffi/demo_node.js',
        esm: true,
      },
      {
        name: 'demo',
        wasm: 'build/generated/boltffi-demo/demo_bg.wasm',
        glue: 'build/generated/boltffi-demo/demo_node.js',
        esm: true,
      },
    ],
  },
  {
    label: 'wasm-bindgen',
    packages: [
      {
        name: 'demo',
        wasm: 'build/generated/wasmbindgen/demo_bg.wasm',
        glue: 'build/generated/wasmbindgen/demo.js',
        esm: false,
      },
    ],
  },
];

const SECTION_NAMES = {
  1: 'type', 2: 'import', 3: 'function', 4: 'table', 5: 'memory', 6: 'global',
  7: 'export', 8: 'start', 9: 'element', 10: 'code', 11: 'data', 12: 'datacount',
};

function readVarUint(bytes, offset) {
  let result = 0, shift = 0, byte;
  do { byte = bytes[offset++]; result |= (byte & 0x7f) << shift; shift += 7; } while (byte & 0x80);
  return [result >>> 0, offset];
}

function sections(bytes) {
  const out = [];
  let offset = 8;
  while (offset < bytes.length) {
    const id = bytes[offset++];
    let size;
    [size, offset] = readVarUint(bytes, offset);
    let label = SECTION_NAMES[id] ?? `id${id}`;
    if (id === 0) {
      const [nameLen, nameStart] = readVarUint(bytes, offset);
      label = `custom:${bytes.toString('utf8', nameStart, nameStart + nameLen)}`;
    }
    out.push({ label, size });
    offset += size;
  }
  return out;
}

/**
 * Splits wasm exports by which framework emitted them.
 *
 * BoltFFI prefixes its symbols. wasm-bindgen splits in two: `__wbg`/`__wbindgen`
 * internals, and the exported API itself, which keeps ordinary Rust names — so
 * what is left over is a backend's public surface, not an "other" bucket.
 */
function classifyExports(bytes) {
  const names = WebAssembly.Module.exports(new WebAssembly.Module(bytes))
    .filter((entry) => entry.kind === 'function')
    .map((entry) => entry.name);
  const isBoltffi = (n) => n.startsWith('boltffi_') || n.startsWith('__boltffi');
  const isWasmBindgenInternal = (n) => n.startsWith('__wbg') || n.startsWith('__wbindgen');
  const boltffi = names.filter(isBoltffi);
  const internals = names.filter(isWasmBindgenInternal);
  const plain = names.filter((n) => !isBoltffi(n) && !isWasmBindgenInternal(n));
  const nameBytes = (list) => list.reduce((sum, n) => sum + n.length + 2, 0);
  return {
    total: names.length,
    boltffi: boltffi.length,
    boltffi_name_bytes: nameBytes(boltffi),
    wasm_bindgen_internals: internals.length,
    wasm_bindgen_internal_name_bytes: nameBytes(internals),
    plain_api: plain.length,
  };
}

/**
 * Counts the functions a glue module exports.
 *
 * Loading the module rather than matching declaration syntax keeps the count
 * independent of how the generator spells things: `export async function` and
 * exported classes count the same as plain declarations.
 */
async function countGlueExports(path, esm) {
  if (esm) {
    const module = await import(pathToFileURL(path).href);
    if (module.initialized) await module.initialized;
    return Object.values(module).filter((value) => typeof value === 'function').length;
  }
  const require = createRequire(import.meta.url);
  return Object.values(require(path)).filter((value) => typeof value === 'function').length;
}

const report = [];
for (const { label, packages } of BACKENDS) {
  const measured = [];
  for (const entry of packages) {
    const wasmPath = join(HERE, entry.wasm);
    if (!existsSync(wasmPath)) {
      console.error(`missing ${entry.wasm} — run ./run-bench.sh first to build the artifacts`);
      process.exit(1);
    }
    const bytes = readFileSync(wasmPath);
    measured.push({
      name: entry.name,
      wasm_bytes: bytes.length,
      glue_bytes: statSync(join(HERE, entry.glue)).size,
      glue_functions: await countGlueExports(join(HERE, entry.glue), entry.esm),
      sections: sections(bytes).filter((s) => s.size > 1024).sort((a, b) => b.size - a.size),
      exports: classifyExports(bytes),
    });
  }
  report.push({ label, packages: measured });
}

if (process.argv.includes('--json')) {
  console.log(JSON.stringify({ backends: report }, null, 2));
} else {
  const kib = (n) => `${(n / 1024).toFixed(0)} KiB`;

  for (const backend of report) {
    const wasmTotal = backend.packages.reduce((sum, p) => sum + p.wasm_bytes, 0);
    const glueTotal = backend.packages.reduce((sum, p) => sum + p.glue_bytes, 0);
    const functions = backend.packages.reduce((sum, p) => sum + p.glue_functions, 0);
    console.log(`\n${backend.label} — harness loads wasm ${kib(wasmTotal)}, js glue ${kib(glueTotal)}`);

    for (const pkg of backend.packages) {
      console.log(
        `  [${pkg.name}] wasm ${kib(pkg.wasm_bytes)}, glue ${kib(pkg.glue_bytes)} over ${pkg.glue_functions} exported functions`
      );
      console.log(`    sections: ${pkg.sections.map((s) => `${s.label} ${kib(s.size)}`).join(', ')}`);
      const e = pkg.exports;
      console.log(
        `    exports: ${e.total} — boltffi ${e.boltffi}, wasm-bindgen internals ${e.wasm_bindgen_internals}, plain API ${e.plain_api}`
      );
    }
    if (functions > 0) {
      console.log(`  ${Math.round(glueTotal / functions)} glue bytes per exported function`);
    }
  }

  console.log('\nCaveats:');
  const boltffi = report.find((backend) => backend.label === 'boltffi');
  if (boltffi && boltffi.packages.length > 1) {
    console.log(
      `  - BoltFFI is packed ${boltffi.packages.length}x from the same crate and the harness loads all of them; a consumer ships one.`
    );
  }
  for (const backend of report) {
    for (const pkg of backend.packages) {
      const e = pkg.exports;
      const [count, bytes] =
        backend.label === 'boltffi'
          ? [e.wasm_bindgen_internals, e.wasm_bindgen_internal_name_bytes]
          : [e.boltffi, e.boltffi_name_bytes];
      if (count === 0) continue;
      const share = ((bytes / pkg.wasm_bytes) * 100).toFixed(1);
      console.log(
        `  - ${backend.label}/${pkg.name} carries ${count} exports from the other framework, ${kib(bytes)} of names (${share}% of the binary, excluding their code).`
      );
    }
  }
}
