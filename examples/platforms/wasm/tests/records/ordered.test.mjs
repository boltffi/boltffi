import { assert, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:records.ordered.rank.should_compare_by_rust_ord");
  const low = { tier: 1, score: 9 };
  const high = { tier: 2, score: 0 };
  assert.equal(demo.Rank.compareTo(low, high), -1);
  assert.equal(demo.Rank.compareTo(high, low), 1);
  assert.equal(demo.Rank.compareTo(low, { tier: 1, score: 9 }), 0);
  assert.deepEqual(
    [high, { tier: 1, score: 10 }, low].sort(demo.Rank.compareTo),
    [low, { tier: 1, score: 10 }, high],
  );

  globalThis.demoCase("case:records.ordered.semantic_version.should_compare_by_rust_ord");
  const alpha = { major: 1, minor: 2, patch: 3, pre: "alpha" };
  const beta = { major: 1, minor: 2, patch: 3, pre: "beta" };
  const release = { major: 1, minor: 2, patch: 3, pre: null };
  const nextPatch = { major: 1, minor: 2, patch: 4, pre: "alpha" };
  assert.ok(demo.SemanticVersion.compareTo(alpha, beta) < 0);
  assert.ok(demo.SemanticVersion.compareTo(beta, release) < 0);
  assert.ok(demo.SemanticVersion.compareTo(release, nextPatch) < 0);
  assert.deepEqual(
    [nextPatch, release, beta, alpha].sort(demo.SemanticVersion.compareTo),
    [alpha, beta, release, nextPatch],
  );
}
