import assert from "node:assert/strict";
import * as demo from "@boltffi/demo";

await demo.initialized;

export { assert, demo };

globalThis.__boltffiDemoCase = null;
globalThis.demoCase = (caseId) => {
  globalThis.__boltffiDemoCase = caseId;
};

export function assertApprox(actual, expected, epsilon = 1e-9) {
  assert.ok(
    Math.abs(actual - expected) <= epsilon,
    `expected ${actual} to be within ${epsilon} of ${expected}`,
  );
}

export function assertPoint(actual, expected, epsilon = 1e-9) {
  assertApprox(actual.x, expected.x, epsilon);
  assertApprox(actual.y, expected.y, epsilon);
}

export function assertArrayEqual(actual, expected) {
  assert.deepEqual(Array.from(actual), Array.from(expected));
}

export function sampleMixedRecordParameters() {
  return {
    tags: ["alpha", "beta"],
    checkpoints: [
      { x: 1, y: 2 },
      { x: 3, y: 5 },
    ],
    fallbackAnchor: { x: -1, y: -2 },
    maxRetries: 4,
    previewOnly: true,
  };
}

export function sampleMixedRecord() {
  return {
    name: "outline",
    anchor: { x: 10, y: 20 },
    priority: demo.Priority.Critical,
    shape: { tag: "Rectangle", width: 3, height: 4 },
    parameters: sampleMixedRecordParameters(),
  };
}

export function assertThrowsWithCode(action, ErrorType, code) {
  try {
    action();
    assert.fail("expected function to throw");
  } catch (error) {
    assert.ok(error instanceof ErrorType, `expected ${ErrorType.name}`);
    assert.equal(error.code, code);
  }
}

export function assertThrowsWithMessage(action, messageFragment) {
  try {
    action();
    assert.fail("expected function to throw");
  } catch (error) {
    assert.ok(error instanceof Error);
    assert.match(error.message, new RegExp(messageFragment));
  }
}

export async function assertRejectsWithCode(action, ErrorType, code) {
  try {
    await action();
    assert.fail("expected promise to reject");
  } catch (error) {
    assert.ok(error instanceof ErrorType, `expected ${ErrorType.name}`);
    assert.equal(error.code, code);
  }
}

export async function assertRejectsWithMessage(action, messageFragment) {
  try {
    await action();
    assert.fail("expected promise to reject");
  } catch (error) {
    assert.ok(error instanceof Error);
    assert.match(error.message, new RegExp(messageFragment));
  }
}
