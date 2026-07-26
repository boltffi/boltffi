import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const suiteModules = [
  "./tests/contract.test.mjs",
  "./tests/async_fns/mod.test.mjs",
  "./tests/builtins/mod.test.mjs",
  "./tests/bytes/mod.test.mjs",
  "./tests/callbacks/async_traits.test.mjs",
  "./tests/callbacks/closures.test.mjs",
  "./tests/callbacks/sync_traits.test.mjs",
  "./tests/classes/async_methods.test.mjs",
  "./tests/classes/constructor_matrix.test.mjs",
  "./tests/classes/constructors.test.mjs",
  "./tests/classes/methods.test.mjs",
  "./tests/classes/static_methods.test.mjs",
  "./tests/classes/streams.test.mjs",
  "./tests/classes/thread_safe.test.mjs",
  "./tests/classes/unsafe_single_threaded.test.mjs",
  "./tests/collections/mod.test.mjs",
  "./tests/constants/mod.test.mjs",
  "./tests/custom_types/mod.test.mjs",
  "./tests/enums/c_style.test.mjs",
  "./tests/enums/complex_variants.test.mjs",
  "./tests/enums/data_enum.test.mjs",
  "./tests/enums/repr_int.test.mjs",
  "./tests/multicrate/mod.test.mjs",
  "./tests/options/complex.test.mjs",
  "./tests/options/primitives.test.mjs",
  "./tests/primitives/scalars.test.mjs",
  "./tests/primitives/strings.test.mjs",
  "./tests/primitives/vecs.test.mjs",
  "./tests/records/blittable.test.mjs",
  "./tests/records/default_values.test.mjs",
  "./tests/records/mixed.test.mjs",
  "./tests/records/nested.test.mjs",
  "./tests/records/with_collections.test.mjs",
  "./tests/records/with_enums.test.mjs",
  "./tests/records/with_options.test.mjs",
  "./tests/records/with_strings.test.mjs",
  "./tests/results/async_results.test.mjs",
  "./tests/results/basic.test.mjs",
  "./tests/results/error_enums.test.mjs",
  "./tests/results/error_structs.test.mjs",
  "./tests/results/nested_results.test.mjs",
];

const entrypointPath = fileURLToPath(import.meta.url);
const requestedSuiteModule = process.argv[2];

if (requestedSuiteModule) {
  const requestedSuite = await import(requestedSuiteModule);
  try {
    await requestedSuite.run();
  } catch (error) {
    const caseId = globalThis.__boltffiDemoCase;
    if (caseId && error instanceof Error && !error.message.includes("case:")) {
      error.message = `${caseId}: ${error.message}`;
    }
    throw error;
  }
  process.exit(0);
}

for (const suiteModule of suiteModules) {
  const suiteStatus = spawnSync(process.execPath, [entrypointPath, suiteModule], {
    cwd: process.cwd(),
    stdio: "inherit",
  });

  if (suiteStatus.status !== 0) {
    process.exit(suiteStatus.status ?? 1);
  }
}

console.log("\nAll wasm tests passed!");
